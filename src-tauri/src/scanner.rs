use crate::database::Database;
use crate::security::PathValidator;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

/// Directories to skip during scanning (system/hidden/build artifacts)
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".svn",
    ".hg",
    "node_modules",
    "__pycache__",
    ".venv",
    "venv",
    ".cache",
    ".Trash",
    "target",
    "dist",
    "build",
    ".next",
    ".nuxt",
    "$Recycle.Bin",
    "System Volume Information",
    "Thumbs.db",
];

/// Directories to search when looking for moved files (user-accessible locations)
const SEARCH_DIRS: &[&str] = &[
    "C:\\Users",
    "/home",
];

pub struct Scanner {
    db: Arc<Database>,
}

impl Scanner {
    pub fn new(db: Arc<Database>) -> Self {
        Scanner { db }
    }

    /// Perform a full scan of a directory and return scan results.
    /// `batch_id` is passed in so insert_change doesn't need a subquery.
    /// `ignore_patterns` are pre-loaded once at scan start to avoid per-file DB queries.
    pub fn scan_directory(
        &self,
        dir_path: &str,
        batch_id: i64,
        ignore_patterns: &[crate::database::IgnorePattern],
    ) -> Result<ScanResult, String> {
        let path = Path::new(dir_path);
        if !path.exists() {
            return Err(format!("Directory does not exist: {}", dir_path));
        }
        if !path.is_dir() {
            return Err(format!("Path is not a directory: {}", dir_path));
        }

        let mut files_scanned = 0i64;
        let mut new_files = 0i64;
        let mut modified_files = 0i64;
        let mut total_size = 0i64;
        let mut errors = Vec::new();
        let mut suspicious_files = Vec::new();

        let walker = WalkDir::new(dir_path)
            .max_depth(20)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !is_skipped(e.file_name().to_str().unwrap_or("")));

        for entry in walker {
            match entry {
                Ok(entry) => {
                    if !entry.file_type().is_file() {
                        continue;
                    }

                    let file_path = entry.path().to_string_lossy().to_string();

                    // Check cached ignore patterns (no DB query)
                    if !ignore_patterns.is_empty()
                        && crate::database::Database::check_ignore_patterns(&file_path, ignore_patterns)
                    {
                        continue;
                    }

                    let metadata = match fs::metadata(entry.path()) {
                        Ok(m) => m,
                        Err(e) => {
                            errors.push(format!("{}: {}", file_path, e));
                            continue;
                        }
                    };

                    let size = metadata.len() as i64;
                    let mtime = metadata
                        .modified()
                        .unwrap_or(UNIX_EPOCH)
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;
                    let ctime = metadata
                        .created()
                        .unwrap_or(UNIX_EPOCH)
                        .duration_since(UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs() as i64;

                    let mtime_str = timestamp_to_string(mtime);
                    let ctime_str = timestamp_to_string(ctime);

                    let filename = entry
                        .file_name()
                        .to_str()
                        .unwrap_or("unknown")
                        .to_string();
                    let extension = Path::new(&filename)
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.to_string());

                    // Check if file already exists in DB (1 query)
                    let existing = self.db.get_file_by_path(&file_path).ok().flatten();

                    // Upsert — returns file_id directly (no re-query needed)
                    let file_id = self
                        .db
                        .upsert_file(
                            &file_path,
                            &filename,
                            extension.as_deref(),
                            size,
                            &mtime_str,
                            &ctime_str,
                        )
                        .map_err(|e| format!("DB error: {}", e))?;

                    files_scanned += 1;
                    total_size += size;

                    // Check for suspicious file patterns
                    let suspicion = PathValidator::analyze_suspicious_path(&file_path);
                    if suspicion.is_suspicious {
                        log::warn!(
                            "Suspicious file detected [{:?}]: {} ({})",
                            suspicion.severity,
                            file_path,
                            suspicion.threat_category
                        );
                        suspicious_files.push(SuspiciousFile {
                            path: file_path.clone(),
                            severity: format!("{:?}", suspicion.severity),
                            threat_category: suspicion.threat_category,
                        });
                    }

                    if let Some(ref existing_file) = existing {
                        if existing_file.mtime != mtime_str {
                            modified_files += 1;
                            // Use batch-aware insert (no subquery for batch_id)
                            self.db.insert_change_in_batch(existing_file.id, "MODIFIED", None, None, batch_id).ok();
                        }
                    } else {
                        // New file — check if it's a move from another location
                        let is_potential_move = self
                            .db
                            .find_file_by_name_and_size(&filename, size, &file_path)
                            .map(|candidates| {
                                candidates.iter().any(|c| !Path::new(&c.path).exists())
                            })
                            .unwrap_or(false);

                        if !is_potential_move {
                            new_files += 1;
                            // Use file_id from upsert (no re-query!)
                            self.db.insert_change_in_batch(file_id, "NEW", None, None, batch_id).ok();
                        }
                    }
                }
                Err(e) => {
                    errors.push(format!("Walk error: {}", e));
                }
            }
        }

        Ok(ScanResult {
            directory: dir_path.to_string(),
            files_scanned,
            new_files,
            modified_files,
            total_size,
            errors,
            scanned_at: Utc::now().to_rfc3339(),
            suspicious_files,
        })
    }

    /// Scan all monitored directories
    pub fn scan_all(&self) -> Result<Vec<ScanResult>, String> {
        let folders = self
            .db
            .get_monitored_folders()
            .map_err(|e| format!("DB error: {}", e))?;

        let batch_id = self.db.create_scan_batch(folders.len() as i64, "").unwrap_or(0);

        let mut results = Vec::new();
        for folder in folders {
            if folder.enabled {
                // Cache ignore patterns once per folder (1 DB query instead of N)
                let patterns = self.db.get_ignore_patterns_for_folder(folder.id).unwrap_or_default();
                match self.scan_directory(&folder.path, batch_id, &patterns) {
                    Ok(result) => results.push(result),
                    Err(e) => log::error!("Failed to scan {}: {}", folder.path, e),
                }
            }
        }
        Ok(results)
    }

    /// Detect moves and mark remaining missing files as deleted.
    /// For each missing file, search for new files with the same name + size.
    /// If found: record as MOVED, update the file record path.
    /// If not found: do a filesystem search as fallback, then mark as DELETED.
    /// Uses batched DB queries to avoid loading all files into memory at once.
    pub fn cleanup_deleted(&self) -> Result<(i64, i64), String> {
        const BATCH_SIZE: i64 = 2000;

        let mut moved_count = 0i64;
        let mut deleted_count = 0i64;
        let mut offset = 0i64;
        let mut total_processed = 0u64;

        loop {
            let batch = self.db.get_active_files_batch(offset, BATCH_SIZE)
                .map_err(|e| format!("DB error: {}", e))?;

            if batch.is_empty() {
                break;
            }

            for file in &batch {
                total_processed += 1;
                let file_exists = match fs::metadata(&file.path) {
                    Ok(_) => true,
                    Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
                        log::debug!("Skipping inaccessible file (permission denied): {}", file.path);
                        true
                    }
                    Err(_) => false,
                };

                if !file_exists {
                    let candidates = self
                        .db
                        .find_file_by_name_and_size(&file.filename, file.size, &file.path)
                        .map_err(|e| format!("DB error: {}", e))?;

                    let moved_to = candidates.iter().find(|c| Path::new(&c.path).exists());

                    if let Some(new_file) = moved_to {
                        self.db.mark_file_deleted(&file.path).ok();
                        self.db
                            .insert_change_with_paths(
                                new_file.id,
                                "MOVED",
                                Some(&file.path),
                                Some(&new_file.path),
                            )
                            .ok();
                        moved_count += 1;
                    } else {
                        match search_filesystem(&file.filename, file.size, &file.path) {
                            Some(found_path) => {
                                if let Ok(new_id) = self.db.upsert_file(
                                    &found_path,
                                    &file.filename,
                                    file.extension.as_deref(),
                                    file.size,
                                    &file.mtime,
                                    &file.ctime,
                                ) {
                                    self.db.mark_file_deleted(&file.path).ok();
                                    self.db
                                        .insert_change_with_paths(
                                            new_id,
                                            "MOVED",
                                            Some(&file.path),
                                            Some(&found_path),
                                        )
                                        .ok();
                                    moved_count += 1;
                                }
                            }
                            None => {
                                self.db.mark_file_deleted(&file.path).ok();
                                self.db
                                    .insert_change(file.id, "DELETED", None)
                                    .ok();
                                deleted_count += 1;
                            }
                        }
                    }
                }
            }

            offset += BATCH_SIZE;
        }

        log::info!(
            "Cleanup: {} moved, {} deleted (scanned {} files in batches)",
            moved_count,
            deleted_count,
            total_processed
        );
        Ok((deleted_count, moved_count))
    }

    /// Compute SHA-256 hash of a file (for duplicate detection)
    pub fn hash_file(path: &Path) -> Result<String, String> {
        // Skip files over 100MB to prevent blocking the scan thread
        if let Ok(meta) = fs::metadata(path) {
            if meta.len() > 100 * 1024 * 1024 {
                return Err("File too large for hashing".to_string());
            }
        }
        let mut file = fs::File::open(path).map_err(|e| format!("Cannot open {}: {}", path.display(), e))?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 8192];
        loop {
            let bytes_read = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }
        Ok(format!("{:x}", hasher.finalize()))
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SuspiciousFile {
    pub path: String,
    pub severity: String,
    pub threat_category: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ScanResult {
    pub directory: String,
    pub files_scanned: i64,
    pub new_files: i64,
    pub modified_files: i64,
    pub total_size: i64,
    pub errors: Vec<String>,
    pub scanned_at: String,
    #[serde(default)]
    pub suspicious_files: Vec<SuspiciousFile>,
}

fn is_skipped(name: &str) -> bool {
    SKIP_DIRS.contains(&name) || name.starts_with('.')
}

fn timestamp_to_string(ts: i64) -> String {
    let dt: DateTime<Utc> = DateTime::from_timestamp(ts, 0).unwrap_or_default();
    dt.to_rfc3339()
}

/// Search the filesystem for a file with matching name and size.
/// Searches user home directories for the file.
/// Returns the full path if found, None otherwise.
fn search_filesystem(filename: &str, size: i64, exclude_path: &str) -> Option<String> {
    let exclude_lower = exclude_path.replace('\\', "/").to_lowercase();
    const MAX_SEARCH_FILES: usize = 100_000;
    let mut files_checked: usize = 0;

    for search_root in SEARCH_DIRS {
        let root = Path::new(search_root);
        if !root.exists() {
            continue;
        }

        // Only search 3 levels deep to avoid performance issues
        let walker = WalkDir::new(search_root)
            .follow_links(false)
            .max_depth(4)
            .into_iter()
            .filter_entry(|e| !is_skipped(e.file_name().to_str().unwrap_or("")));

        for entry in walker.flatten() {
            if !entry.file_type().is_file() {
                continue;
            }

            files_checked += 1;
            if files_checked > MAX_SEARCH_FILES {
                log::warn!("Filesystem search exceeded {} file limit, aborting", MAX_SEARCH_FILES);
                return None;
            }

            let path = entry.path().to_string_lossy().to_string();
            let path_lower = path.replace('\\', "/").to_lowercase();

            // Skip the original path
            if path_lower == exclude_lower {
                continue;
            }

            // Check filename match
            let entry_name = entry.file_name().to_str().unwrap_or("");
            if entry_name != filename {
                continue;
            }

            // Check size match
            if let Ok(metadata) = fs::metadata(entry.path()) {
                if metadata.len() as i64 == size {
                    log::info!("Filesystem search found match: {}", path);
                    return Some(path);
                }
            }
        }
    }

    None
}
