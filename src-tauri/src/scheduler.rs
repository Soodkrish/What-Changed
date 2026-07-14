use crate::database::Database;
use crate::events::ScanProgressEvent;
use crate::scanner::Scanner;
use crate::storage::StorageAnalyzer;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use tokio::time::{sleep, Duration};

/// RAII guard that resets `scanning` to false on drop (including panic).
struct ScanningGuard(Arc<AtomicBool>);

impl Drop for ScanningGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// Background scheduler that runs periodic scans based on user settings.
/// Reads scan_frequency from DB, runs in a spawned tokio task.
pub struct ScanScheduler {
    db: Arc<Database>,
    app: AppHandle,
    app_data_dir: PathBuf,
    running: Arc<AtomicBool>,
    handle: Mutex<Option<tokio::task::JoinHandle<()>>>,
    scanning: Arc<AtomicBool>,
    crypto: Arc<crate::crypto::CryptoManager>,
}

impl ScanScheduler {
    pub fn new(
        db: Arc<Database>,
        app: AppHandle,
        app_data_dir: PathBuf,
        scanning: Arc<AtomicBool>,
        crypto: Arc<crate::crypto::CryptoManager>,
    ) -> Self {
        ScanScheduler {
            db,
            app,
            app_data_dir,
            running: Arc::new(AtomicBool::new(false)),
            handle: Mutex::new(None),
            scanning,
            crypto,
        }
    }

    /// Get the scan frequency in minutes from DB settings.
    fn get_frequency_minutes(&self) -> u64 {
        self.db
            .get_setting("scan_frequency")
            .ok()
            .flatten()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(15) // default: 15 minutes
    }

    /// Check if auto-scan is enabled (it always is unless user sets to 0).
    fn is_enabled(&self) -> bool {
        self.db
            .get_setting("auto_scan_enabled")
            .ok()
            .flatten()
            .map(|v| v != "false")
            .unwrap_or(true)
    }

    /// Start the periodic scan loop. Idempotent — won't start if already running.
    pub async fn start(&self) {
        if self.running.load(Ordering::SeqCst) {
            return; // already running
        }

        let frequency_minutes = self.get_frequency_minutes();
        let interval = Duration::from_secs(frequency_minutes * 60);

        log::info!(
            "Starting periodic scan scheduler (every {} minutes)",
            frequency_minutes
        );

        self.running.store(true, Ordering::SeqCst);

        let running = self.running.clone();
        let scanning = self.scanning.clone();
        let db = self.db.clone();
        let app = self.app.clone();
        let app_data_dir = self.app_data_dir.clone();
        let crypto = self.crypto.clone();

        let handle = tokio::spawn(async move {
            // Initial delay before first scan (1 minute after startup)
            sleep(Duration::from_secs(60)).await;

            while running.load(Ordering::SeqCst) {
                if !Self::is_enabled_static(&db) {
                    sleep(interval).await;
                    continue;
                }

                // H13: Skip if a manual scan is already in progress
                if scanning.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
                    log::info!("Periodic scan skipped — a scan is already in progress");
                    let new_freq = Self::get_frequency_static(&db);
                    let new_interval = Duration::from_secs(new_freq * 60);
                    sleep(new_interval).await;
                    continue;
                }

                log::info!("Periodic scan triggered");
                // RAII guard: always reset scanning flag, even on panic
                let _guard = ScanningGuard(scanning.clone());
                Self::run_scan(&db, &app, &app_data_dir, &crypto).await;

                // Re-read interval each cycle in case user changed settings
                let new_freq = Self::get_frequency_static(&db);
                let new_interval = Duration::from_secs(new_freq * 60);
                sleep(new_interval).await;
            }

            log::info!("Periodic scan scheduler stopped");
        });

        *self.handle.lock().await = Some(handle);
    }

    /// Stop the periodic scan loop.
    pub async fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(handle) = self.handle.lock().await.take() {
            handle.abort();
        }
        log::info!("Periodic scan scheduler stopped");
    }

    /// Check if scheduler is currently running.
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Restart the scheduler (e.g., when user changes frequency).
    pub async fn restart(&self) {
        self.stop().await;
        if self.is_enabled() {
            self.start().await;
        }
    }

    fn get_frequency_static(db: &Database) -> u64 {
        db.get_setting("scan_frequency")
            .ok()
            .flatten()
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(15)
    }

    fn is_enabled_static(db: &Database) -> bool {
        db.get_setting("auto_scan_enabled")
            .ok()
            .flatten()
            .map(|v| v != "false")
            .unwrap_or(true)
    }

    /// Run a full scan across all monitored folders.
    async fn run_scan(
        db: &Arc<Database>,
        app: &AppHandle,
        app_data_dir: &std::path::Path,
        crypto: &Arc<crate::crypto::CryptoManager>,
    ) {
        let folders = match db.get_monitored_folders() {
            Ok(f) => f,
            Err(e) => {
                log::error!("Failed to get monitored folders: {}", e);
                return;
            }
        };

        let active_folders: Vec<_> = folders.into_iter().filter(|f| f.enabled).collect();
        if active_folders.is_empty() {
            log::info!("No active folders to scan");
            return;
        }

        let total = active_folders.len();
        let scanner = Scanner::new(db.clone());

        // Build folder names string
        let folder_names: Vec<String> = active_folders.iter().map(|f| {
            std::path::Path::new(&f.path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&f.path)
                .to_string()
        }).collect();
        let folders_display = folder_names.join(", ");

        // Create a scan batch
        let batch_id = match db.create_scan_batch(total as i64, &folders_display) {
            Ok(id) => id,
            Err(e) => {
                log::error!("Failed to create scan batch: {}", e);
                return;
            }
        };

        let mut batch_new = 0i64;
        let mut batch_modified = 0i64;
        let mut batch_total_files = 0i64;
        let mut batch_total_size = 0i64;

        for (i, folder) in active_folders.iter().enumerate() {
            let progress = (i as f64 / total as f64 * 100.0) as u32;
            let _ = app.emit("scan-progress", ScanProgressEvent {
                current: i + 1,
                total,
                directory: folder.path.clone(),
                phase: "scanning".to_string(),
                progress_percent: progress,
                files_scanned: 0,
            });

            match scanner.scan_directory(&folder.path) {
                Ok(result) => {
                    batch_new += result.new_files;
                    batch_modified += result.modified_files;
                    batch_total_files += result.files_scanned;
                    batch_total_size += result.total_size;
                }
                Err(e) => log::error!("Periodic scan failed for {}: {}", folder.path, e),
            }
        }

        // Cleanup
        let _ = app.emit("scan-progress", ScanProgressEvent {
            current: total,
            total,
            directory: String::new(),
            phase: "cleanup".to_string(),
            progress_percent: 90,
            files_scanned: 0,
        });

        let _ = scanner.cleanup_deleted();

        // Run recovery subsystems in parallel (independent I/O-bound tasks)
        let db_snap = db.clone();
        let folder_paths: Vec<String> = active_folders.iter().map(|f| f.path.clone()).collect();
        let app_data_dir_snap = app_data_dir.to_path_buf();
        let db_cloud = db.clone();
        let db_rb = db.clone();

        let (snap_result, cloud_result, rb_result) = tokio::join!(
            tokio::task::spawn_blocking(move || {
                let snapshot_mgr = crate::file_snapshots::FileSnapshotManager::new(db_snap, &app_data_dir_snap);
                snapshot_mgr.scan_and_snapshot(&folder_paths)
            }),
            tokio::task::spawn_blocking(move || {
                let detector = crate::cloud_detect::CloudDetector::new(db_cloud);
                detector.detect_cloud_folders()
            }),
            tokio::task::spawn_blocking(move || {
                let rb = crate::recycle_bin::RecycleBinManager::new(db_rb);
                rb.query_and_match()
            }),
        );

        match snap_result {
            Ok(Ok(count)) if count > 0 => log::info!("Created {} file snapshots", count),
            Ok(Err(e)) => log::error!("File snapshot error: {}", e),
            Err(e) => log::error!("File snapshot task panicked: {}", e),
            _ => {}
        }
        match cloud_result {
            Ok(Ok(folders)) if !folders.is_empty() => log::info!("Detected {} cloud folders", folders.len()),
            Ok(Err(e)) => log::error!("Cloud detection error: {}", e),
            Err(e) => log::error!("Cloud detection task panicked: {}", e),
            _ => {}
        }
        match rb_result {
            Ok(Ok(entries)) => log::info!("Recycle bin: {} recoverable files", entries.len()),
            Ok(Err(e)) => log::error!("Recycle bin error: {}", e),
            Err(e) => log::error!("Recycle bin task panicked: {}", e),
        }

        // Snapshot
        let _ = app.emit("scan-progress", ScanProgressEvent {
            current: total,
            total,
            directory: String::new(),
            phase: "snapshot".to_string(),
            progress_percent: 95,
            files_scanned: 0,
        });

        let storage = StorageAnalyzer::new(db.clone());
        let _ = storage.snapshot_all();

        // Cleanup old audit logs (90-day retention) — runs once per scan cycle
        match db.cleanup_old_audit_logs(90) {
            Ok(n) if n > 0 => log::info!("Cleaned up {} old audit log entries", n),
            Err(e) => log::error!("Audit log cleanup failed: {}", e),
            _ => {}
        }

        // Complete the batch
        let stats = db.get_change_stats_today().unwrap_or_default();
        let _ = db.complete_scan_batch(
            batch_id,
            batch_total_files,
            batch_new,
            batch_modified,
            stats.deleted_count,
            stats.moved_count,
            batch_total_size,
        );

        // Fire webhooks for changes in this batch
        match db.get_changes_in_batch(batch_id) {
            Ok(changes) => {
                log::info!("Scheduler webhook check: batch {} has {} changes", batch_id, changes.len());
                if !changes.is_empty() {
                    let event_types: Vec<&str> = changes.iter().map(|c| c.change_type.as_str()).collect();
                    log::info!("Scheduler webhook: event types = {:?}", event_types);
                    let changes_json = serde_json::to_string(&changes).unwrap_or_default();
                    match crate::webhook::fire_webhooks_for_changes(db, crypto, crate::get_http_client(), &changes_json, Some(app_data_dir)).await {
                        Ok(report) => {
                            log::info!("Scheduler webhook result: {} fired, {} failed, errors: {:?}",
                                report.fired, report.failed, report.errors);
                            if report.fired == 0 && report.failed == 0 {
                                log::warn!("Scheduler webhook: no endpoints matched");
                            }
                        }
                        Err(e) => log::error!("Scheduled scan webhook error: {}", e),
                    }
                } else {
                    log::info!("Scheduler webhook: no changes in batch {}", batch_id);
                }
            }
            Err(e) => log::error!("Scheduler webhook: failed to get changes for batch {}: {}", batch_id, e),
        }

        // Complete
        let _ = app.emit("scan-progress", ScanProgressEvent {
            current: total,
            total,
            directory: String::new(),
            phase: "complete".to_string(),
            progress_percent: 100,
            files_scanned: 0,
        });

        // Send desktop notification if enabled
        let notifications_enabled = db
            .get_setting("notifications_enabled")
            .ok()
            .flatten()
            .map(|v| v != "false")
            .unwrap_or(true);

        if notifications_enabled {
            let nm = crate::notifications::NotificationManager::new(db.clone());
            if let Ok(summary) = nm.build_daily_summary() {
                if summary != "No changes detected today." {
                    let _ = app.emit("notification", summary);
                }
            }
        }

        log::info!("Periodic scan completed (batch {})", batch_id);
    }
}
