use crate::database::Database;
use crate::scanner::Scanner;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

pub struct DuplicateDetector {
    db: Arc<Database>,
}

impl DuplicateDetector {
    pub fn new(db: Arc<Database>) -> Self {
        DuplicateDetector { db }
    }

    /// Run full duplicate detection across all monitored folders
    pub fn detect(&self) -> Result<DuplicateResult, String> {
        // Clear previous results
        self.db
            .clear_duplicates()
            .map_err(|e| format!("DB error: {}", e))?;

        let files = self
            .db
            .get_all_active_files()
            .map_err(|e| format!("DB error: {}", e))?;

        // Step 1: Group by file size
        let mut size_groups: HashMap<i64, Vec<&crate::database::FileRecord>> = HashMap::new();
        for file in &files {
            if file.size >= 1024 {
                // Skip tiny files
                size_groups
                    .entry(file.size)
                    .or_default()
                    .push(file);
            }
        }

        // Step 2: For groups with 2+ files, compute hashes
        let mut hash_groups: HashMap<String, Vec<&crate::database::FileRecord>> = HashMap::new();

        for (size, group_files) in &size_groups {
            if group_files.len() < 2 {
                continue;
            }

            for file in group_files {
                let path = Path::new(&file.path);
                if !path.exists() {
                    continue;
                }

                // Quick hash: first 4KB for fast filtering
                if let Ok(quick_hash) = quick_hash_file(path) {
                    hash_groups
                        .entry(format!("{}:{}", size, quick_hash))
                        .or_default()
                        .push(file);
                }
            }
        }

        // Step 3: Full hash for groups with 2+ files
        let mut full_hash_groups: HashMap<String, Vec<&crate::database::FileRecord>> = HashMap::new();

        for (_key, group_files) in &hash_groups {
            if group_files.len() < 2 {
                continue;
            }

            for file in group_files {
                let path = Path::new(&file.path);
                if !path.exists() {
                    continue;
                }

                if let Ok(full_hash) = Scanner::hash_file(path) {
                    full_hash_groups
                        .entry(full_hash)
                        .or_default()
                        .push(file);
                }
            }
        }

        // Step 4: Store duplicate groups
        let mut total_groups = 0i64;
        let mut total_wasted = 0i64;

        for (hash, group_files) in &full_hash_groups {
            if group_files.len() < 2 {
                continue;
            }

            let file_size = group_files[0].size;
            let group_id = self
                .db
                .insert_duplicate_group(hash, file_size)
                .map_err(|e| format!("DB error: {}", e))?;

            for file in group_files {
                self.db.insert_duplicate_file(group_id, file.id).ok();
            }

            // Wasted space = (count - 1) * size (keep one copy)
            total_wasted += (group_files.len() as i64 - 1) * file_size;
            total_groups += 1;
        }

        Ok(DuplicateResult {
            groups_found: total_groups,
            wasted_bytes: total_wasted,
        })
    }

    /// Get duplicate groups from database
    pub fn get_groups(&self) -> Result<Vec<crate::database::DuplicateGroupRecord>, String> {
        self.db
            .get_duplicate_groups()
            .map_err(|e| format!("DB error: {}", e))
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DuplicateResult {
    pub groups_found: i64,
    pub wasted_bytes: i64,
}

/// Quick hash using first 4KB of file for fast duplicate filtering
fn quick_hash_file(path: &Path) -> Result<String, String> {
    use std::io::Read;

    let mut file =
        std::fs::File::open(path).map_err(|e| format!("Cannot open {}: {}", path.display(), e))?;
    let mut buffer = [0u8; 4096];
    let bytes_read = file.read(&mut buffer).map_err(|e| e.to_string())?;

    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(&buffer[..bytes_read]);
    Ok(format!("{:x}", hasher.finalize()))
}
