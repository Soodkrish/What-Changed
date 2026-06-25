use crate::database::Database;
use std::path::Path;
use std::sync::Arc;

pub struct CloudDetector {
    db: Arc<Database>,
}

impl CloudDetector {
    pub fn new(db: Arc<Database>) -> Self {
        CloudDetector { db }
    }

    pub fn is_enabled(&self) -> bool {
        self.db
            .get_setting("cloud_detection_enabled")
            .ok()
            .flatten()
            .map(|v| v != "false")
            .unwrap_or(true)
    }

    /// Detect cloud sync folders on the system
    pub fn detect_cloud_folders(&self) -> Result<Vec<crate::database::CloudFolder>, String> {
        if !self.is_enabled() {
            return Ok(Vec::new());
        }

        let home = dirs_or_home();
        let mut found = Vec::new();

        // Windows & Linux cloud folder paths
        let candidates: Vec<(&str, &str, &str)> = vec![
            ("OneDrive", "onedrive", "OneDrive"),
            ("Google Drive", "gdrive", "Google Drive"),
            ("Dropbox", "dropbox", "Dropbox"),
            ("iCloudDrive", "icloud", "iCloud Drive"),
            ("OneDrive - Personal", "onedrive", "OneDrive - Personal"),
            ("OneDrive - Business", "onedrive", "OneDrive - Business"),
        ];

        for (dir_name, provider, display_name) in &candidates {
            let candidate_path = home.join(dir_name);
            if candidate_path.exists() && candidate_path.is_dir() {
                let path_str = candidate_path.to_str().unwrap_or("").to_string();
                self.db
                    .upsert_cloud_folder(&path_str, provider, Some(*display_name))
                    .ok();
                found.push(crate::database::CloudFolder {
                    id: 0,
                    path: path_str,
                    provider: provider.to_string(),
                    display_name: Some(display_name.to_string()),
                    is_active: true,
                    detected_at: String::new(),
                });
            }
        }

        // Also check for mapped drives (Windows)
        #[cfg(target_os = "windows")]
        {
            for drive in 'D'..='Z' {
                let drive_path = format!("{}:\\", drive);
                let p = Path::new(&drive_path);
                if p.exists() {
                    // Check if it looks like a cloud drive
                    for (dir_name, provider, display_name) in &candidates {
                        let cloud_in_drive = p.join(dir_name);
                        if cloud_in_drive.exists() && cloud_in_drive.is_dir() {
                            let path_str = cloud_in_drive.to_str().unwrap_or("").to_string();
                            self.db
                                .upsert_cloud_folder(&path_str, provider, Some(*display_name))
                                .ok();
                            found.push(crate::database::CloudFolder {
                                id: 0,
                                path: path_str,
                                provider: provider.to_string(),
                                display_name: Some(display_name.to_string()),
                                is_active: true,
                                detected_at: String::new(),
                            });
                        }
                    }
                }
            }
        }

        self.db.log_recovery_action(
            "cloud_detect",
            Some(&format!("{{\"folders_found\":{}}}", found.len())),
            true,
            None,
        ).ok();

        Ok(found)
    }

    /// Check if a path is inside a cloud-synced folder
    pub fn is_cloud_backed(&self, path: &str) -> Option<String> {
        self.db.is_path_in_cloud_folder(path).ok().flatten()
    }

    /// Get all detected cloud folders
    pub fn get_cloud_folders(&self) -> Result<Vec<crate::database::CloudFolder>, String> {
        self.db.get_cloud_folders().map_err(|e| e.to_string())
    }
}

/// Get user home directory cross-platform
fn dirs_or_home() -> std::path::PathBuf {
    #[cfg(target_os = "windows")]
    {
        std::env::var("USERPROFILE")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("C:\\Users"))
    }
    #[cfg(target_os = "linux")]
    {
        std::env::var("HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|_| std::path::PathBuf::from("/home"))
    }
    #[cfg(not(any(target_os = "windows", target_os = "linux")))]
    {
        std::path::PathBuf::from("/")
    }
}
