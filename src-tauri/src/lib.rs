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

use database::Database;
use scanner::Scanner;
use duplicates::DuplicateDetector;
use storage::StorageAnalyzer;
use notifications::NotificationManager;
use security::PathValidator;
use scheduler::ScanScheduler;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{Manager, Emitter, WindowEvent};
use sha2::Digest;
use events::ScanProgressEvent;

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
    state.db.get_changes_today().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_changes_range(
    state: tauri::State<'_, AppState>,
    start: String,
    end: String,
) -> Result<Vec<database::ChangeRecord>, String> {
    state.db.get_changes_range(&start, &end).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_change_stats_today(state: tauri::State<'_, AppState>) -> Result<database::ChangeStats, String> {
    state.db.get_change_stats_today().map_err(|e| e.to_string())
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

    let folders = state.db.get_monitored_folders().map_err(|e| e.to_string())?;
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
    let batch_id = state.db.create_scan_batch(total as i64, &folders_display).map_err(|e| e.to_string())?;

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
            phase: format!("batch error: {}", e),
            progress_percent: 100,
            files_scanned: 0,
        });
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
    state.db.add_monitored_folder(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn remove_monitored_folder(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<(), String> {
    state.db.remove_monitored_folder(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_monitored_folders(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<database::MonitoredFolder>, String> {
    state.db.get_monitored_folders().map_err(|e| e.to_string())
}

#[tauri::command]
fn toggle_monitored_folder(
    state: tauri::State<'_, AppState>,
    id: i64,
    enabled: bool,
) -> Result<(), String> {
    state.db.toggle_monitored_folder(id, enabled).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_setting(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<Option<String>, String> {
    state.db.get_setting(&key).map_err(|e| e.to_string())
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
        _ => {}
    }

    state.db.set_setting(&key, &value).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_all_settings(
    state: tauri::State<'_, AppState>,
) -> Result<std::collections::HashMap<String, String>, String> {
    state.db.get_all_settings().map_err(|e| e.to_string())
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
    state.db.get_all_batches_with_changes().map_err(|e| e.to_string())
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
    state.db.set_setting("file_snapshots_enabled", "true").map_err(|e| e.to_string())
}

#[tauri::command]
fn disable_file_snapshots(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.db.set_setting("file_snapshots_enabled", "false").map_err(|e| e.to_string())
}

#[tauri::command]
fn get_snapshots_for_file(state: tauri::State<'_, AppState>, path: String) -> Result<Vec<database::FileSnapshotRecord>, String> {
    state.db.get_snapshots_for_file(&path).map_err(|e| e.to_string())
}

#[tauri::command]
fn restore_file_snapshot(state: tauri::State<'_, AppState>, snapshot_id: i64) -> Result<String, String> {
    let manager = file_snapshots::FileSnapshotManager::new(state.db.clone(), &state.app_data_dir);
    manager.restore_file_snapshot(snapshot_id)
}

#[tauri::command]
fn get_snapshot_content(state: tauri::State<'_, AppState>, snapshot_id: i64) -> Result<String, String> {
    state.db.get_snapshot_content(snapshot_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Snapshot not found or content unavailable".to_string())
}

#[tauri::command]
fn get_file_content(state: tauri::State<'_, AppState>, path: String) -> Result<String, String> {
    // Validate path with PathValidator for security (files, not just directories)
    PathValidator::validate_file_path(&path)?;
    // Validate path exists
    let p = std::path::Path::new(&path);
    if !p.exists() {
        return Err("File does not exist".into());
    }
    state.db.get_file_content(&path)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "File is binary or cannot be read as text".to_string())
}

#[tauri::command]
fn cleanup_old_snapshots(state: tauri::State<'_, AppState>, keep_days: i64) -> Result<i64, String> {
    // Use the DB methods directly with the caller-specified retention,
    // bypassing the settings-based get_retention_days() in scan_and_snapshot
    let old_paths = state.db.get_old_snapshot_paths(keep_days).map_err(|e| e.to_string())?;
    let mut deleted_files = 0i64;
    for path in &old_paths {
        let p = std::path::Path::new(path);
        if p.exists() && std::fs::remove_file(p).is_ok() {
            deleted_files += 1;
        }
    }
    let db_deleted = state.db.cleanup_old_file_snapshots(keep_days).map_err(|e| e.to_string())?;
    state.db.log_recovery_action(
        "cleanup_snapshots",
        Some(&serde_json::json!({"files_deleted": deleted_files, "db_deleted": db_deleted}).to_string()),
        true,
        None,
    ).ok();
    Ok(db_deleted)
}

#[tauri::command]
fn get_snapshot_stats(state: tauri::State<'_, AppState>) -> Result<(i64, i64), String> {
    state.db.get_file_snapshot_stats().map_err(|e| e.to_string())
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
    state.db.get_recovery_stats().map_err(|e| e.to_string())
}

// --- Ignore Pattern commands ---

#[tauri::command]
fn add_ignore_pattern(state: tauri::State<'_, AppState>, folder_id: i64, pattern: String, pattern_type: String) -> Result<i64, String> {
    // Validate pattern_type to only accept known types
    let valid_types = ["glob", "regex", "contains"];
    if !valid_types.contains(&pattern_type.as_str()) {
        return Err(format!("Invalid pattern type '{}'. Must be one of: glob, regex, contains", pattern_type));
    }
    state.db.add_ignore_pattern(folder_id, &pattern, &pattern_type).map_err(|e| e.to_string())
}

#[tauri::command]
fn remove_ignore_pattern(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    state.db.remove_ignore_pattern(id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_ignore_patterns(state: tauri::State<'_, AppState>, folder_id: Option<i64>) -> Result<Vec<database::IgnorePattern>, String> {
    match folder_id {
        Some(fid) => state.db.get_ignore_patterns_for_folder(fid).map_err(|e| e.to_string()),
        None => state.db.get_all_ignore_patterns().map_err(|e| e.to_string()),
    }
}

// --- Snapshot Tag commands ---

#[tauri::command]
fn add_snapshot_tag(state: tauri::State<'_, AppState>, snapshot_id: i64, name: String, description: Option<String>, color: Option<String>) -> Result<i64, String> {
    state.db.add_snapshot_tag(snapshot_id, &name, description.as_deref(), color.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
fn remove_snapshot_tag(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    state.db.remove_snapshot_tag(id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_tags_for_snapshot(state: tauri::State<'_, AppState>, snapshot_id: i64) -> Result<Vec<database::SnapshotTag>, String> {
    state.db.get_tags_for_snapshot(snapshot_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_all_tags(state: tauri::State<'_, AppState>) -> Result<Vec<database::SnapshotTag>, String> {
    state.db.get_all_tags().map_err(|e| e.to_string())
}

#[tauri::command]
fn compare_snapshots(state: tauri::State<'_, AppState>, snapshot_a_id: i64, snapshot_b_id: i64) -> Result<Option<(String, String)>, String> {
    state.db.compare_snapshots(snapshot_a_id, snapshot_b_id).map_err(|e| e.to_string())
}

// --- Workspace Profile commands ---

#[tauri::command]
fn create_profile(state: tauri::State<'_, AppState>, name: String) -> Result<i64, String> {
    state.db.create_profile(&name).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_profile(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    state.db.delete_profile(id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_all_profiles(state: tauri::State<'_, AppState>) -> Result<Vec<database::WorkspaceProfile>, String> {
    state.db.get_all_profiles().map_err(|e| e.to_string())
}

#[tauri::command]
fn activate_profile(state: tauri::State<'_, AppState>, profile_id: i64) -> Result<(), String> {
    state.db.activate_profile(profile_id).map_err(|e| e.to_string())
}

#[tauri::command]
fn save_current_folders_to_profile(state: tauri::State<'_, AppState>, profile_id: i64) -> Result<(), String> {
    state.db.save_current_folders_to_profile(profile_id).map_err(|e| e.to_string())
}

// --- File History commands ---

#[tauri::command]
fn get_file_history(state: tauri::State<'_, AppState>, file_path: String) -> Result<Vec<database::ChangeRecord>, String> {
    state.db.get_file_history(&file_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn search_changes(state: tauri::State<'_, AppState>, query: String, limit: Option<i64>) -> Result<Vec<database::ChangeRecord>, String> {
    state.db.search_changes(&query, limit.unwrap_or(100)).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_activity_heatmap(state: tauri::State<'_, AppState>, days: Option<i64>) -> Result<Vec<database::HeatmapEntry>, String> {
    state.db.get_activity_heatmap(days.unwrap_or(90)).map_err(|e| e.to_string())
}

// ==================== PHASE 2 COMMANDS ====================

#[tauri::command]
fn create_notification_profile(state: tauri::State<'_, AppState>, name: String, quiet_hours_start: Option<i64>, quiet_hours_end: Option<i64>) -> Result<i64, String> {
    state.db.create_notification_profile(&name, quiet_hours_start.unwrap_or(0), quiet_hours_end.unwrap_or(0)).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_notification_profile(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    state.db.delete_notification_profile(id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_all_notification_profiles(state: tauri::State<'_, AppState>) -> Result<Vec<database::NotificationProfile>, String> {
    state.db.get_all_notification_profiles().map_err(|e| e.to_string())
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
    state.db.update_notification_profile(id, quiet_hours_start, quiet_hours_end, notify_new, notify_modified, notify_deleted, notify_moved, enabled).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_notification_profile_folders(state: tauri::State<'_, AppState>, profile_id: i64, folder_ids: Vec<i64>) -> Result<(), String> {
    state.db.set_notification_profile_folders(profile_id, &folder_ids).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_folders_for_notification_profile(state: tauri::State<'_, AppState>, profile_id: i64) -> Result<Vec<database::MonitoredFolder>, String> {
    state.db.get_folders_for_notification_profile(profile_id).map_err(|e| e.to_string())
}

/// Check if an IP address is disallowed (SSRF protection).
/// Returns true for localhost, private ranges, and link-local addresses.
/// Properly handles IPv4-mapped IPv6 (e.g. ::ffff:127.0.0.1).
fn is_ip_disallowed(ip: &str) -> bool {
    // Parse the IP string — if it's IPv6, try to extract IPv4-mapped
    let normalized: std::net::IpAddr = match ip.parse() {
        Ok(addr) => addr,
        Err(_) => return true, // unparseable = disallow
    };

    let ipv4: std::net::Ipv4Addr = match normalized {
        std::net::IpAddr::V4(v4) => v4,
        std::net::IpAddr::V6(v6) => {
            // Convert IPv4-mapped IPv6 (::ffff:x.x.x.x) to IPv4
            match v6.to_ipv4_mapped() {
                Some(v4) => v4,
                None => {
                    // Pure IPv6 — block loopback and any unspecified
                    return v6.is_loopback() || v6.is_unspecified();
                }
            }
        }
    };

    ipv4.is_loopback()
        || ipv4.is_unspecified()
        || ipv4.is_link_local()
        || ipv4.is_private()
}

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
    // Reject private/loopback IPs
    if is_ip_disallowed(host) {
        return Err("Webhook URLs cannot target localhost, private, or link-local addresses".into());
    }
    Ok(())
}

/// Re-resolve a URL's hostname at fire time and verify all resolved IPs are allowed.
/// Returns Ok(true) if safe, Ok(false) if any IP is disallowed (DNS rebinding detected).
fn verify_dns_at_fire_time(url: &str) -> Result<bool, String> {
    let parsed = url::Url::parse(url)
        .map_err(|e| format!("Invalid URL: {}", e))?;
    let host = parsed.host_str()
        .ok_or_else(|| "No hostname in URL".to_string())?;
    let port = parsed.port_or_known_default().unwrap_or(80);
    let addr = format!("{}:{}", host, port);

    use std::net::ToSocketAddrs;
    match addr.to_socket_addrs() {
        Ok(addrs) => {
            for addr in addrs {
                let ip = addr.ip().to_string();
                if is_ip_disallowed(&ip) {
                    log::warn!(
                        "DNS rebinding detected for {}: resolved to {} (disallowed)",
                        url, ip
                    );
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Err(e) => {
            log::warn!("DNS resolution failed for {}: {}", host, e);
            Err(format!("Cannot resolve hostname: {}", e))
        }
    }
}

#[tauri::command]
fn create_webhook_endpoint(state: tauri::State<'_, AppState>, name: String, url: String, events: Option<String>, secret: Option<String>) -> Result<i64, String> {
    validate_webhook_url(&url)?;

    // Validate events field
    let valid_event_types = ["ALL", "NEW", "MODIFIED", "DELETED", "MOVED"];
    let events_str = events.unwrap_or_else(|| "ALL".to_string());
    if events_str != "ALL" {
        for ev in events_str.split(',') {
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

    state.db.create_webhook_endpoint(&name, &url, &events_str, encrypted_secret.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
fn delete_webhook_endpoint(state: tauri::State<'_, AppState>, id: i64) -> Result<(), String> {
    state.db.delete_webhook_endpoint(id).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_all_webhook_endpoints(state: tauri::State<'_, AppState>) -> Result<Vec<database::WebhookEndpointSafe>, String> {
    let endpoints = state.db.get_all_webhook_endpoints().map_err(|e| e.to_string())?;
    Ok(endpoints.into_iter().map(database::WebhookEndpointSafe::from).collect())
}

#[tauri::command]
fn toggle_webhook_endpoint(state: tauri::State<'_, AppState>, id: i64, enabled: bool) -> Result<(), String> {
    state.db.toggle_webhook_endpoint(id, enabled).map_err(|e| e.to_string())
}

#[tauri::command]
fn test_webhook_endpoint(state: tauri::State<'_, AppState>, id: i64) -> Result<i64, String> {
    let endpoints = state.db.get_all_webhook_endpoints().map_err(|e| e.to_string())?;
    let endpoint = endpoints.iter().find(|e| e.id == id).ok_or("Webhook not found")?;
    let url = endpoint.url.clone();

    // DNS re-resolve check (mitigates DNS rebinding attack)
    match verify_dns_at_fire_time(&url) {
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

    // Build test payload
    let payload = serde_json::json!({
        "event": "test",
        "message": "What Changed? webhook test ping",
        "timestamp": chrono::Local::now().to_rfc3339(),
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()
        .map_err(|e| e.to_string())?;

    let mut req = client.post(&url)
        .header("Content-Type", "application/json")
        .header("User-Agent", "WhatChanged/1.0")
        .body(payload.to_string());

    if let Some(ref s) = plaintext_secret {
        let sig = format!("{:x}", sha2::Sha256::digest(s.as_bytes()));
        req = req.header("X-Webhook-Secret", sig);
    }

    let status = req.send()
        .map(|r| r.status().as_u16() as i64)
        .unwrap_or(0);

    state.db.update_webhook_trigger(id, status).map_err(|e| e.to_string())?;
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
fn fire_webhook_for_changes(state: tauri::State<'_, AppState>, changes_json: String) -> Result<WebhookFireReport, String> {
    // Reject unbounded payloads
    if changes_json.len() > 1_000_000 {
        return Err("Changes payload too large (max 1MB)".into());
    }
    let changes: Vec<database::ChangeRecord> = serde_json::from_str(&changes_json).map_err(|e| e.to_string())?;
    let mut report = WebhookFireReport {
        triggered_ids: Vec::new(),
        failures: Vec::new(),
    };

    for change in &changes {
        let endpoints = state.db.get_active_webhooks_for_event(&change.change_type).map_err(|e| e.to_string())?;
        for endpoint in endpoints {
            // DNS re-resolve check (mitigates DNS rebinding attack)
            match verify_dns_at_fire_time(&endpoint.url) {
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

            let client = reqwest::blocking::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build();
            if let Ok(client) = client {
                let mut req = client.post(&endpoint.url)
                    .header("Content-Type", "application/json")
                    .header("User-Agent", "WhatChanged/1.0")
                    .body(payload.to_string());

                if let Some(ref s) = plaintext_secret {
                    let sig = format!("{:x}", sha2::Sha256::digest(s.as_bytes()));
                    req = req.header("X-Webhook-Secret", sig);
                }

                let status = req.send().map(|r| r.status().as_u16() as i64).unwrap_or(0);
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
            } else {
                report.failures.push(WebhookFireFailure {
                    endpoint_id: endpoint.id,
                    endpoint_name: endpoint.name.clone(),
                    endpoint_url: endpoint.url.clone(),
                    reason: "Failed to create HTTP client".to_string(),
                });
            }
        }
    }
    Ok(report)
}

#[tauri::command]
fn get_blame_data(state: tauri::State<'_, AppState>, file_path: String) -> Result<Vec<database::BlameLine>, String> {
    PathValidator::validate_file_path(&file_path)?;
    // Guard against OOM on large files — blame is O(n*m) memory
    const MAX_BLAME_FILE_SIZE: u64 = 1_000_000; // 1MB
    if let Ok(meta) = std::fs::metadata(&file_path) {
        if meta.len() > MAX_BLAME_FILE_SIZE {
            return Err(format!(
                "File too large for blame analysis ({} bytes, max {}). Use a smaller file.",
                meta.len(),
                MAX_BLAME_FILE_SIZE
            ));
        }
    }
    state.db.get_blame_data(&file_path).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_changelog_entries(state: tauri::State<'_, AppState>, limit: Option<i64>) -> Result<Vec<database::ChangelogEntry>, String> {
    state.db.get_changelog_entries(limit.unwrap_or(30)).map_err(|e| e.to_string())
}

#[tauri::command]
fn generate_changelog_markdown(state: tauri::State<'_, AppState>, limit: Option<i64>) -> Result<String, String> {
    let entries = state.db.get_changelog_entries(limit.unwrap_or(30)).map_err(|e| e.to_string())?;
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
    state.db.compare_any_snapshots(id_a, id_b).map_err(|e| e.to_string())
}

// ==================== PHASE 3 COMMANDS ====================

#[tauri::command]
fn get_extension_stats(state: tauri::State<'_, AppState>) -> Result<Vec<database::ExtensionStat>, String> {
    state.db.get_extension_stats().map_err(|e| e.to_string())
}

#[tauri::command]
fn get_daily_trends(state: tauri::State<'_, AppState>, days: Option<i64>) -> Result<Vec<database::DailyTrend>, String> {
    state.db.get_daily_trends(days.unwrap_or(90)).map_err(|e| e.to_string())
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
    ).map_err(|e| e.to_string())
}

#[tauri::command]
fn get_export_data(state: tauri::State<'_, AppState>) -> Result<database::ExportData, String> {
    state.db.get_export_data().map_err(|e| e.to_string())
}

#[tauri::command]
fn export_changes_csv(
    state: tauri::State<'_, AppState>,
    date_from: Option<String>,
    date_to: Option<String>,
) -> Result<String, String> {
    state.db.export_changes_csv(date_from.as_deref(), date_to.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
fn generate_html_report(state: tauri::State<'_, AppState>) -> Result<String, String> {
    let data = state.db.get_export_data().map_err(|e| e.to_string())?;

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    env_logger::init();

    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .on_window_event(|window, event| {
            // Intercept close: hide to tray instead of quitting
            if let WindowEvent::CloseRequested { api, .. } = event {
                if let Err(e) = window.hide() {
                    log::warn!("Failed to hide window: {}", e);
                }
                let _ = window.emit("close-warning", ());
                api.prevent_close();
            }
        })
        .setup(|app| {
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
            let scheduler = Arc::new(ScanScheduler::new(db.clone(), handle, app_dir.clone(), scanning.clone()));

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

            // Show window unless --minimized or setting says so
            if !start_minimized && !setting_minimized {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                }
            } else if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
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
            get_snapshot_content,
            get_file_content,
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
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
