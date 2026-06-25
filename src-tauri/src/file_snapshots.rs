use crate::database::Database;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const DEFAULT_MAX_SIZE: i64 = 102400; // 100KB
const MAX_DECOMPRESSED_BYTES: usize = 10 * 1024 * 1024; // 10MB hard ceiling for decompression
const MAX_RECURSION_DEPTH: u32 = 10; // Prevent unbounded recursion
const MAX_TOTAL_SNAPSHOT_BYTES: i64 = 500 * 1024 * 1024; // 500MB global disk quota
const MAX_SNAPSHOT_COUNT: i64 = 10000; // Hard cap on total snapshot count
const DEFAULT_EXTENSIONS: &[&str] = &[
    ".txt", ".md", ".js", ".ts", ".jsx", ".tsx", ".py", ".json",
    ".yaml", ".yml", ".toml", ".cfg", ".xml", ".html", ".css",
    ".sql", ".rs", ".go", ".java", ".c", ".cpp", ".h", ".sh",
    ".bat", ".ps1", ".rb", ".php", ".swift", ".kt", ".scala",
    ".vue", ".svelte", ".graphql", ".proto", ".env",
    ".ini", ".conf", ".log", ".csv", ".tsv",
];

pub struct FileSnapshotManager {
    db: Arc<Database>,
    base_dir: PathBuf,
}

impl FileSnapshotManager {
    pub fn new(db: Arc<Database>, app_data_dir: &Path) -> Self {
        let base_dir = app_data_dir.join("snapshots");
        fs::create_dir_all(&base_dir).ok();
        FileSnapshotManager { db, base_dir }
    }

    pub fn is_enabled(&self) -> bool {
        self.db
            .get_setting("file_snapshots_enabled")
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(true) // ON by default — opt-out, not opt-in
    }

    /// Get max file size in bytes (cached per call)
    fn get_max_size(&self) -> i64 {
        self.db
            .get_setting("snapshot_max_size")
            .ok()
            .flatten()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(DEFAULT_MAX_SIZE)
    }

    fn get_retention_days(&self) -> i64 {
        self.db
            .get_setting("snapshot_retention_days")
            .ok()
            .flatten()
            .and_then(|v| v.parse::<i64>().ok())
            .unwrap_or(30)
    }

    fn get_extensions(&self) -> Vec<String> {
        self.db
            .get_setting("snapshot_extensions")
            .ok()
            .flatten()
            .map(|v| {
                v.split(',')
                    .map(|s| s.trim().to_lowercase())
                    .collect()
            })
            .unwrap_or_else(|| DEFAULT_EXTENSIONS.iter().map(|s| s.to_string()).collect())
    }

    /// Check if a file should be snapshotted (uses cached settings)
    fn should_snapshot_with(&self, path: &str, size: i64, max_size: i64, extensions: &[String]) -> bool {
        if size > max_size || size == 0 {
            return false;
        }
        let ext = Path::new(path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e.to_lowercase()))
            .unwrap_or_default();
        extensions.contains(&ext)
    }

    /// Get the monitored folders list for path validation
    fn get_monitored_folders(&self) -> Vec<String> {
        self.db.get_monitored_folders()
            .map(|f| f.into_iter().map(|f| f.path).collect())
            .unwrap_or_default()
    }

    /// Validate that a restore destination is within monitored folders
    fn validate_restore_path(&self, original_path: &str) -> Result<(), String> {
        // Reject traversal
        let path_str = original_path.replace('\\', "/");
        if path_str.contains("/../") || path_str.ends_with("/..") {
            return Err("Path traversal detected".to_string());
        }

        // Reject system directories
        let lower = path_str.to_lowercase();
        let blocked = [
            "/windows/system32", "/windows/syswow64", "/windows/servicing",
            "/program files", "/program files (x86)",
            "/usr/bin", "/usr/sbin", "/usr/lib", "/sbin", "/bin", "/etc", "/proc", "/sys",
        ];
        for b in &blocked {
            if lower.starts_with(b) {
                return Err(format!("Cannot restore to system directory: {}", b));
            }
            // Also check after stripping drive letter prefix (Windows: "c:/windows/...")
            if let Some(after_drive) = lower.strip_prefix(|c: char| c.is_ascii_alphabetic()) {
                let after_colon = after_drive.strip_prefix(':').unwrap_or(after_drive);
                if after_colon.starts_with(b) {
                    return Err(format!("Cannot restore to system directory: {}", b));
                }
            }
        }

        // Reject extended-length paths
        if original_path.starts_with("\\\\?\\") || original_path.starts_with("\\\\.\\") {
            return Err("Extended-length paths not allowed".to_string());
        }

        // Check the path is under a monitored folder
        let monitored = self.get_monitored_folders();
        let is_monitored = monitored.iter().any(|folder| {
            let folder_lower = folder.replace('\\', "/").to_lowercase();
            let path_lower = path_str.to_lowercase();
            path_lower.starts_with(&folder_lower)
        });

        if !is_monitored && !monitored.is_empty() {
            log::warn!(
                "Restore target '{}' is not under any monitored folder",
                original_path
            );
            // Allow but log — the user may legitimately restore outside monitored dirs
        }

        Ok(())
    }

    /// Check if we're within disk quota and snapshot count limits
    fn check_quota(&self) -> Result<(), String> {
        let (count, total_size) = self.db.get_file_snapshot_stats().map_err(|e| e.to_string())?;
        if count >= MAX_SNAPSHOT_COUNT {
            return Err(format!(
                "Snapshot limit reached ({}/{}). Run cleanup or increase retention.",
                count, MAX_SNAPSHOT_COUNT
            ));
        }
        if total_size >= MAX_TOTAL_SNAPSHOT_BYTES {
            return Err(format!(
                "Snapshot disk quota reached ({}/{} MB). Run cleanup or increase retention.",
                total_size / (1024 * 1024),
                MAX_TOTAL_SNAPSHOT_BYTES / (1024 * 1024)
            ));
        }
        Ok(())
    }

    pub fn snapshot_file(&self, file_path: &str) -> Result<String, String> {
        let path = Path::new(file_path);
        if !path.exists() {
            return Err("File does not exist".to_string());
        }

        // Check global quota before doing any work
        self.check_quota()?;

        let metadata = fs::metadata(path).map_err(|e| format!("Failed to read metadata: {}", e))?;
        let size = metadata.len() as i64;

        // Hard cap: refuse files over 10MB to prevent OOM
        const MAX_SNAPSHOT_FILE_SIZE: u64 = 10 * 1024 * 1024;
        if metadata.len() > MAX_SNAPSHOT_FILE_SIZE {
            return Err(format!(
                "File too large for snapshot ({} bytes, max 10MB)",
                metadata.len()
            ));
        }

        // Cache settings for this call
        let max_size = self.get_max_size();
        let extensions = self.get_extensions();

        if !self.should_snapshot_with(file_path, size, max_size, &extensions) {
            return Err("File not eligible for snapshot".to_string());
        }

        // Read file content
        let mut file = fs::File::open(path).map_err(|e| format!("Failed to open file: {}", e))?;
        let mut content = Vec::new();
        file.read_to_end(&mut content)
            .map_err(|e| format!("Failed to read file: {}", e))?;

        // Compute SHA-256 hash of original content
        use sha2::{Sha256, Digest};
        let mut hasher = Sha256::new();
        hasher.update(&content);
        let content_hash = format!("{:x}", hasher.finalize());

        // M32: Skip if content hash matches most recent snapshot (dedup)
        if let Ok(snapshots) = self.db.get_snapshots_for_file(file_path) {
            if let Some(latest) = snapshots.first() {
                if latest.file_hash.as_deref() == Some(&content_hash) {
                    return Ok("skipped: unchanged".to_string());
                }
            }
        }

        // Compress with zstd (level 1 for speed)
        let compressed = zstd::encode_all(&content[..], 1)
            .map_err(|e| format!("Failed to compress: {}", e))?;
        let compressed_size = compressed.len() as i64;

        // Generate snapshot path
        let today = chrono::Local::now().format("%Y-%m-%d").to_string();
        let date_dir = self.base_dir.join(&today);
        fs::create_dir_all(&date_dir).ok();

        // Use hash of path as filename (full 32-char SHA-256 to avoid collisions)
        let mut path_hasher = Sha256::new();
        path_hasher.update(file_path.as_bytes());
        let path_hash = format!("{:x}", path_hasher.finalize());
        let snapshot_filename = format!("{}.zst", &path_hash[..32]); // Full hash, not truncated
        let snapshot_path = date_dir.join(&snapshot_filename);

        // Write to temp file first, then rename (atomic-ish) to prevent ghost snapshots
        let temp_path = snapshot_path.with_extension("zst.tmp");
        {
            let mut tmp = fs::File::create(&temp_path)
                .map_err(|e| format!("Failed to create temp snapshot: {}", e))?;
            tmp.write_all(&compressed)
                .map_err(|e| format!("Failed to write temp snapshot: {}", e))?;
            tmp.sync_all().ok();
        }

        // Record in database BEFORE finalizing the file
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown");

        self.db
            .insert_file_snapshot(
                file_path,
                filename,
                snapshot_path.to_str().unwrap_or(""),
                compressed_size,
                size,
                Some(&content_hash),
            )
            .map_err(|e| {
                // Clean up temp file if DB insert fails
                fs::remove_file(&temp_path).ok();
                format!("Failed to record snapshot: {}", e)
            })?;

        // Now rename temp to final (DB record exists, so this is safe)
        fs::rename(&temp_path, &snapshot_path)
            .map_err(|e| format!("Failed to finalize snapshot: {}", e))?;

        self.db.log_recovery_action(
            "snapshot_create",
            Some(&serde_json::json!({"path": file_path, "compressed_size": compressed_size}).to_string()),
            true,
            None,
        ).ok();

        Ok(snapshot_path.to_str().unwrap_or("").to_string())
    }

    /// Get snapshot info by ID (targeted query, not full table scan)
    fn get_snapshot_info(&self, snapshot_id: i64) -> Result<(String, String, Option<String>), String> {
        self.db.get_file_snapshot_by_id(snapshot_id)
            .map_err(|e| format!("DB error: {}", e))?
            .ok_or_else(|| "Snapshot not found".to_string())
            .map(|s| (s.snapshot_path, s.original_path, s.file_hash))
    }

    /// Decompress with size limit to prevent OOM from decompression bombs
    fn decompress_with_limit(compressed: &[u8], max_bytes: usize) -> Result<Vec<u8>, String> {
        let mut decoder = zstd::Decoder::new(compressed)
            .map_err(|e| format!("Decoder init failed: {}", e))?;
        let mut output = Vec::new();
        let mut buf = [0u8; 8192];
        let mut total = 0usize;
        loop {
            let n = decoder.read(&mut buf).map_err(|e| format!("Decompression read error: {}", e))?;
            if n == 0 { break; }
            total += n;
            if total > max_bytes {
                return Err(format!(
                    "Decompressed size exceeds {} byte limit (possible decompression bomb)",
                    max_bytes
                ));
            }
            output.extend_from_slice(&buf[..n]);
        }
        Ok(output)
    }

    pub fn restore_file_snapshot(&self, snapshot_id: i64) -> Result<String, String> {
        let (snapshot_path, original_path, expected_hash) = self.get_snapshot_info(snapshot_id)?;

        // Validate restore destination
        self.validate_restore_path(&original_path)?;

        let snapshot_file = Path::new(&snapshot_path);
        if !snapshot_file.exists() {
            return Err("Snapshot file missing from disk".to_string());
        }

        // Read compressed content
        let compressed = fs::read(snapshot_file)
            .map_err(|e| format!("Failed to read snapshot: {}", e))?;

        // Decompress with size limit (prevents decompression bombs)
        let decompressed = Self::decompress_with_limit(&compressed, MAX_DECOMPRESSED_BYTES)?;

        // Verify integrity via hash
        if let Some(ref expected) = expected_hash {
            use sha2::{Sha256, Digest};
            let mut hasher = Sha256::new();
            hasher.update(&decompressed);
            let actual = format!("{:x}", hasher.finalize());
            if actual != *expected {
                return Err(format!(
                    "Integrity check failed: expected hash {}, got {}. Snapshot may be corrupted or tampered.",
                    expected, actual
                ));
            }
        }

        // Back up existing file before overwriting
        let original = Path::new(&original_path);
        if original.exists() {
            let backup_path = PathBuf::from(format!("{}.pre-restore", original_path));
            fs::copy(original, &backup_path)
                .map_err(|e| format!("Failed to back up existing file: {}", e))?;
            log::info!("Backed up existing file to {}", backup_path.display());
        }

        // Create parent directories if needed
        if let Some(parent) = original.parent() {
            fs::create_dir_all(parent).ok();
        }

        // Write restored file
        fs::write(original, &decompressed)
            .map_err(|e| format!("Failed to write restored file: {}", e))?;

        self.db.log_recovery_action(
            "restore_snapshot",
            Some(&serde_json::json!({"snapshot_id": snapshot_id, "path": original_path}).to_string()),
            true,
            None,
        ).ok();

        Ok(original_path)
    }

    pub fn get_stats(&self) -> Result<(i64, i64), String> {
        self.db
            .get_file_snapshot_stats()
            .map_err(|e| e.to_string())
    }

    /// Cleanup old snapshots using the existing SQL method (not in-Rust filtering)
    pub fn cleanup_old_snapshots(&self) -> Result<i64, String> {
        let keep_days = self.get_retention_days();

        // Get paths of old snapshots before deleting DB records
        let old_snapshots = self.db.get_old_snapshot_paths(keep_days)
            .map_err(|e| format!("Failed to query old snapshots: {}", e))?;

        // Delete physical files first
        let mut deleted_files = 0i64;
        for path in &old_snapshots {
            let p = Path::new(path);
            if p.exists() {
                if fs::remove_file(p).is_ok() {
                    deleted_files += 1;
                }
            }
        }

        // Then delete DB records in one SQL statement (inside a transaction)
        let db_deleted = self.db.cleanup_old_file_snapshots(keep_days)
            .map_err(|e| format!("Failed to cleanup DB: {}", e))?;

        self.db.log_recovery_action(
            "cleanup_snapshots",
            Some(&serde_json::json!({"files_deleted": deleted_files, "db_deleted": db_deleted}).to_string()),
            true,
            None,
        ).ok();

        Ok(db_deleted)
    }

    /// Scan monitored folders and snapshot qualifying files (called during scan cycle)
    pub fn scan_and_snapshot(&self, folder_paths: &[String]) -> Result<i64, String> {
        if !self.is_enabled() {
            return Ok(0);
        }

        // Cache settings once at scan start
        let max_size = self.get_max_size();
        let extensions = self.get_extensions();

        let mut snapshot_count = 0i64;

        for folder_path in folder_paths {
            let path = Path::new(folder_path);
            if !path.exists() || !path.is_dir() {
                continue;
            }

            self.snapshot_directory_recursive(path, &mut snapshot_count, 0, &max_size, &extensions)?;
        }

        // Cleanup old snapshots
        self.cleanup_old_snapshots().ok();

        Ok(snapshot_count)
    }

    /// Recursive directory scan with depth limit to prevent stack overflow
    fn snapshot_directory_recursive(
        &self,
        dir: &Path,
        count: &mut i64,
        depth: u32,
        max_size: &i64,
        extensions: &[String],
    ) -> Result<(), String> {
        if depth >= MAX_RECURSION_DEPTH {
            return Ok(()); // Stop recursion at depth limit
        }

        let entries = fs::read_dir(dir).map_err(|e| format!("Failed to read dir: {}", e))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                // Skip hidden dirs, common non-user dirs, and symlinks
                let dir_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if dir_name.starts_with('.')
                    || dir_name == "node_modules"
                    || dir_name == "__pycache__"
                    || dir_name == ".git"
                {
                    continue;
                }
                // Skip symlinks (don't follow to prevent loops)
                if path.is_symlink() {
                    continue;
                }
                self.snapshot_directory_recursive(&path, count, depth + 1, max_size, extensions)?;
            } else if path.is_file() {
                let path_str = path.to_str().unwrap_or("");
                let file_size = entry.metadata().map(|m| m.len() as i64).unwrap_or(0);
                if self.should_snapshot_with(path_str, file_size, *max_size, extensions) {
                    // Check if file changed since last snapshot (compare hashes)
                    if let Ok(snapshots) = self.db.get_snapshots_for_file(path_str) {
                        if let Some(latest) = snapshots.first() {
                            // Compare current file hash against most recent snapshot
                            if let Ok(current_hash) = crate::scanner::Scanner::hash_file(&path) {
                                if latest.file_hash.as_deref() == Some(&current_hash) {
                                    continue; // File unchanged, skip snapshot
                                }
                            }
                        }
                    }
                    if self.snapshot_file(path_str).is_ok() {
                        *count += 1;
                    }
                }
            }
        }

        Ok(())
    }
}
