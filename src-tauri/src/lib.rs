pub mod database;
pub mod scanner;
pub mod duplicates;
pub mod storage;
pub mod notifications;
pub mod tray;
pub mod security;
pub mod scheduler;
pub mod autostart;
pub mod file_snapshots;
pub mod cloud_detect;
pub mod recycle_bin;
pub mod export;
pub mod crypto;
pub mod events;
pub mod webhook;

use database::Database;
use scanner::Scanner;
use duplicates::DuplicateDetector;
use storage::StorageAnalyzer;
use notifications::NotificationManager;
use security::PathValidator;
use scheduler::ScanScheduler;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use tauri::{Manager, Emitter, WindowEvent};
use events::ScanProgressEvent;

/// Global async HTTP client for webhooks.
/// Lives for the entire process lifetime — only ~1MB overhead.
/// Using async Client avoids the tokio runtime panic that reqwest::blocking causes
/// when send() is called from within an existing tokio context.
static ASYNC_HTTP_CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

fn get_http_client() -> &'static reqwest::Client {
    ASYNC_HTTP_CLIENT.get().expect("HTTP client not initialized")
}

/// Log the real error server-side and return a safe user-facing message.
/// Never expose internal paths, DB schema, or stack traces to the frontend.
fn log_and_user_error(context: &str, e: impl std::fmt::Display) -> String {
    log::error!("{}: {}", context, e);
    "Something went wrong. Please try again.".to_string()
}

pub struct AppState {
    pub db: Arc<Database>,
    pub scheduler: Arc<ScanScheduler>,
    pub app_data_dir: std::path::PathBuf,
    pub crypto: Arc<crypto::CryptoManager>,
    /// Guard against overlapping scans (manual + scheduled)
    pub scanning: Arc<AtomicBool>,
}

/// RAII guard that clears the scanning flag on drop (prevents stuck flag on early return)
struct ScanLockGuard<'a> {
    flag: &'a AtomicBool,
}

impl<'a> Drop for ScanLockGuard<'a> {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::SeqCst);
    }
}

#[tauri::command]
fn get_changes_today(state: tauri::State<'_, AppState>) -> Result<Vec<database::ChangeRecord>, String> {
    state.db.get_changes_today().map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn get_changes_range(
    state: tauri::State<'_, AppState>,
    start: String,
    end: String,
) -> Result<Vec<database::ChangeRecord>, String> {
    state.db.get_changes_range(&start, &end).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn get_change_stats_today(state: tauri::State<'_, AppState>) -> Result<database::ChangeStats, String> {
    state.db.get_change_stats_today().map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn scan_directory(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<scanner::ScanResult, String> {
    // Security: validate path
    PathValidator::validate_directory(&path)?;
    let scanner = Scanner::new(state.db.clone());
    scanner.scan_directory(&path)
}

#[tauri::command]
fn scan_all(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<Vec<scanner::ScanResult>, String> {
    // H13: Prevent overlapping scans
    if state.scanning.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
        return Err("A scan is already in progress. Please wait for it to complete.".to_string());
    }
    let _guard = ScanLockGuard { flag: &state.scanning };

    let folders = state.db.get_monitored_folders().map_err(|e| log_and_user_error("command", e))?;
    let active_folders: Vec<_> = folders.into_iter().filter(|f| f.enabled).collect();
    let total = active_folders.len();

    // Build folder names string for the batch
    let folder_names: Vec<String> = active_folders.iter().map(|f| {
        std::path::Path::new(&f.path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&f.path)
            .to_string()
    }).collect();
    let folders_display = folder_names.join(", ");

    // Create a scan batch
    let batch_id = state.db.create_scan_batch(total as i64, &folders_display).map_err(|e| log_and_user_error("command", e))?;

    let scanner = Scanner::new(state.db.clone());
    let mut results = Vec::new();

    let mut batch_new = 0i64;
    let mut batch_modified = 0i64;
    let mut batch_total_files = 0i64;
    let mut batch_total_size = 0i64;

    for (i, folder) in active_folders.iter().enumerate() {
        // Emit progress event to frontend
        let progress = (i as f64 / total as f64 * 100.0) as u32;
        let _ = app.emit("scan-progress", ScanProgressEvent {
            current: i + 1,
            total,
            directory: folder.path.clone(),
            phase: "scanning".to_string(),
            progress_percent: progress,
            files_scanned: 0,
        });

        // Validate path from DB before scanning (defense-in-depth)
        if let Err(e) = PathValidator::validate_directory(&folder.path) {
            log::warn!("Skipping folder with invalid path {}: {}", folder.path, e);
            continue;
        }

        match scanner.scan_directory(&folder.path) {
            Ok(result) => {
                batch_new += result.new_files;
                batch_modified += result.modified_files;
                batch_total_files += result.files_scanned;
                batch_total_size += result.total_size;
                results.push(result);
            }
            Err(e) => log::error!("Failed to scan {}: {}", folder.path, e),
        }
    }

    // Emit cleanup phase
    let _ = app.emit("scan-progress", ScanProgressEvent {
        current: total,
        total,
        directory: String::new(),
        phase: "cleanup".to_string(),
        progress_percent: 90,
        files_scanned: 0,
    });

    let (batch_deleted, batch_moved) = match scanner.cleanup_deleted() {
        Ok(result) => result,
        Err(e) => {
            log::error!("Cleanup failed during scan: {}", e);
            (0, 0)
        }
    };

    // Emit snapshot phase
    let _ = app.emit("scan-progress", ScanProgressEvent {
        current: total,
        total,
        directory: String::new(),
        phase: "snapshot".to_string(),
        progress_percent: 95,
        files_scanned: 0,
    });

    let storage = StorageAnalyzer::new(state.db.clone());
    let _snapshots = storage.snapshot_all()?;

    // Create file content snapshots for diff viewer
    // (only the scheduler did this before — manual scans had zero snapshots)
    {
        let snapshot_mgr = file_snapshots::FileSnapshotManager::new(state.db.clone(), &state.app_data_dir);
        let folder_paths: Vec<String> = active_folders.iter().map(|f| f.path.clone()).collect();
        match snapshot_mgr.scan_and_snapshot(&folder_paths) {
            Ok(n) if n > 0 => log::info!("Manual scan: created {} file snapshots", n),
            Ok(_) => {}
            Err(e) => log::error!("File snapshot error during manual scan: {}", e),
        }
    }

    // Complete the batch
    if let Err(e) = state.db.complete_scan_batch(
        batch_id,
        batch_total_files,
        batch_new,
        batch_modified,
        batch_deleted,
        batch_moved,
        batch_total_size,
    ) {
        log::error!("Failed to complete scan batch {}: {}", batch_id, e);
        let _ = app.emit("scan-progress", ScanProgressEvent {
            current: total,
            total,
            directory: String::new(),
            phase: "batch error".to_string(),
            progress_percent: 100,
            files_scanned: 0,
        });
    }

    // Auto-run duplicate detection after scan
    {
        let detector = DuplicateDetector::new(state.db.clone());
        match detector.detect() {
            Ok(result) if result.groups_found > 0 => {
                log::info!("Duplicate detection: {} groups, {} wasted", result.groups_found, result.wasted_bytes);
            }
            Ok(_) => {}
            Err(e) => log::error!("Duplicate detection failed: {}", e),
        }
    }

    // Emit complete
    let _ = app.emit("scan-progress", ScanProgressEvent {
        current: total,
        total,
        directory: String::new(),
        phase: "complete".to_string(),
        progress_percent: 100,
        files_scanned: batch_total_files,
    });

    // Fire webhooks for changes in this batch
    match state.db.get_changes_in_batch(batch_id) {
        Ok(changes) => {
            log::info!("Webhook check: batch {} has {} changes", batch_id, changes.len());
            if !changes.is_empty() {
                // Log event types for debugging
                let event_types: Vec<&str> = changes.iter().map(|c| c.change_type.as_str()).collect();
                log::info!("Webhook check: event types = {:?}", event_types);
                let changes_json = serde_json::to_string(&changes).unwrap_or_default();
                // Spawn async webhook fire — scan_all is sync, so we need tokio::spawn
                let db = state.db.clone();
                let crypto = state.crypto.clone();
                let app_data_dir = state.app_data_dir.clone();
                tokio::spawn(async move {
                    match webhook::fire_webhooks_for_changes(
                        &db,
                        &crypto,
                        get_http_client(),
                        &changes_json,
                        Some(&app_data_dir),
                    ).await {
                        Ok(report) => {
                            log::info!("Webhook result: {} fired, {} failed, errors: {:?}",
                                report.fired, report.failed, report.errors);
                            if report.fired == 0 && report.failed == 0 {
                                log::warn!("Webhook: no endpoints matched — check webhook event filter settings");
                            }
                        }
                        Err(e) => log::error!("Webhook fire error: {}", e),
                    }
                });
            } else {
                log::info!("Webhook: no changes in batch {} — nothing to fire", batch_id);
            }
        }
        Err(e) => log::error!("Webhook: failed to get changes for batch {}: {}", batch_id, e),
    }

    Ok(results)
}

#[tauri::command]
fn detect_duplicates(
    state: tauri::State<'_, AppState>,
    app: tauri::AppHandle,
) -> Result<duplicates::DuplicateResult, String> {
    let _ = app.emit("scan-progress", ScanProgressEvent {
        current: 1,
        total: 1,
        directory: String::new(),
        phase: "detecting_duplicates".to_string(),
        progress_percent: 50,
        files_scanned: 0,
    });
    let detector = DuplicateDetector::new(state.db.clone());
    let result = detector.detect();
    let _ = app.emit("scan-progress", ScanProgressEvent {
        current: 1,
        total: 1,
        directory: String::new(),
        phase: "complete".to_string(),
        progress_percent: 100,
        files_scanned: 0,
    });
    result
}

#[tauri::command]
fn get_duplicate_groups(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<database::DuplicateGroupRecord>, String> {
    let detector = DuplicateDetector::new(state.db.clone());
    detector.get_groups()
}

#[tauri::command]
fn get_storage_snapshots(
    state: tauri::State<'_, AppState>,
    directory: String,
    days: i64,
) -> Result<Vec<database::SnapshotRecord>, String> {
    let storage = StorageAnalyzer::new(state.db.clone());
    storage.get_growth_history(&directory, days)
}

#[tauri::command]
fn snapshot_directory(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<storage::DirSnapshot, String> {
    PathValidator::validate_directory(&path)?;
    let storage = StorageAnalyzer::new(state.db.clone());
    storage.snapshot_directory(&path)
}

#[tauri::command]
fn add_monitored_folder(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<i64, String> {
    PathValidator::validate_directory(&path)?;
    state.db.add_monitored_folder(&path).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn remove_monitored_folder(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    state.db.remove_monitored_folder(&path).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn get_monitored_folders(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<database::MonitoredFolder>, String> {
    state.db.get_monitored_folders().map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn toggle_monitored_folder(
    state: tauri::State<'_, AppState>,
    id: i64,
    enabled: bool,
) -> Result<(), String> {
    state.db.toggle_monitored_folder(id, enabled).map_err(|e| log_and_user_error("command", e))
}

/// Settings keys that are safe to expose to the frontend.
const READABLE_SETTINGS: &[&str] = &[
    "scan_frequency", "auto_scan_enabled",
    "file_snapshots_enabled", "snapshot_max_size", "snapshot_retention_days", "snapshot_extensions",
    "notifications_enabled", "daily_summary_enabled", "start_minimized",
    "cloud_detection_enabled", "autostart_enabled", "dark_mode",
    "webhook_latest_report",
    "daily_summary_webhook_enabled", "daily_summary_time",
];

#[tauri::command]
fn get_setting(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<Option<String>, String> {
    if !READABLE_SETTINGS.contains(&key.as_str()) {
        return Err("Unknown setting".to_string());
    }
    state.db.get_setting(&key).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn set_setting(
    state: tauri::State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    // Allowlist of permitted settings keys
    const ALLOWED_SETTINGS: &[&str] = &[
        "scan_frequency", "auto_scan_enabled",
        "file_snapshots_enabled", "snapshot_max_size", "snapshot_retention_days", "snapshot_extensions",
        "notifications_enabled", "daily_summary_enabled", "start_minimized",
        "cloud_detection_enabled", "autostart_enabled", "dark_mode",
        "daily_summary_webhook_enabled", "daily_summary_time",
    ];
    if !ALLOWED_SETTINGS.contains(&key.as_str()) {
        return Err(format!("Unknown setting: {}", key));
    }

    // Per-key validation
    match key.as_str() {
        "scan_frequency" => {
            let mins: u64 = value.parse().map_err(|_| "Must be a positive integer".to_string())?;
            if mins < 1 || mins > 1440 {
                return Err("Frequency must be between 1 and 1440 minutes".into());
            }
        }
        "snapshot_max_size" => {
            let size: i64 = value.parse().map_err(|_| "Must be a positive integer".to_string())?;
            if size < 1024 || size > 10_000_000 {
                return Err("Max size must be between 1KB and 10MB".into());
            }
        }
        "snapshot_retention_days" => {
            let days: i64 = value.parse().map_err(|_| "Must be a positive integer".to_string())?;
            if days < 1 || days > 365 {
                return Err("Retention must be between 1 and 365 days".into());
            }
        }
        "snapshot_extensions" => {
            // Validate: only allow comma-separated alphanumeric extensions
            for ext in value.split(',') {
                let ext = ext.trim().trim_start_matches('.');
                if ext.is_empty() || !ext.chars().all(|c| c.is_alphanumeric() || c == '_') || ext.len() > 20 {
                    return Err(format!("Invalid extension: '{}'. Use comma-separated extensions (e.g., txt,rs,py)", ext));
                }
            }
        }
        _ => {}
    }

    state.db.set_setting(&key, &value).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn get_all_settings(
    state: tauri::State<'_, AppState>,
) -> Result<std::collections::HashMap<String, String>, String> {
    let all = state.db.get_all_settings().map_err(|e| log_and_user_error("command", e))?;
    // Only return settings the frontend should see
    Ok(all.into_iter()
        .filter(|(k, _)| READABLE_SETTINGS.iter().any(|s| *s == k.as_str()))
        .collect())
}

#[tauri::command]
fn get_daily_summary(
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let nm = NotificationManager::new(state.db.clone());
    nm.build_daily_summary()
}

#[tauri::command]
fn get_scan_batches(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<database::ScanBatchWithChanges>, String> {
    state.db.get_all_batches_with_changes().map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
async fn restart_scheduler(
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    state.scheduler.restart().await;
    Ok(())
}

#[tauri::command]
async fn get_scheduler_status(
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    Ok(state.scheduler.is_running())
}

#[tauri::command]
fn enable_autostart() -> Result<(), String> {
    autostart::enable_autostart()
}

#[tauri::command]
fn disable_autostart() -> Result<(), String> {
    autostart::disable_autostart()
}

#[tauri::command]
fn is_autostart_enabled() -> Result<bool, String> {
    Ok(autostart::is_autostart_enabled())
}

#[tauri::command]
fn open_in_explorer(path: String) -> Result<(), String> {
    // Validate path with PathValidator for security (files or directories)
    PathValidator::validate_file_path(&path)?;

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open explorer: {}", e))?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("Failed to open file manager: {}", e))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .args(["-R", &path])
            .spawn()
            .map_err(|e| format!("Failed to open Finder: {}", e))?;
    }
    Ok(())
}

// --- Recovery commands ---

#[tauri::command]
fn refresh_recycle_bin(state: tauri::State<'_, AppState>) -> Result<Vec<database::RecycleBinEntry>, String> {
    let manager = recycle_bin::RecycleBinManager::new(state.db.clone());
    manager.query_and_match()
}

#[tauri::command]
fn get_recoverable_files(state: tauri::State<'_, AppState>) -> Result<Vec<database::RecycleBinEntry>, String> {
    let manager = recycle_bin::RecycleBinManager::new(state.db.clone());
    manager.get_recoverable_files()
}

#[tauri::command]
fn restore_from_recycle_bin(state: tauri::State<'_, AppState>, entry_id: i64) -> Result<String, String> {
    let manager = recycle_bin::RecycleBinManager::new(state.db.clone());
    manager.restore_file(entry_id)
}

#[tauri::command]
fn enable_file_snapshots(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.db.set_setting("file_snapshots_enabled", "true").map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn disable_file_snapshots(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.db.set_setting("file_snapshots_enabled", "false").map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn get_snapshots_for_file(state: tauri::State<'_, AppState>, path: String) -> Result<Vec<database::FileSnapshotRecord>, String> {
    state.db.get_snapshots_for_file(&path).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn restore_file_snapshot(state: tauri::State<'_, AppState>, snapshot_id: i64) -> Result<String, String> {
    let manager = file_snapshots::FileSnapshotManager::new(state.db.clone(), &state.app_data_dir);
    manager.restore_file_snapshot(snapshot_id)
}

#[tauri::command]
fn save_snapshot_to_file(state: tauri::State<'_, AppState>, snapshot_id: i64, dest_path: String) -> Result<String, String> {
    // Validate destination path — security checks that allow non-existent files
    if dest_path.trim().is_empty() {
        return Err("Destination path cannot be empty".into());
    }
    if dest_path.len() > 260 {
        return Err("Path too long (max 260 characters)".into());
    }
    let normalized = dest_path.replace('\\', "/");
    if normalized.contains("/../") || normalized.ends_with("/..") || normalized == ".." {
        return Err("Path traversal detected".into());
    }
    if dest_path.contains('\0') || dest_path.contains('`') || dest_path.contains('$') {
        return Err("Invalid characters in path".into());
    }
    if dest_path.starts_with("\\\\") || dest_path.starts_with("//") {
        return Err("Network paths are not allowed".into());
    }

    // Get snapshot info from DB
    let snapshot = state.db.get_file_snapshot_by_id(snapshot_id)
        .map_err(|e| format!("DB error: {}", e))?
        .ok_or_else(|| "Snapshot not found".to_string())?;

    // Read and decompress the snapshot
    let snapshot_file = std::path::Path::new(&snapshot.snapshot_path);
    if !snapshot_file.exists() {
        return Err("Snapshot file missing from disk".to_string());
    }
    let compressed = std::fs::read(snapshot_file)
        .map_err(|e| format!("Failed to read snapshot: {}", e))?;
    let decompressed = file_snapshots::FileSnapshotManager::decompress_with_limit(&compressed, 10 * 1024 * 1024)?;

    // Verify integrity
    if let Some(ref expected_hash) = snapshot.file_hash {
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(&decompressed);
        let actual = format!("{:x}", hasher.finalize());
        if actual != *expected_hash {
            return Err("Snapshot integrity check failed — file may be corrupted".to_string());
        }
    }

    // Create parent directories if needed
    let dest = std::path::Path::new(&dest_path);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).ok();
    }

    // Write to disk
    std::fs::write(dest, &decompressed)
        .map_err(|e| format!("Failed to write file: {}", e))?;

    state.db.log_recovery_action(
        "snapshot_download",
        Some(&serde_json::json!({"snapshot_id": snapshot_id, "dest": dest_path}).to_string()),
        true,
        None,
    ).ok();

    Ok(dest_path)
}

#[tauri::command]
fn get_snapshot_content(state: tauri::State<'_, AppState>, snapshot_id: i64) -> Result<String, String> {
    state.db.get_snapshot_content(snapshot_id)
        .map_err(|e| log_and_user_error("command", e))?
        .ok_or_else(|| "Snapshot not found or content unavailable".to_string())
}

#[tauri::command]
fn get_file_content(state: tauri::State<'_, AppState>, path: String) -> Result<String, String> {
    // Validate path with PathValidator for security (files, not just directories)
    PathValidator::validate_file_path(&path)?;
    // Enforce monitored-directory restriction — only allow reading files under monitored folders
    let folders = state.db.get_monitored_folders().map_err(|e| log_and_user_error("command", e))?;
    let under_monitored = folders.iter().any(|f| path.starts_with(&f.path));
    if !under_monitored {
        return Err("File is not within a monitored directory".into());
    }
    // Validate path exists
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err("File does not exist".into());
    }
    state.db.get_file_content(&path)
        .map_err(|e| log_and_user_error("command", e))?
        .ok_or_else(|| "File is binary or cannot be read as text".to_string())
}

#[tauri::command]
fn cleanup_old_snapshots(state: tauri::State<'_, AppState>, keep_days: i64) -> Result<i64, String> {
    // Use the DB methods directly with the caller-specified retention,
    // bypassing the settings-based get_retention_days() in scan_and_snapshot
    let old_paths = state.db.get_old_snapshot_paths(keep_days).map_err(|e| log_and_user_error("command", e))?;
    let mut deleted_files = 0i64;
    for path in &old_paths {
        let p = std::path::Path::new(path);
        if p.exists() && std::fs::remove_file(p).is_ok() {
            deleted_files += 1;
        }
    }
    let db_deleted = state.db.cleanup_old_file_snapshots(keep_days).map_err(|e| log_and_user_error("command", e))?;
    state.db.log_recovery_action(
        "cleanup_snapshots",
        Some(&serde_json::json!({"files_deleted": deleted_files, "db_deleted": db_deleted}).to_string()),
        true,
        None,
    ).ok();
    Ok(db_deleted)
}

#[tauri::command]
fn get_snapshots_grouped_by_file(state: tauri::State<'_, AppState>) -> Result<Vec<database::SnapshotFileGroup>, String> {
    state.db.get_snapshots_grouped_by_file().map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn get_snapshot_stats(state: tauri::State<'_, AppState>) -> Result<(i64, i64), String> {
    state.db.get_file_snapshot_stats().map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn detect_cloud_folders(state: tauri::State<'_, AppState>) -> Result<Vec<database::CloudFolder>, String> {
    let detector = cloud_detect::CloudDetector::new(state.db.clone());
    detector.detect_cloud_folders()
}

#[tauri::command]
fn get_cloud_folders(state: tauri::State<'_, AppState>) -> Result<Vec<database::CloudFolder>, String> {
    let detector = cloud_detect::CloudDetector::new(state.db.clone());
    detector.get_cloud_folders()
}

#[tauri::command]
fn is_cloud_backed(state: tauri::State<'_, AppState>, path: String) -> Result<Option<String>, String> {
    let detector = cloud_detect::CloudDetector::new(state.db.clone());
    Ok(detector.is_cloud_backed(&path))
}

#[tauri::command]
fn export_daily_report(state: tauri::State<'_, AppState>, date: String, format: String) -> Result<String, String> {
    // Validate date format is YYYY-MM-DD
    if !date.chars().all(|c| c.is_ascii_digit() || c == '-') || date.len() != 10 {
        return Err("Date must be in YYYY-MM-DD format".to_string());
    }
    let parts: Vec<&str> = date.split('-').collect();
    if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
        return Err("Date must be in YYYY-MM-DD format".to_string());
    }
    if let (Ok(y), Ok(m), Ok(d)) = (parts[0].parse::<i32>(), parts[1].parse::<i32>(), parts[2].parse::<i32>()) {
        if y < 2000 || y > 2100 || m < 1 || m > 12 || d < 1 || d > 31 {
            return Err("Date values out of range".to_string());
        }
    } else {
        return Err("Date must be valid numbers".to_string());
    }

    let exporter = export::ReportExporter::new(state.db.clone());
    match format.as_str() {
        "json" => exporter.export_daily_json(&date),
        "csv" => exporter.export_daily_csv(&date),
        _ => Err(format!("Unknown format: {}", format)),
    }
}

#[tauri::command]
fn get_recovery_stats(state: tauri::State<'_, AppState>) -> Result<database::RecoveryStats, String> {
    state.db.get_recovery_stats().map_err(|e| log_and_user_error("command", e))
}

// --- Ignore Pattern commands ---

#[tauri::command]
fn add_ignore_pattern(state: tauri::State<'_, AppState>, folder_id: i64, pattern: String, pattern_type: String) -> Result<i64, String> {
    // Validate pattern_type to only accept known types
    let valid_types = ["glob", "regex", "contains"];
    if !valid_types.contains(&pattern_type.as_str()) {
        return Err(format!("Invalid pattern type '{}'. Must be one of: glob, regex, contains", pattern_type));
    }
    state.db.add_ignore_pattern(folder_id, &pattern, &pattern_type).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn remove_ignore_pattern(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    state.db.remove_ignore_pattern(id).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn get_ignore_patterns(state: tauri::State<'_, AppState>, folder_id: Option<i64>) -> Result<Vec<database::IgnorePattern>, String> {
    match folder_id {
        Some(fid) => state.db.get_ignore_patterns_for_folder(fid).map_err(|e| log_and_user_error("command", e)),
        None => state.db.get_all_ignore_patterns().map_err(|e| log_and_user_error("command", e)),
    }
}

// --- Snapshot Tag commands ---

#[tauri::command]
fn add_snapshot_tag(state: tauri::State<'_, AppState>, snapshot_id: i64, name: String, description: Option<String>, color: Option<String>) -> Result<i64, String> {
    state.db.add_snapshot_tag(snapshot_id, &name, description.as_deref(), color.as_deref()).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn remove_snapshot_tag(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    state.db.remove_snapshot_tag(id).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn get_tags_for_snapshot(state: tauri::State<'_, AppState>, snapshot_id: i64) -> Result<Vec<database::SnapshotTag>, String> {
    state.db.get_tags_for_snapshot(snapshot_id).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn get_all_tags(state: tauri::State<'_, AppState>) -> Result<Vec<database::SnapshotTag>, String> {
    state.db.get_all_tags().map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn compare_snapshots(state: tauri::State<'_, AppState>, snapshot_a_id: i64, snapshot_b_id: i64) -> Result<Option<(String, String)>, String> {
    state.db.compare_snapshots(snapshot_a_id, snapshot_b_id).map_err(|e| log_and_user_error("command", e))
}

// --- Workspace Profile commands ---

#[tauri::command]
fn create_profile(state: tauri::State<'_, AppState>, name: String) -> Result<i64, String> {
    state.db.create_profile(&name).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn delete_profile(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    state.db.delete_profile(id).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn get_all_profiles(state: tauri::State<'_, AppState>) -> Result<Vec<database::WorkspaceProfile>, String> {
    state.db.get_all_profiles().map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn activate_profile(state: tauri::State<'_, AppState>, profile_id: i64) -> Result<(), String> {
    state.db.activate_profile(profile_id).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn save_current_folders_to_profile(state: tauri::State<'_, AppState>, profile_id: i64) -> Result<(), String> {
    state.db.save_current_folders_to_profile(profile_id).map_err(|e| log_and_user_error("command", e))
}

// --- File History commands ---

#[tauri::command]
fn get_file_history(state: tauri::State<'_, AppState>, file_path: String) -> Result<Vec<database::ChangeRecord>, String> {
    state.db.get_file_history(&file_path).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn search_changes(state: tauri::State<'_, AppState>, query: String, limit: Option<i64>) -> Result<Vec<database::ChangeRecord>, String> {
    if query.len() > 1000 {
        return Err("Search query too long (max 1000 characters)".into());
    }
    state.db.search_changes(&query, limit.unwrap_or(100)).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn get_activity_heatmap(state: tauri::State<'_, AppState>, days: Option<i64>) -> Result<Vec<database::HeatmapEntry>, String> {
    state.db.get_activity_heatmap(days.unwrap_or(90)).map_err(|e| log_and_user_error("command", e))
}

// ==================== PHASE 2 COMMANDS ====================

#[tauri::command]
fn create_notification_profile(state: tauri::State<'_, AppState>, name: String, quiet_hours_start: Option<i64>, quiet_hours_end: Option<i64>) -> Result<i64, String> {
    state.db.create_notification_profile(&name, quiet_hours_start.unwrap_or(0), quiet_hours_end.unwrap_or(0)).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn delete_notification_profile(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    state.db.delete_notification_profile(id).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn get_all_notification_profiles(state: tauri::State<'_, AppState>) -> Result<Vec<database::NotificationProfile>, String> {
    state.db.get_all_notification_profiles().map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn update_notification_profile(
    state: tauri::State<'_, AppState>,
    id: i64,
    quiet_hours_start: Option<i64>,
    quiet_hours_end: Option<i64>,
    notify_new: Option<bool>,
    notify_modified: Option<bool>,
    notify_deleted: Option<bool>,
    notify_moved: Option<bool>,
    enabled: Option<bool>,
) -> Result<(), String> {
    state.db.update_notification_profile(id, quiet_hours_start, quiet_hours_end, notify_new, notify_modified, notify_deleted, notify_moved, enabled).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn set_notification_profile_folders(state: tauri::State<'_, AppState>, profile_id: i64, folder_ids: Vec<i64>) -> Result<(), String> {
    state.db.set_notification_profile_folders(profile_id, &folder_ids).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn get_folders_for_notification_profile(state: tauri::State<'_, AppState>, profile_id: i64) -> Result<Vec<database::MonitoredFolder>, String> {
    state.db.get_folders_for_notification_profile(profile_id).map_err(|e| log_and_user_error("command", e))
}

/// Check if an IP address is disallowed (SSRF protection).
/// Returns true for localhost, private ranges, and link-local addresses.
/// Properly handles IPv4-mapped IPv6 (e.g. ::ffff:127.0.0.1).
fn validate_webhook_url(url: &str) -> Result<(), String> {
    // Reject excessively long URLs (H22 / M51)
    if url.len() > 2048 {
        return Err("URL too long (max 2048 characters)".into());
    }
    let parsed = url::Url::parse(url).map_err(|e| format!("Invalid URL: {}", e))?;
    // Only allow http/https (SSRF prevention)
    if parsed.scheme() != "https" && parsed.scheme() != "http" {
        return Err("Only HTTP and HTTPS URLs are allowed".into());
    }
    // Reject URLs with embedded credentials (SSRF + credential leak)
    if parsed.username() != "" || parsed.password().is_some() {
        return Err("URLs with embedded credentials are not allowed".into());
    }
    // Reject empty host
    let host = parsed.host_str().ok_or("URL must have a hostname")?;
    if host.is_empty() {
        return Err("URL must have a valid hostname".into());
    }
    // Reject private/loopback IPs (direct IP check)
    if webhook::is_ip_disallowed(host) {
        return Err("Webhook URLs cannot target localhost, private, or link-local addresses".into());
    }
    // Also resolve DNS at creation time to catch hostname-based SSRF (DNS rebinding)
    // This closes the TOCTOU window between creation and first fire.
    match webhook::verify_dns_at_fire_time(url) {
        Ok(true) => {} // All resolved IPs are safe
        Ok(false) => {
            return Err("Webhook hostname resolves to a private/loopback address (DNS rebinding detected)".into());
        }
        Err(e) => {
            log::warn!("DNS resolution failed during webhook validation: {}", e);
            return Err(format!("Cannot resolve webhook hostname: {}", e));
        }
    }
    Ok(())
}

#[tauri::command]
fn create_webhook_endpoint(state: tauri::State<'_, AppState>, name: String, url: String, events: Option<String>, secret: Option<String>) -> Result<i64, String> {
    validate_webhook_url(&url)?;

    // Validate events field (case-insensitive)
    let valid_event_types = ["ALL", "NEW", "MODIFIED", "DELETED", "MOVED"];
    let events_upper = events.unwrap_or_else(|| "ALL".to_string()).to_uppercase();
    if events_upper != "ALL" {
        for ev in events_upper.split(',') {
            let ev = ev.trim();
            if !valid_event_types.contains(&ev) {
                return Err(format!("Invalid event type '{}'. Valid types: ALL, NEW, MODIFIED, DELETED, MOVED", ev));
            }
        }
    }

    // Encrypt secret before storing
    let encrypted_secret = match secret {
        Some(ref s) if !s.is_empty() => {
            Some(state.crypto.encrypt(s).map_err(|e| format!("Encryption failed: {}", e))?)
        }
        _ => None,
    };

    state.db.create_webhook_endpoint(&name, &url, &events_upper, encrypted_secret.as_deref()).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn delete_webhook_endpoint(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    state.db.delete_webhook_endpoint(id).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn get_all_webhook_endpoints(state: tauri::State<'_, AppState>) -> Result<Vec<database::WebhookEndpointSafe>, String> {
    let endpoints = state.db.get_all_webhook_endpoints().map_err(|e| log_and_user_error("command", e))?;
    Ok(endpoints.into_iter().map(database::WebhookEndpointSafe::from).collect())
}

#[tauri::command]
fn toggle_webhook_endpoint(state: tauri::State<'_, AppState>, id: i64, enabled: bool) -> Result<(), String> {
    state.db.toggle_webhook_endpoint(id, enabled).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
async fn test_webhook_endpoint(state: tauri::State<'_, AppState>, id: i64) -> Result<i64, String> {
    let endpoints = state.db.get_all_webhook_endpoints().map_err(|e| log_and_user_error("command", e))?;
    let endpoint = endpoints.iter().find(|e| e.id == id).ok_or("Webhook not found")?;
    let url = endpoint.url.clone();

    // DNS re-resolve check (mitigates DNS rebinding attack)
    match webhook::verify_dns_at_fire_time(&url) {
        Ok(true) => {}
        Ok(false) => {
            log::warn!("Skipping webhook test {} — DNS rebinding detected", endpoint.name);
            let _ = state.db.update_webhook_trigger(id, 0);
            return Ok(0);
        }
        Err(e) => {
            log::warn!("Skipping webhook test {} — DNS resolution failed: {}", endpoint.name, e);
            let _ = state.db.update_webhook_trigger(id, 0);
            return Ok(0);
        }
    }

    // Decrypt secret if present
    let plaintext_secret = match endpoint.secret {
        Some(ref s) => Some(state.crypto.decrypt(s).map_err(|e|
            format!("Failed to decrypt webhook secret: {}", e)
        )?),
        None => None,
    };

    // Build platform-specific test payload
    let url_lower = url.to_lowercase();
    let is_telegram = url_lower.contains("api.telegram.org");
    let payload = if is_telegram {
        // Extract chat_id from URL query params
        let chat_id = url::Url::parse(&url)
            .ok()
            .and_then(|u| u.query_pairs().find(|(k, _)| k == "chat_id").map(|(_, v)| v.to_string()))
            .unwrap_or_default();
        let mut p = serde_json::json!({
            "text": "🧪 What Changed? webhook test ping",
            "parse_mode": "Markdown",
        });
        if !chat_id.is_empty() {
            p["chat_id"] = serde_json::json!(chat_id);
        }
        p
    } else {
        serde_json::json!({
            "content": "🧪 What Changed? webhook test ping",
            "username": "What Changed?",
            "event": "test",
            "message": "What Changed? webhook test ping",
            "timestamp": chrono::Local::now().to_rfc3339(),
        })
    };

    let body = payload.to_string();
    let mut req = get_http_client().post(&url)
        .header("Content-Type", "application/json")
        .header("User-Agent", "WhatChanged/1.0")
        .header("X-Webhook-App", "What Changed?")
        .body(body.clone());

    if let Some(ref s) = plaintext_secret {
        let sig = webhook::compute_signature(s, &body);
        req = req.header("X-Webhook-Signature", format!("sha256={}", sig));
    }

    let status = match req.send().await {
        Ok(r) => r.status().as_u16() as i64,
        Err(e) => {
            log::error!("Webhook test failed for {}: {}", url, e);
            let _ = state.db.update_webhook_trigger(id, 0);
            return Err(format!("Webhook test failed: {}", e));
        }
    };

    state.db.update_webhook_trigger(id, status).map_err(|e| log_and_user_error("command", e))?;
    Ok(status)
}

#[derive(serde::Serialize)]
struct WebhookFireReport {
    triggered_ids: Vec<i64>,
    failures: Vec<WebhookFireFailure>,
}

#[derive(serde::Serialize)]
struct WebhookFireFailure {
    endpoint_id: i64,
    endpoint_name: String,
    endpoint_url: String,
    reason: String,
}

#[tauri::command]
async fn fire_webhook_for_changes(state: tauri::State<'_, AppState>, changes_json: String) -> Result<WebhookFireReport, String> {
    // Reject unbounded payloads
    if changes_json.len() > 1_000_000 {
        return Err("Changes payload too large (max 1MB)".into());
    }
    let changes: Vec<database::ChangeRecord> = serde_json::from_str(&changes_json).map_err(|e| log_and_user_error("command", e))?;
    let mut report = WebhookFireReport {
        triggered_ids: Vec::new(),
        failures: Vec::new(),
    };

    for change in &changes {
        let endpoints = state.db.get_active_webhooks_for_event(&change.change_type).map_err(|e| log_and_user_error("command", e))?;
        for endpoint in endpoints {
            // DNS re-resolve check (mitigates DNS rebinding attack)
            match webhook::verify_dns_at_fire_time(&endpoint.url) {
                Ok(true) => {}
                Ok(false) => {
                    log::warn!("Skipping webhook {} — DNS rebinding detected", endpoint.name);
                    let _ = state.db.update_webhook_trigger(endpoint.id, 0);
                    report.failures.push(WebhookFireFailure {
                        endpoint_id: endpoint.id,
                        endpoint_name: endpoint.name.clone(),
                        endpoint_url: endpoint.url.clone(),
                        reason: "DNS rebinding detected".to_string(),
                    });
                    continue;
                }
                Err(e) => {
                    log::warn!("Skipping webhook {} — DNS resolution failed: {}", endpoint.name, e);
                    let _ = state.db.update_webhook_trigger(endpoint.id, 0);
                    report.failures.push(WebhookFireFailure {
                        endpoint_id: endpoint.id,
                        endpoint_name: endpoint.name.clone(),
                        endpoint_url: endpoint.url.clone(),
                        reason: format!("DNS resolution failed: {}", e),
                    });
                    continue;
                }
            }

            // Decrypt secret if present
            let plaintext_secret = match endpoint.secret {
                Some(ref s) => match state.crypto.decrypt(s) {
                    Ok(plain) => Some(plain),
                    Err(e) => {
                        log::error!("Failed to decrypt webhook secret for {}: {}", endpoint.name, e);
                        report.failures.push(WebhookFireFailure {
                            endpoint_id: endpoint.id,
                            endpoint_name: endpoint.name.clone(),
                            endpoint_url: endpoint.url.clone(),
                            reason: format!("Secret decryption failed: {}", e),
                        });
                        continue;
                    }
                },
                None => None,
            };

            let payload = serde_json::json!({
                "event": change.change_type.to_lowercase(),
                "file": change.file_path,
                "filename": change.filename,
                "previous_path": change.previous_path,
                "new_path": change.new_path,
                "detected_at": change.detected_at,
                "timestamp": chrono::Local::now().to_rfc3339(),
            });

            let body = payload.to_string();
            let mut req = get_http_client().post(&endpoint.url)
                .header("Content-Type", "application/json")
                .header("User-Agent", "WhatChanged/1.0")
                .header("X-Webhook-App", "What Changed?")
                .header("X-Webhook-Event", &change.change_type)
                .body(body.clone());

            if let Some(ref s) = plaintext_secret {
                let sig = webhook::compute_signature(s, &body);
                req = req.header("X-Webhook-Signature", format!("sha256={}", sig));
            }

            let status = match req.send().await {
                Ok(r) => r.status().as_u16() as i64,
                Err(e) => {
                    log::error!("Webhook fire failed for {}: {}", endpoint.url, e);
                    let _ = state.db.update_webhook_trigger(endpoint.id, 0);
                    report.failures.push(WebhookFireFailure {
                        endpoint_id: endpoint.id,
                        endpoint_name: endpoint.name.clone(),
                        endpoint_url: endpoint.url.clone(),
                        reason: format!("Request failed: {}", e),
                    });
                    continue;
                }
            };
            let _ = state.db.update_webhook_trigger(endpoint.id, status);
            if status >= 200 && status < 300 {
                report.triggered_ids.push(endpoint.id);
            } else {
                report.failures.push(WebhookFireFailure {
                    endpoint_id: endpoint.id,
                    endpoint_name: endpoint.name.clone(),
                    endpoint_url: endpoint.url.clone(),
                    reason: format!("HTTP status {}", status),
                });
            }
        }
    }
    Ok(report)
}

#[tauri::command]
fn get_blame_data(state: tauri::State<'_, AppState>, file_path: String) -> Result<Vec<database::BlameLine>, String> {
    // No PathValidator here — blame reads entirely from snapshot data in the DB.
    // The file may have been deleted or moved; that's exactly when blame is useful.
    state.db.get_blame_data(&file_path).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn get_changelog_entries(state: tauri::State<'_, AppState>, limit: Option<i64>) -> Result<Vec<database::ChangelogEntry>, String> {
    state.db.get_changelog_entries(limit.unwrap_or(30)).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn generate_changelog_markdown(state: tauri::State<'_, AppState>, limit: Option<i64>) -> Result<String, String> {
    let entries = state.db.get_changelog_entries(limit.unwrap_or(30)).map_err(|e| log_and_user_error("command", e))?;
    let mut md = String::from("# Changelog\n\nGenerated by What Changed?\n\n");

    for entry in &entries {
        let date = &entry.date;
        md.push_str(&format!("## {} (Scan #{})\n\n", date, entry.batch_id));
        md.push_str(&format!("Folders: `{}`\n\n", entry.folders_scanned));
        md.push_str(&format!(
            "| Metric | Count |\n|--------|-------|\n| Files Scanned | {} |\n| New | {} |\n| Modified | {} |\n| Deleted | {} |\n| Moved | {} |\n\n",
            entry.total_files, entry.new_files, entry.modified_files, entry.deleted_files, entry.moved_files
        ));

        if !entry.changes.is_empty() {
            md.push_str("### Changes\n\n");
            for c in &entry.changes {
                let icon = match c.change_type.as_str() {
                    "NEW" => "🆕",
                    "MODIFIED" => "📝",
                    "DELETED" => "🗑️",
                    "MOVED" => "📦",
                    _ => "📄",
                };
                md.push_str(&format!("- {} `{}`", icon, c.filename));
                if c.change_type == "MOVED" {
                    if let Some(ref prev) = c.previous_path {
                        md.push_str(&format!(" ← `{}`", prev));
                    }
                }
                md.push('\n');
            }
            md.push('\n');
        }
    }

    Ok(md)
}

#[tauri::command]
fn compare_any_snapshots(state: tauri::State<'_, AppState>, id_a: i64, id_b: i64) -> Result<Option<(String, String, String, String)>, String> {
    state.db.compare_any_snapshots(id_a, id_b).map_err(|e| log_and_user_error("command", e))
}

// ==================== PHASE 3 COMMANDS ====================

#[tauri::command]
fn get_extension_stats(state: tauri::State<'_, AppState>) -> Result<Vec<database::ExtensionStat>, String> {
    state.db.get_extension_stats().map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn get_daily_trends(state: tauri::State<'_, AppState>, days: Option<i64>) -> Result<Vec<database::DailyTrend>, String> {
    state.db.get_daily_trends(days.unwrap_or(90)).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn advanced_search(
    state: tauri::State<'_, AppState>,
    query: Option<String>,
    change_type: Option<String>,
    date_from: Option<String>,
    date_to: Option<String>,
    extension: Option<String>,
    min_size: Option<i64>,
    max_size: Option<i64>,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<database::AdvancedSearchResult, String> {
    state.db.advanced_search(
        query.as_deref(),
        change_type.as_deref(),
        date_from.as_deref(),
        date_to.as_deref(),
        extension.as_deref(),
        min_size,
        max_size,
        limit.unwrap_or(50),
        offset.unwrap_or(0),
    ).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn get_export_data(state: tauri::State<'_, AppState>) -> Result<database::ExportData, String> {
    state.db.get_export_data().map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn export_changes_csv(
    state: tauri::State<'_, AppState>,
    date_from: Option<String>,
    date_to: Option<String>,
) -> Result<String, String> {
    state.db.export_changes_csv(date_from.as_deref(), date_to.as_deref()).map_err(|e| log_and_user_error("command", e))
}

#[tauri::command]
fn generate_html_report(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let data = state.db.get_export_data().map_err(|e| log_and_user_error("command", e))?;

    let mut html = String::from("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"UTF-8\">\n<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n<title>What Changed? Report</title>\n<style>\n");
    html.push_str("  * { margin: 0; padding: 0; box-sizing: border-box; }\n");
    html.push_str("  body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #f8fafc; color: #1e293b; padding: 2rem; }\n");
    html.push_str("  .header { text-align: center; margin-bottom: 2rem; }\n");
    html.push_str("  .header h1 { font-size: 2rem; color: #4f46e5; }\n");
    html.push_str("  .header p { color: #64748b; margin-top: 0.5rem; }\n");
    html.push_str("  .cards { display: grid; grid-template-columns: repeat(4, 1fr); gap: 1rem; margin-bottom: 2rem; }\n");
    html.push_str("  .card { background: white; border-radius: 12px; padding: 1.5rem; box-shadow: 0 1px 3px rgba(0,0,0,0.1); }\n");
    html.push_str("  .card .value { font-size: 2rem; font-weight: 700; }\n");
    html.push_str("  .card .label { color: #64748b; font-size: 0.875rem; margin-top: 0.25rem; }\n");
    html.push_str("  .card.new .value { color: #10b981; }\n");
    html.push_str("  .card.mod .value { color: #3b82f6; }\n");
    html.push_str("  .card.del .value { color: #ef4444; }\n");
    html.push_str("  .card.mov .value { color: #f59e0b; }\n");
    html.push_str("  table { width: 100%; border-collapse: collapse; background: white; border-radius: 12px; overflow: hidden; box-shadow: 0 1px 3px rgba(0,0,0,0.1); margin-bottom: 2rem; }\n");
    html.push_str("  th, td { padding: 0.75rem 1rem; text-align: left; border-bottom: 1px solid #e2e8f0; }\n");
    html.push_str("  th { background: #f1f5f9; font-weight: 600; color: #475569; font-size: 0.875rem; }\n");
    html.push_str("  td { font-size: 0.875rem; }\n");
    html.push_str("  .badge { display: inline-block; padding: 2px 8px; border-radius: 9999px; font-size: 0.75rem; font-weight: 600; }\n");
    html.push_str("  .badge.new { background: #d1fae5; color: #065f46; }\n");
    html.push_str("  .badge.mod { background: #dbeafe; color: #1e40af; }\n");
    html.push_str("  .badge.del { background: #fee2e2; color: #991b1b; }\n");
    html.push_str("  .badge.mov { background: #fef3c7; color: #92400e; }\n");
    html.push_str("  .section { margin-bottom: 2rem; }\n");
    html.push_str("  .section h2 { font-size: 1.25rem; margin-bottom: 1rem; color: #334155; }\n");
    html.push_str("  .footer { text-align: center; color: #94a3b8; font-size: 0.75rem; margin-top: 2rem; }\n");
    html.push_str("</style>\n</head>\n<body>\n");
    html.push_str("<div class=\"header\">\n");
    html.push_str("  <h1>What Changed? Report</h1>\n");
    html.push_str(&format!("  <p>Generated: {}</p>\n", data.generated_at));
    html.push_str("</div>\n\n");

    // Stats cards
    html.push_str("<div class=\"cards\">\n");
    html.push_str(&format!("  <div class=\"card new\"><div class=\"value\">{}</div><div class=\"label\">New Files</div></div>\n", data.summary.new_count));
    html.push_str(&format!("  <div class=\"card mod\"><div class=\"value\">{}</div><div class=\"label\">Modified</div></div>\n", data.summary.modified_count));
    html.push_str(&format!("  <div class=\"card del\"><div class=\"value\">{}</div><div class=\"label\">Deleted</div></div>\n", data.summary.deleted_count));
    html.push_str(&format!("  <div class=\"card mov\"><div class=\"value\">{}</div><div class=\"label\">Moved</div></div>\n", data.summary.moved_count));
    html.push_str("</div>\n\n");

    // Extension stats
    if !data.extension_stats.is_empty() {
        html.push_str("<div class=\"section\">\n<h2>📁 Files by Type</h2>\n<table>\n<tr><th>Extension</th><th>Count</th><th>Total Size</th></tr>\n");
        for ext in &data.extension_stats {
            let size_str = format_bytes_rust(ext.total_size);
            html.push_str(&format!("<tr><td><code>{}</code></td><td>{}</td><td>{}</td></tr>\n", html_escape(&ext.extension), ext.count, size_str));
        }
        html.push_str("</table>\n</div>\n\n");
    }

    // Changes table
    html.push_str("<div class=\"section\">\n<h2>📋 Recent Changes</h2>\n<table>\n<tr><th>Type</th><th>File</th><th>Path</th><th>When</th></tr>\n");
    for batch in &data.batches {
        for c in &batch.changes {
            let badge_class = match c.change_type.as_str() {
                "NEW" => "new",
                "MODIFIED" => "mod",
                "DELETED" => "del",
                "MOVED" => "mov",
                _ => "",
            };
            let short_path: String = c.file_path.chars().rev().take(60).collect::<String>().chars().rev().collect();
            let safe_type = html_escape(&c.change_type);
            let safe_filename = html_escape(&c.filename);
            let safe_path = html_escape(&c.file_path);
            let safe_short = html_escape(&short_path);
            html.push_str(&format!(
                "<tr><td><span class=\"badge {}\">{}</span></td><td>{}</td><td title=\"{}\">...{}</td><td>{}</td></tr>\n",
                badge_class, safe_type, safe_filename, safe_path, safe_short, html_escape(&c.detected_at)
            ));
        }
    }
    html.push_str("</table>\n</div>\n\n");

    html.push_str("<div class=\"footer\">Report generated by What Changed? — File System Monitor</div>\n</body>\n</html>");

    Ok(html)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
     .replace('<', "&lt;")
     .replace('>', "&gt;")
     .replace('"', "&quot;")
     .replace('\'', "&#x27;")
}

fn format_bytes_rust(bytes: i64) -> String {
    if bytes <= 0 { return "0 B".to_string(); }
    let k: f64 = 1024.0;
    let sizes = ["B", "KB", "MB", "GB", "TB"];
    let i = ((bytes as f64).ln() / k.ln()).floor() as usize;
    let i = i.min(sizes.len() - 1);
    format!("{:.2} {}", bytes as f64 / k.powi(i as i32), sizes[i])
}

// --- Update Checker ---

/// GitHub repository to check for updates.
/// TODO: Replace with your actual GitHub username/repo before first release.
const GITHUB_REPO_OWNER: &str = "OWNER";
const GITHUB_REPO_NAME: &str = "REPO";

#[derive(serde::Serialize)]
struct UpdateInfo {
    has_update: bool,
    current_version: String,
    latest_version: String,
    download_url: String,
    release_notes: String,
}

/// Diagnostic command: trace the entire webhook pipeline and return a human-readable report.
/// The frontend can call this and display the results, no terminal needed.
#[tauri::command]
fn diagnose_webhooks(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let mut lines: Vec<String> = Vec::new();
    lines.push("=== WEBHOOK DIAGNOSTIC ===".into());

    // 1. List all endpoints
    let all_endpoints = state.db.get_all_webhook_endpoints().map_err(|e| e.to_string())?;
    lines.push(format!("Total endpoints in DB: {}", all_endpoints.len()));
    for ep in &all_endpoints {
        lines.push(format!(
            "  [id={}] name='{}' events='{}' enabled={} url={}",
            ep.id, ep.name, ep.events, ep.enabled, ep.url
        ));
    }

    // 2. Check latest scan batch
    match state.db.get_latest_scan_batch() {
        Some(batch) => {
            lines.push(format!("Latest batch: id={} completed={:?}", batch.id, batch.completed_at));
            match state.db.get_changes_in_batch(batch.id) {
                Ok(changes) => {
                    lines.push(format!("  Changes in batch: {}", changes.len()));
                    for c in &changes {
                        lines.push(format!("    change_type='{}' file='{}'", c.change_type, c.filename));
                    }
                }
                Err(e) => lines.push(format!("  Error getting changes: {}", e)),
            }
        }
        None => lines.push("No scan batches found".into()),
    }

    // 3. Test event matching for each change type
    for event_type in &["NEW", "MODIFIED", "DELETED", "MOVED"] {
        match state.db.get_active_webhooks_for_event(event_type) {
            Ok(matched) => {
                lines.push(format!("Event '{}' matches {} endpoints", event_type, matched.len()));
                for ep in &matched {
                    lines.push(format!("  -> {} (url={})", ep.name, ep.url));
                }
            }
            Err(e) => lines.push(format!("Event '{}' match error: {}", event_type, e)),
        }
    }

    // 4. DNS check each endpoint URL
    for ep in &all_endpoints {
        match webhook::verify_dns_at_fire_time(&ep.url) {
            Ok(true) => lines.push(format!("DNS OK for {} ({})", ep.name, ep.url)),
            Ok(false) => lines.push(format!("DNS BLOCKED for {} — private/loopback IP!", ep.name)),
            Err(e) => lines.push(format!("DNS FAIL for {}: {}", ep.name, e)),
        }
    }

    // 5. Write debug log file path
    let debug_log_path = state.app_data_dir.join("webhook_debug.log");
    lines.push(format!("Debug log file: {}", debug_log_path.display()));
    if debug_log_path.exists() {
        match std::fs::read_to_string(&debug_log_path) {
            Ok(content) => {
                let tail: Vec<&str> = content.lines().rev().take(20).collect();
                lines.push(format!("Last {} lines of debug log:", tail.len()));
                for line in tail.into_iter().rev() {
                    lines.push(format!("  {}", line));
                }
            }
            Err(e) => lines.push(format!("Could not read debug log: {}", e)),
        }
    } else {
        lines.push("No debug log file yet (no webhooks have fired)".into());
    }

    Ok(lines.join("\n"))
}

/// Check GitHub Releases API for a newer version.
/// Returns UpdateInfo with comparison result. Fails gracefully if offline.
#[tauri::command]
async fn check_for_updates() -> Result<UpdateInfo, String> {
    let current_version = env!("CARGO_PKG_VERSION");

    let client = get_http_client();
    let url = format!(
        "https://api.github.com/repos/{}/{}/releases/latest",
        GITHUB_REPO_OWNER, GITHUB_REPO_NAME
    );

    let response = match client
        .get(&url)
        .header("User-Agent", "WhatChanged-Updater/1.0")
        .header("Accept", "application/vnd.github.v3+json")
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            log::warn!("Update check failed (network): {}", e);
            return Ok(UpdateInfo {
                has_update: false,
                current_version: current_version.to_string(),
                latest_version: String::new(),
                download_url: String::new(),
                release_notes: String::new(),
            });
        }
    };

    if !response.status().is_success() {
        log::warn!("Update check failed (status {})", response.status());
        return Ok(UpdateInfo {
            has_update: false,
            current_version: current_version.to_string(),
            latest_version: String::new(),
            download_url: String::new(),
            release_notes: String::new(),
        });
    }

    let text = match response.text().await {
        Ok(t) => t,
        Err(e) => {
            log::warn!("Update check failed (read body): {}", e);
            return Ok(UpdateInfo {
                has_update: false,
                current_version: current_version.to_string(),
                latest_version: String::new(),
                download_url: String::new(),
                release_notes: String::new(),
            });
        }
    };

    let body: serde_json::Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("Update check failed (parse): {}", e);
            return Ok(UpdateInfo {
                has_update: false,
                current_version: current_version.to_string(),
                latest_version: String::new(),
                download_url: String::new(),
                release_notes: String::new(),
            });
        }
    };

    let tag_name = body.get("tag_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim_start_matches('v')
        .to_string();

    let release_notes = body.get("body")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let html_url = body.get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let has_update = compare_versions(&tag_name, current_version);

    log::info!(
        "Update check: current={}, latest={}, has_update={}",
        current_version, tag_name, has_update
    );

    Ok(UpdateInfo {
        has_update,
        current_version: current_version.to_string(),
        latest_version: tag_name,
        download_url: html_url,
        release_notes,
    })
}

/// Compare two semver strings. Returns true if `latest` is newer than `current`.
fn compare_versions(latest: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u32> {
        v.split('.')
            .filter_map(|s| s.parse::<u32>().ok())
            .collect()
    };
    let latest_parts = parse(latest);
    let current_parts = parse(current);

    for (l, c) in latest_parts.iter().zip(current_parts.iter()) {
        if l > c { return true; }
        if l < c { return false; }
    }
    latest_parts.len() > current_parts.len()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Show info-level logs by default; override with RUST_LOG env var if needed.
    let _ = env_logger::Builder::from_default_env()
        .filter_level(log::LevelFilter::Info)
        .is_test(false)
        .try_init();

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .on_window_event(|window, event| {
            // Intercept close: destroy the webview (frees ~100MB Chromium renderer).
            // Tray "Show" will recreate the window on demand.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let _ = window.emit("close-warning", ());
                api.prevent_close();
                if let Err(e) = window.destroy() {
                    log::warn!("Failed to destroy window: {}", e);
                }
            }
        })
        .setup(|app| {
            // Set Windows AppUserModelId so Task Manager shows "What Changed?"
            // instead of "WebView" as the process name. This groups all child
            // processes (WebView2 renderer, GPU rasterizer) under our app name.
            #[cfg(target_os = "windows")]
            {
                #[link(name = "shell32")]
                extern "system" {
                    fn SetCurrentProcessExplicitAppUserModelID(
                        appId: *const u16,
                    ) -> i32;
                }
                let wide: Vec<u16> = "com.whatchanged.app"
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                unsafe {
                    SetCurrentProcessExplicitAppUserModelID(wide.as_ptr());
                }
            }

            let app_dir = app
                .path()
                .app_data_dir()
                .expect("Failed to get app data dir");
            std::fs::create_dir_all(&app_dir).ok();

            let db_path = app_dir.join("whatchanged.db");
            let db = Database::new(&db_path).expect("Failed to initialize database");
            let db = Arc::new(db);

            let handle = app.handle().clone();
            let scanning = Arc::new(AtomicBool::new(false));

            let crypto_manager = crypto::CryptoManager::new(&app_dir)
                .expect("Failed to initialize encryption");
            let crypto = Arc::new(crypto_manager);

            // One-time migration: encrypt any existing plaintext webhook secrets
            if let Err(e) = db.migrate_plaintext_secrets(
                &|s| crypto::CryptoManager::is_encrypted(s),
                &|plaintext| crypto.encrypt(plaintext),
            ) {
                log::warn!("Webhook secret migration warning: {}", e);
            }

            // Shared async HTTP client — uses reqwest::Client (NOT blocking) to avoid
            // the tokio runtime panic that reqwest::blocking causes when send() is
            // called from within an existing tokio context.
            let _ = ASYNC_HTTP_CLIENT.set(
                reqwest::Client::builder()
                    .timeout(std::time::Duration::from_secs(10))
                    .pool_max_idle_per_host(4)
                    .build()
                    .expect("Failed to create HTTP client")
            );

            let scheduler = Arc::new(ScanScheduler::new(
                db.clone(), handle, app_dir.clone(), scanning.clone(),
                crypto.clone(),
            ));

            app.manage(AppState {
                db: db.clone(),
                scheduler: scheduler.clone(),
                app_data_dir: app_dir,
                crypto: crypto.clone(),
                scanning,
            });

            // Start periodic scan scheduler
            {
                let scheduler = scheduler.clone();
                tauri::async_runtime::spawn(async move {
                    scheduler.start().await;
                });
            }

            // Create system tray (non-fatal: window should still show if tray fails)
            if let Err(e) = tray::create_tray(app.handle()) {
                log::warn!("Failed to create system tray: {}", e);
            }

            // Check for --minimized flag (from auto-start)
            let start_minimized = std::env::args().any(|arg| arg == "--minimized");
            let setting_minimized = db
                .get_setting("start_minimized")
                .ok()
                .flatten()
                .map(|v| v == "true")
                .unwrap_or(false);

            // Detect dev mode (tauri dev sets TAURI_ENV_DEBUG=1 or args contain "tauri-dev")
            #[cfg(debug_assertions)]
            let is_dev = true;
            #[cfg(not(debug_assertions))]
            let is_dev = std::env::args().any(|a| a.contains("tauri"));

            // Replace config-created window with memory-optimized version.
            // tauri.conf.json windows can't carry additional_browser_args,
            // so destroy the initial window and rebuild with WebView2 flags
            // that save ~10-20MB GPU overhead.
            let should_show = is_dev || (!start_minimized && !setting_minimized);
            // Always destroy the config-created window
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.destroy();
            }
            // Rebuild with optimized Chromium args
            match tray::build_main_window(app.handle(), None) {
                Ok(w) => {
                    if should_show {
                        let _ = w.show();
                        let _ = w.set_focus();
                    } else {
                        let _ = w.hide();
                    }
                }
                Err(e) => log::warn!("Failed to create optimized window: {}", e),
            }

            // Sync autostart: reconcile DB setting with actual OS registry state
            let os_autostart = autostart::is_autostart_enabled();
            let db_autostart = db.get_setting("autostart_enabled")
                .ok().flatten().map(|v| v == "true").unwrap_or(false);
            if os_autostart != db_autostart {
                log::info!(
                    "Autostart sync: OS={}, DB={} → reconciling DB to match OS",
                    os_autostart, db_autostart
                );
                let _ = db.set_setting("autostart_enabled", &os_autostart.to_string());
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_changes_today,
            get_changes_range,
            get_change_stats_today,
            scan_directory,
            scan_all,
            detect_duplicates,
            get_duplicate_groups,
            get_storage_snapshots,
            snapshot_directory,
            add_monitored_folder,
            remove_monitored_folder,
            get_monitored_folders,
            toggle_monitored_folder,
            get_setting,
            set_setting,
            get_all_settings,
            get_daily_summary,
            get_scan_batches,
            restart_scheduler,
            get_scheduler_status,
            enable_autostart,
            disable_autostart,
            is_autostart_enabled,
            open_in_explorer,
            refresh_recycle_bin,
            get_recoverable_files,
            restore_from_recycle_bin,
            enable_file_snapshots,
            disable_file_snapshots,
            get_snapshots_for_file,
            restore_file_snapshot,
            save_snapshot_to_file,
            get_snapshot_content,
            get_file_content,
            get_snapshots_grouped_by_file,
            get_snapshot_stats,
            cleanup_old_snapshots,
            detect_cloud_folders,
            get_cloud_folders,
            is_cloud_backed,
            export_daily_report,
            get_recovery_stats,
            add_ignore_pattern,
            remove_ignore_pattern,
            get_ignore_patterns,
            add_snapshot_tag,
            remove_snapshot_tag,
            get_tags_for_snapshot,
            get_all_tags,
            compare_snapshots,
            create_profile,
            delete_profile,
            get_all_profiles,
            activate_profile,
            save_current_folders_to_profile,
            get_file_history,
            search_changes,
            get_activity_heatmap,
            create_notification_profile,
            delete_notification_profile,
            get_all_notification_profiles,
            update_notification_profile,
            set_notification_profile_folders,
            get_folders_for_notification_profile,
            create_webhook_endpoint,
            delete_webhook_endpoint,
            get_all_webhook_endpoints,
            toggle_webhook_endpoint,
            test_webhook_endpoint,
            fire_webhook_for_changes,
            get_blame_data,
            get_changelog_entries,
            generate_changelog_markdown,
            compare_any_snapshots,
            get_extension_stats,
            get_daily_trends,
            advanced_search,
            get_export_data,
            export_changes_csv,
            generate_html_report,
            check_for_updates,
            diagnose_webhooks,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app_handle, event| {
            // Prevent app from exiting when the last window is destroyed.
            // The tray icon + scheduler keep the app alive for background scanning.
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                api.prevent_exit();
            }
        });
}
