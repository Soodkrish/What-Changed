use crate::database::Database;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct RecycleBinManager {
    db: Arc<Database>,
}

impl RecycleBinManager {
    pub fn new(db: Arc<Database>) -> Self {
        RecycleBinManager { db }
    }

    /// Query the recycle bin and match against tracked files
    pub fn query_and_match(&self) -> Result<Vec<crate::database::RecycleBinEntry>, String> {
        // Clear old entries first
        self.db.clear_old_recycle_bin_entries().ok();

        let entries = self.scan_recycle_bin()?;

        // Batch-match against tracked files in DB
        if !entries.is_empty() {
            let paths: Vec<&str> = entries.iter().map(|e| e.original_path.as_str()).collect();
            let tracked_files = self.db.get_deleted_files_by_paths(&paths).unwrap_or_default();
            let tracked_map: std::collections::HashMap<String, bool> = tracked_files
                .into_iter()
                .map(|f| (f.path.clone(), f.is_deleted))
                .collect();

            for entry in &entries {
                if tracked_map.get(&entry.original_path).copied().unwrap_or(false) {
                    self.db.insert_recycle_bin_entry(
                        &entry.original_path,
                        &entry.filename,
                        entry.original_size,
                        &entry.deleted_at,
                    ).ok();
                }
            }
        }

        self.db.log_recovery_action(
            "recycle_bin_scan",
            Some(&serde_json::json!({"entries_found": entries.len()}).to_string()),
            true,
            None,
        ).ok();

        self.db.get_recoverable_files().map_err(|e| e.to_string())
    }

    /// Scan the recycle bin for entries (platform-specific)
    fn scan_recycle_bin(&self) -> Result<Vec<RecycleBinCandidate>, String> {
        #[cfg(target_os = "windows")]
        {
            self.scan_windows_recycle_bin_native()
        }
        #[cfg(target_os = "linux")]
        {
            self.scan_linux_trash()
        }
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        {
            Ok(Vec::new())
        }
    }

    /// Windows: Parse $Recycle.Bin directly via Rust — no PowerShell dependency
    #[cfg(target_os = "windows")]
    fn scan_windows_recycle_bin_native(&self) -> Result<Vec<RecycleBinCandidate>, String> {
        use std::env;

        let system_drive = env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
        let recycle_root = format!("{}\\$Recycle.Bin", system_drive);
        let recycle_path = Path::new(&recycle_root);

        if !recycle_path.exists() {
            return Ok(Vec::new());
        }

        let mut candidates = Vec::new();

        // Iterate SID directories under $Recycle.Bin
        let sid_entries = fs::read_dir(recycle_path)
            .map_err(|e| format!("Cannot read $Recycle.Bin: {}", e))?;

        for sid_dir in sid_entries.flatten() {
            if !sid_dir.path().is_dir() {
                continue;
            }

            let sid_files = match fs::read_dir(sid_dir.path()) {
                Ok(f) => f,
                Err(_) => continue,
            };

            for file_entry in sid_files.flatten() {
                let name = file_entry.file_name();
                let name_str = name.to_string_lossy();

                // Only parse $I files
                if !name_str.starts_with("$I") {
                    continue;
                }

                // Extract the SID suffix (the part after "$I")
                let suffix = &name_str[2..];

                // Read the $I file: 8-byte header + UTF-16LE original path
                let content = match fs::read(file_entry.path()) {
                    Ok(c) => c,
                    Err(_) => continue,
                };

                if content.len() < 24 {
                    continue;
                }

                // Parse UTF-16LE path starting at offset 24
                let path_bytes = &content[24..];
                let utf16_values: Vec<u16> = path_bytes
                    .chunks(2)
                    .filter(|c| c.len() == 2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                let original_path = String::from_utf16_lossy(&utf16_values)
                    .chars()
                    .take_while(|&c| c != '\0')
                    .collect::<String>();

                if original_path.is_empty() {
                    continue;
                }

                let filename = Path::new(&original_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                // Find the corresponding $R file by matching suffix
                let r_file_name = format!("$R{}", suffix);
                let r_file_path = sid_dir.path().join(&r_file_name);
                let size = fs::metadata(&r_file_path)
                    .map(|m| m.len() as i64)
                    .unwrap_or(0);

                // Get deletion time from $I file modification time
                let deleted_at = fs::metadata(file_entry.path())
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|t| {
                        let datetime: chrono::DateTime<chrono::Local> = t.into();
                        Some(datetime.to_rfc3339())
                    })
                    .unwrap_or_default();

                candidates.push(RecycleBinCandidate {
                    original_path,
                    filename,
                    original_size: size,
                    deleted_at,
                    sid_suffix: suffix.to_string(),
                    r_file_path: Some(r_file_path),
                });
            }
        }

        // Sort by deletion time (most recent first), limit to 200
        candidates.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at));
        candidates.truncate(200);

        Ok(candidates)
    }

    #[cfg(target_os = "linux")]
    fn scan_linux_trash(&self) -> Result<Vec<RecycleBinCandidate>, String> {
        let home = std::env::var("HOME").unwrap_or_default();
        let trash_dir = PathBuf::from(format!("{}/.local/share/Trash", home));
        let info_dir = trash_dir.join("info");
        let files_dir = trash_dir.join("files");

        if !info_dir.exists() {
            return Ok(Vec::new());
        }

        let mut candidates = Vec::new();

        if let Ok(entries) = fs::read_dir(&info_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("trashinfo") {
                    if let Ok(content) = fs::read_to_string(&path) {
                        let mut original_path = String::new();
                        let mut deletion_date = String::new();

                        for line in content.lines() {
                            if let Some(p) = line.strip_prefix("Path=") {
                                original_path = p.to_string();
                            }
                            if let Some(d) = line.strip_prefix("DeletionDate=") {
                                deletion_date = d.to_string();
                            }
                        }

                        if !original_path.is_empty() {
                            let decoded = urlencoding::decode(&original_path)
                                .unwrap_or_default()
                                .to_string();
                            let filename = Path::new(&decoded)
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("unknown")
                                .to_string();

                            let file_in_trash = files_dir.join(&filename);
                            let size = fs::metadata(&file_in_trash)
                                .map(|m| m.len() as i64)
                                .unwrap_or(0);

                            candidates.push(RecycleBinCandidate {
                                original_path: decoded,
                                filename,
                                original_size: size,
                                deleted_at: deletion_date,
                                sid_suffix: String::new(),
                                r_file_path: None,
                            });
                        }
                    }
                }
            }
        }

        Ok(candidates)
    }

    /// Restore a file from the recycle bin
    pub fn restore_file(&self, entry_id: i64) -> Result<String, String> {
        let entries = self.db.get_recoverable_files().map_err(|e| e.to_string())?;
        let entry = entries.iter().find(|e| e.id == entry_id)
            .ok_or("Recycle bin entry not found")?;

        let original_path = entry.original_path.clone();

        // Validate restore destination
        validate_restore_path(&original_path)?;

        #[cfg(target_os = "windows")]
        {
            return self.restore_windows(entry_id, &original_path, &entry.filename);
        }

        #[cfg(target_os = "linux")]
        {
            return self.restore_linux(entry_id, &original_path, &entry.filename);
        }

        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        {
            Err("Recycle bin restore not supported on this platform".to_string())
        }
    }

    /// Windows restore: match $R file by SID suffix, verify size, copy
    #[cfg(target_os = "windows")]
    fn restore_windows(&self, entry_id: i64, original_path: &str, filename: &str) -> Result<String, String> {
        // First try: find the $R file that pairs with the $I entry via SID suffix
        if let Some(r_path) = self.find_r_file_by_suffix(original_path) {
            return self.copy_and_verify(entry_id, &r_path, original_path);
        }

        // Second try: PowerShell COM object (Shell.Application) as fallback
        // Write a temp .ps1 script and invoke via -File to avoid command injection.
        // The script receives filename and path as arguments — no interpolation.
        let ps_script = format!(
            "param($fn, $rp)\n\
             $s = New-Object -ComObject Shell.Application\n\
             $rb = $s.NameSpace(0xa)\n\
             foreach ($item in $rb.Items()) {{\n\
                 if ($item.Name -eq $fn) {{\n\
                     $destDir = Split-Path $rp -Parent\n\
                     $s.Namespace($destDir).MoveHere($item)\n\
                     break\n\
                 }}\n\
             }}"
        );
        let ps_path = std::env::temp_dir().join(format!("wc_restore_{}.ps1", entry_id));
        let _ = std::fs::write(&ps_path, &ps_script);
        let restore_result = std::process::Command::new("powershell")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File", &ps_path.to_string_lossy(), "--", filename, original_path])
            .output();
        let _ = std::fs::remove_file(&ps_path);

        match restore_result {
            Ok(output) if output.status.success() => {
                self.db.mark_recycle_bin_recovered(entry_id).ok();
                self.db.log_recovery_action(
                    "restore_recycle_bin",
                    Some(&serde_json::json!({"entry_id": entry_id, "path": original_path, "method": "com"}).to_string()),
                    true,
                    None,
                ).ok();
                Ok(original_path.to_string())
            }
            _ => {
                // Third try: search all $R files by name match (last resort)
                self.restore_by_name_search(entry_id, filename, original_path)
            }
        }
    }

    /// Find the $R file paired with an $I file by matching the SID suffix
    #[cfg(target_os = "windows")]
    fn find_r_file_by_suffix(&self, original_path: &str) -> Option<PathBuf> {
        use std::env;

        let system_drive = env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
        let recycle_root = format!("{}\\$Recycle.Bin", system_drive);
        let recycle_path = Path::new(&recycle_root);

        if !recycle_path.exists() {
            return None;
        }

        // Scan SID directories
        if let Ok(sid_entries) = fs::read_dir(recycle_path) {
            for sid_dir in sid_entries.flatten() {
                if !sid_dir.path().is_dir() {
                    continue;
                }

                if let Ok(s_files) = fs::read_dir(sid_dir.path()) {
                    for f in s_files.flatten() {
                        let name = f.file_name().to_string_lossy().to_string();

                        // Find $I files that contain our original path
                        if name.starts_with("$I") {
                            let suffix = &name[2..];
                            if let Ok(content) = fs::read(f.path()) {
                                if content.len() > 24 {
                                    let path_bytes = &content[24..];
                                    let utf16: Vec<u16> = path_bytes
                                        .chunks(2)
                                        .filter(|c| c.len() == 2)
                                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                                        .collect();
                                    let ip = String::from_utf16_lossy(&utf16)
                                        .chars()
                                        .take_while(|&c| c != '\0')
                                        .collect::<String>();

                                    if ip == original_path {
                                        // Found the matching $I — get the paired $R
                                        let r_path = sid_dir.path().join(format!("$R{}", suffix));
                                        if r_path.exists() {
                                            return Some(r_path);
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Copy file from recycle bin to original path with size verification
    #[cfg(target_os = "windows")]
    fn copy_and_verify(&self, entry_id: i64, r_path: &Path, original_path: &str) -> Result<String, String> {
        let dest = Path::new(original_path);

        // Get file info before copying
        let r_size = fs::metadata(r_path)
            .map(|m| m.len())
            .map_err(|e| format!("Cannot read recycle bin file: {}", e))?;

        // Check if destination exists — back it up
        if dest.exists() {
            let backup_path = PathBuf::from(format!("{}.pre-restore", original_path));
            fs::copy(dest, &backup_path)
                .map_err(|e| format!("Failed to back up existing file: {}", e))?;
        }

        // Ensure parent directory exists
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).ok();
        }

        // Copy the file
        fs::copy(r_path, dest)
            .map_err(|e| format!("Failed to copy file: {}", e))?;

        // Verify size matches
        let dest_size = fs::metadata(dest).map(|m| m.len()).unwrap_or(0);
        if dest_size != r_size {
            log::warn!(
                "Size mismatch after restore: expected {}, got {}",
                r_size, dest_size
            );
        }

        self.db.mark_recycle_bin_recovered(entry_id).ok();
        self.db.log_recovery_action(
            "restore_recycle_bin",
            Some(&serde_json::json!({
                "entry_id": entry_id,
                "path": original_path,
                "source": r_path.to_string_lossy(),
                "size": r_size,
                "method": "sid_match"
            }).to_string()),
            true,
            None,
        ).ok();

        Ok(original_path.to_string())
    }

    /// Last-resort restore: search by filename match (with size verification)
    #[cfg(target_os = "windows")]
    fn restore_by_name_search(&self, entry_id: i64, filename: &str, original_path: &str) -> Result<String, String> {
        use std::env;

        let system_drive = env::var("SystemDrive").unwrap_or_else(|_| "C:".to_string());
        let recycle_root = format!("{}\\$Recycle.Bin", system_drive);
        let recycle_path = Path::new(&recycle_root);

        if !recycle_path.exists() {
            return Err("Recycle bin not found".to_string());
        }

        // Get expected size from DB entry
        let entries = self.db.get_recoverable_files().unwrap_or_default();
        let expected_size = entries.iter()
            .find(|e| e.id == entry_id)
            .map(|e| e.original_size as u64)
            .unwrap_or(0);

        // Scan for $R files matching the filename
        if let Ok(sid_entries) = fs::read_dir(recycle_path) {
            for sid_dir in sid_entries.flatten() {
                if !sid_dir.path().is_dir() {
                    continue;
                }
                if let Ok(r_files) = fs::read_dir(sid_dir.path()) {
                    for r_entry in r_files.flatten() {
                        let name = r_entry.file_name().to_string_lossy().to_string();
                        if name.starts_with("$R") {
                            // Verify size matches expected
                            if let Ok(meta) = fs::metadata(r_entry.path()) {
                                if expected_size > 0 && meta.len() != expected_size {
                                    continue; // Size doesn't match — skip
                                }
                            }

                            // Verify original filename matches
                            let r_suffix = &name[2..];
                            let i_file = sid_dir.path().join(format!("$I{}", r_suffix));
                            if i_file.exists() {
                                if let Ok(content) = fs::read(&i_file) {
                                    if content.len() > 24 {
                                        let path_bytes = &content[24..];
                                        let utf16: Vec<u16> = path_bytes
                                            .chunks(2)
                                            .filter(|c| c.len() == 2)
                                            .map(|c| u16::from_le_bytes([c[0], c[1]]))
                                            .collect();
                                        let ip = String::from_utf16_lossy(&utf16)
                                            .chars()
                                            .take_while(|&c| c != '\0')
                                            .collect::<String>();

                                        let ip_filename = Path::new(&ip)
                                            .file_name()
                                            .and_then(|n| n.to_str())
                                            .unwrap_or("");

                                        if ip_filename == filename {
                                            // Match found — verify size then copy
                                            let r_size = fs::metadata(r_entry.path())
                                                .map(|m| m.len())
                                                .unwrap_or(0);

                                            if expected_size > 0 && r_size != expected_size {
                                                log::warn!(
                                                    "Size mismatch: expected {}, got {} for {}",
                                                    expected_size, r_size, filename
                                                );
                                                continue;
                                            }

                                            let dest = Path::new(original_path);
                                            if dest.exists() {
                                                let backup = PathBuf::from(format!("{}.pre-restore", original_path));
                                                fs::copy(dest, &backup).ok();
                                            }
                                            if let Some(parent) = dest.parent() {
                                                fs::create_dir_all(parent).ok();
                                            }

                                            if fs::copy(r_entry.path(), dest).is_ok() {
                                                self.db.mark_recycle_bin_recovered(entry_id).ok();
                                                self.db.log_recovery_action(
                                                    "restore_recycle_bin",
                                                    Some(&serde_json::json!({
                                                        "entry_id": entry_id,
                                                        "path": original_path,
                                                        "method": "name_search",
                                                        "size": r_size
                                                    }).to_string()),
                                                    true,
                                                    None,
                                                ).ok();
                                                return Ok(original_path.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Err("Could not find matching file in recycle bin".to_string())
    }

    /// Linux restore: copy from Trash/files/ to original path
    #[cfg(target_os = "linux")]
    fn restore_linux(&self, entry_id: i64, original_path: &str, filename: &str) -> Result<String, String> {
        // Validate filename has no path traversal
        if filename.contains('/') || filename.contains('\\') || filename.contains("..") {
            return Err("Invalid filename: path traversal detected".to_string());
        }

        let home = std::env::var("HOME").unwrap_or_default();
        let trash_file = PathBuf::from(format!("{}/.local/share/Trash/files/{}", home, filename));

        if !trash_file.exists() {
            return Err("Trash file not found".to_string());
        }

        let dest = Path::new(original_path);

        // Back up existing file if present
        if dest.exists() {
            let backup_path = PathBuf::from(format!("{}.pre-restore", original_path));
            fs::copy(dest, &backup_path)
                .map_err(|e| format!("Failed to back up existing file: {}", e))?;
        }

        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).ok();
        }

        fs::copy(&trash_file, dest)
            .map_err(|e| format!("Failed to restore: {}", e))?;

        // Remove trash file and .trashinfo
        fs::remove_file(&trash_file).ok();
        let info_file = PathBuf::from(format!(
            "{}/.local/share/Trash/info/{}.trashinfo",
            home, filename
        ));
        fs::remove_file(info_file).ok();

        self.db.mark_recycle_bin_recovered(entry_id).ok();
        self.db.log_recovery_action(
            "restore_recycle_bin",
            Some(&serde_json::json!({
                "entry_id": entry_id,
                "path": original_path,
                "method": "trash_direct"
            }).to_string()),
            true,
            None,
        ).ok();

        Ok(original_path.to_string())
    }

    /// Get all recoverable files from DB
    pub fn get_recoverable_files(&self) -> Result<Vec<crate::database::RecycleBinEntry>, String> {
        self.db.get_recoverable_files().map_err(|e| e.to_string())
    }

    /// Get count of recoverable files
    pub fn get_count(&self) -> i64 {
        self.db.get_recoverable_files().map(|v| v.len() as i64).unwrap_or(0)
    }
}

/// Validate that a restore path is within allowed boundaries
fn validate_restore_path(path: &str) -> Result<(), String> {
    // Delegate to the full PathValidator (covers traversal, null bytes, UNC,
    // symlinks, reserved names, ADS, depth limit, and blocked directories)
    crate::security::PathValidator::validate_file_path(path)
}

struct RecycleBinCandidate {
    original_path: String,
    filename: String,
    original_size: i64,
    deleted_at: String,
    #[allow(dead_code)]
    sid_suffix: String,
    #[allow(dead_code)]
    r_file_path: Option<PathBuf>,
}
