use crate::database::Database;
use chrono::Utc;
use std::path::Path;
use std::sync::Arc;
use walkdir::WalkDir;

pub struct StorageAnalyzer {
    db: Arc<Database>,
}

impl StorageAnalyzer {
    pub fn new(db: Arc<Database>) -> Self {
        StorageAnalyzer { db }
    }

    /// Take a snapshot of a directory (size + file count)
    pub fn snapshot_directory(&self, dir_path: &str) -> Result<DirSnapshot, String> {
        let path = Path::new(dir_path);
        if !path.exists() {
            return Err(format!("Directory does not exist: {}", dir_path));
        }

        let mut total_size = 0u64;
        let mut file_count = 0u64;

        for entry in WalkDir::new(dir_path)
            .follow_links(false)
            .into_iter()
            .filter_entry(|e| !e.file_name().to_str().unwrap_or("").starts_with('.'))
        {
            match entry {
                Ok(entry) => {
                    if entry.file_type().is_file() {
                        if let Ok(metadata) = std::fs::metadata(entry.path()) {
                            total_size += metadata.len();
                            file_count += 1;
                        }
                    }
                }
                Err(_) => {}
            }
        }

        let today = Utc::now().format("%Y-%m-%d").to_string();

        self.db
            .insert_snapshot(dir_path, &today, total_size as i64, file_count as i64)
            .map_err(|e| format!("DB error: {}", e))?;

        Ok(DirSnapshot {
            directory: dir_path.to_string(),
            total_size: total_size as i64,
            file_count: file_count as i64,
            snapshot_date: today,
        })
    }

    /// Snapshot all monitored directories
    pub fn snapshot_all(&self) -> Result<Vec<DirSnapshot>, String> {
        let folders = self
            .db
            .get_monitored_folders()
            .map_err(|e| format!("DB error: {}", e))?;

        let mut results = Vec::new();
        for folder in folders {
            if folder.enabled {
                match self.snapshot_directory(&folder.path) {
                    Ok(snapshot) => results.push(snapshot),
                    Err(e) => log::error!("Failed to snapshot {}: {}", folder.path, e),
                }
            }
        }
        Ok(results)
    }

    /// Get growth history for a directory
    pub fn get_growth_history(
        &self,
        dir_path: &str,
        days: i64,
    ) -> Result<Vec<crate::database::SnapshotRecord>, String> {
        self.db
            .get_snapshots(dir_path, days)
            .map_err(|e| format!("DB error: {}", e))
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DirSnapshot {
    pub directory: String,
    pub total_size: i64,
    pub file_count: i64,
    pub snapshot_date: String,
}
