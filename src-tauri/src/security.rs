use std::path::PathBuf;

/// Path validation for read-only security.
/// Prevents path traversal, symlink attacks, and access to sensitive system directories.
pub struct PathValidator;

/// Directories that should NEVER be scanned (system-critical, security-sensitive)
const BLOCKED_DIRS: &[&str] = &[
    "C:\\Windows",
    "C:\\Program Files",
    "C:\\Program Files (x86)",
    "C:\\ProgramData",
    "C:\\$Recycle.Bin",
    "C:\\System Volume Information",
    "C:\\Boot",
    "C:\\Recovery",
    "/etc",
    "/boot",
    "/proc",
    "/sys",
    "/dev",
    "/sbin",
    "/usr/sbin",
    "/root",
    "/var/log",
    "/var/run",
    "/run",
];

/// Maximum path depth to prevent extremely deep traversal
const MAX_PATH_DEPTH: usize = 32;

/// Maximum path length (Windows limit)
const MAX_PATH_LENGTH: usize = 260;

impl PathValidator {
    /// Validate a directory path is safe to scan.
    /// Returns Ok(()) if valid, Err with reason if invalid.
    pub fn validate_directory(path: &str) -> Result<(), String> {

        // 1. Empty check
        if path.trim().is_empty() {
            return Err("Path cannot be empty".to_string());
        }

        // 2. Path length check
        if path.len() > MAX_PATH_LENGTH {
            return Err(format!("Path exceeds maximum length of {} characters", MAX_PATH_LENGTH));
        }

        // 3. Path traversal prevention (reject .., ~, etc.)
        let normalized = path.replace('\\', "/");
        if normalized.contains("/../") || normalized.ends_with("/..") || normalized == ".." {
            return Err("Path traversal detected (.. is not allowed)".to_string());
        }
        if normalized.contains("/./") || normalized.ends_with("/.") {
            return Err("Redundant path components (.) are not allowed".to_string());
        }

        // 4. Check for null bytes (injection attack)
        if path.contains('\0') {
            return Err("Null bytes in path are not allowed".to_string());
        }

        // 5. Check for special characters that could be used for injection
        if path.contains('`') || path.contains('$') {
            return Err("Special characters in path are not allowed".to_string());
        }
        // Reject URL-encoded sequences (%xx) but allow literal % (e.g. "100%")
        if path.contains('%') {
            let bytes = path.as_bytes();
            for i in 0..bytes.len().saturating_sub(2) {
                if bytes[i] == b'%' && bytes[i+1].is_ascii_hexdigit() && bytes[i+2].is_ascii_hexdigit() {
                    return Err("URL-encoded sequences in path are not allowed".to_string());
                }
            }
        }

        // 6. Resolve to actual path and check it exists
        let path_buf = PathBuf::from(path);
        if !path_buf.exists() {
            return Err(format!("Path does not exist: {}", path));
        }
        if !path_buf.is_dir() {
            return Err(format!("Path is not a directory: {}", path));
        }

        // 7. Symlink check — reject symlinks to prevent symlink attacks
        let metadata = std::fs::symlink_metadata(path)
            .map_err(|e| format!("Cannot read path metadata: {}", e))?;
        if metadata.file_type().is_symlink() {
            return Err("Symbolic links are not allowed for security reasons".to_string());
        }

        // 8. Check it's not a blocked system directory
        let canonical = path_buf
            .canonicalize()
            .map_err(|e| format!("Cannot resolve path: {}", e))?;
        let canonical_str = canonical.to_string_lossy().replace('\\', "/").to_lowercase();

        for blocked in BLOCKED_DIRS {
            let blocked_normalized = blocked.replace('\\', "/").to_lowercase();
            if canonical_str == blocked_normalized || canonical_str.starts_with(&format!("{}/", blocked_normalized)) {
                return Err(format!(
                    "Access to system directory '{}' is blocked for security",
                    blocked
                ));
            }
        }

        // 9. Check path depth
        let depth = canonical.components().count();
        if depth > MAX_PATH_DEPTH {
            return Err(format!(
                "Path too deep ({} levels). Maximum is {}",
                depth, MAX_PATH_DEPTH
            ));
        }

        // 10. Reject network paths (UNC, SMB, etc.)
        if path.starts_with("\\\\") || path.starts_with("//") {
            return Err("Network paths (UNC/SMB) are not allowed".to_string());
        }

        // 11. Reject paths with drive letter changes (Windows)
        #[cfg(target_os = "windows")]
        {
            if path.len() >= 2 && path.as_bytes()[1] == b':' {
                let drive = path.as_bytes()[0] as char;
                if !drive.is_ascii_alphabetic() {
                    return Err("Invalid drive letter".to_string());
                }
            }
        }

        // 12. Reject Windows reserved device names (CON, PRN, AUX, NUL, COM1-9, LPT1-9)
        #[cfg(target_os = "windows")]
        {
            if let Some(name) = canonical.file_name().and_then(|n| n.to_str()) {
                let stem = name.split('.').next().unwrap_or("").to_uppercase();
                const RESERVED: &[&str] = &[
                    "CON","PRN","AUX","NUL",
                    "COM1","COM2","COM3","COM4","COM5","COM6","COM7","COM8","COM9",
                    "LPT1","LPT2","LPT3","LPT4","LPT5","LPT6","LPT7","LPT8","LPT9",
                ];
                if RESERVED.contains(&stem.as_str()) {
                    return Err("Windows reserved name is not allowed".to_string());
                }
            }
            // Reject NTFS Alternate Data Streams (colon after drive letter)
            // Check the original path, NOT canonical_str — canonicalize() on Windows
            // returns UNC format (\\?\C:\...) which has colons that cause false positives.
            // Normalize: strip leading/trailing whitespace and any trailing separators
            let trimmed = path.trim();
            let normalized_path = trimmed.trim_end_matches('\\').trim_end_matches('/');
            // Only check for ADS if the path has a drive letter (e.g., "C:\...")
            if normalized_path.len() >= 2 && normalized_path.as_bytes()[1] == b':' {
                let without_drive = &normalized_path[2..];
                if without_drive.contains(':') {
                    return Err("Alternate data streams are not allowed".to_string());
                }
            }
        }

        Ok(())
    }

    /// Validate a file path is safe (same as validate_directory but allows files, not just dirs).
    pub fn validate_file_path(path: &str) -> Result<(), String> {
        // 1. Empty check
        if path.trim().is_empty() {
            return Err("Path cannot be empty".to_string());
        }

        // 2. Path length check
        if path.len() > MAX_PATH_LENGTH {
            return Err(format!("Path exceeds maximum length of {} characters", MAX_PATH_LENGTH));
        }

        // 3. Path traversal prevention
        let normalized = path.replace('\\', "/");
        if normalized.contains("/../") || normalized.ends_with("/..") || normalized == ".." {
            return Err("Path traversal detected (.. is not allowed)".to_string());
        }
        if normalized.contains("/./") || normalized.ends_with("/.") {
            return Err("Redundant path components (.) are not allowed".to_string());
        }

        // 4. Check for null bytes
        if path.contains('\0') {
            return Err("Null bytes in path are not allowed".to_string());
        }

        // 5. Check for special characters
        if path.contains('`') || path.contains('$') {
            return Err("Special characters in path are not allowed".to_string());
        }
        if path.contains('%') {
            let bytes = path.as_bytes();
            for i in 0..bytes.len().saturating_sub(2) {
                if bytes[i] == b'%' && bytes[i+1].is_ascii_hexdigit() && bytes[i+2].is_ascii_hexdigit() {
                    return Err("URL-encoded sequences in path are not allowed".to_string());
                }
            }
        }

        // 6. Resolve to actual path and check it exists (but allow files)
        let path_buf = PathBuf::from(path);
        if !path_buf.exists() {
            return Err(format!("Path does not exist: {}", path));
        }
        // Note: No is_dir() check — files are allowed

        // 7. Symlink check
        let metadata = std::fs::symlink_metadata(path)
            .map_err(|e| format!("Cannot read path metadata: {}", e))?;
        if metadata.file_type().is_symlink() {
            return Err("Symbolic links are not allowed for security reasons".to_string());
        }

        // 8. Check blocked system directories
        let canonical = path_buf
            .canonicalize()
            .map_err(|e| format!("Cannot resolve path: {}", e))?;
        let canonical_str = canonical.to_string_lossy().replace('\\', "/").to_lowercase();

        for blocked in BLOCKED_DIRS {
            let blocked_normalized = blocked.replace('\\', "/").to_lowercase();
            if canonical_str == blocked_normalized || canonical_str.starts_with(&format!("{}/", blocked_normalized)) {
                return Err(format!(
                    "Access to system directory '{}' is blocked for security",
                    blocked
                ));
            }
        }

        // 9. Check path depth
        let depth = canonical.components().count();
        if depth > MAX_PATH_DEPTH {
            return Err(format!(
                "Path too deep ({} levels). Maximum is {}",
                depth, MAX_PATH_DEPTH
            ));
        }

        // 10. Reject network paths
        if path.starts_with("\\\\") || path.starts_with("//") {
            return Err("Network paths (UNC/SMB) are not allowed".to_string());
        }

        // 11. Reject paths with drive letter changes (Windows)
        #[cfg(target_os = "windows")]
        {
            if path.len() >= 2 && path.as_bytes()[1] == b':' {
                let drive = path.as_bytes()[0] as char;
                if !drive.is_ascii_alphabetic() {
                    return Err("Invalid drive letter".to_string());
                }
            }
        }

        // 12. Reject Windows reserved device names
        #[cfg(target_os = "windows")]
        {
            if let Some(name) = canonical.file_name().and_then(|n| n.to_str()) {
                let stem = name.split('.').next().unwrap_or("").to_uppercase();
                const RESERVED: &[&str] = &[
                    "CON","PRN","AUX","NUL",
                    "COM1","COM2","COM3","COM4","COM5","COM6","COM7","COM8","COM9",
                    "LPT1","LPT2","LPT3","LPT4","LPT5","LPT6","LPT7","LPT8","LPT9",
                ];
                if RESERVED.contains(&stem.as_str()) {
                    return Err("Windows reserved name is not allowed".to_string());
                }
            }
            // Reject NTFS Alternate Data Streams
            // Check the original path, NOT canonical_str — canonicalize() on Windows
            // returns UNC format (\\?\C:\...) which has colons that cause false positives.
            let trimmed = path.trim();
            let normalized_path = trimmed.trim_end_matches('\\').trim_end_matches('/');
            if normalized_path.len() >= 2 && normalized_path.as_bytes()[1] == b':' {
                let without_drive = &normalized_path[2..];
                if without_drive.contains(':') {
                    return Err("Alternate data streams are not allowed".to_string());
                }
            }
        }

        Ok(())
    }

    /// Sanitize a filename for safe storage in database
    pub fn sanitize_filename(name: &str) -> String {
        name.chars()
            .filter(|c| !c.is_control())
            .take(255) // Max filename length
            .collect()
    }
}

/// Severity level for suspicious file detection
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum ThreatSeverity {
    Low,
    Medium,
    High,
}

/// Result of suspicious path analysis
#[derive(Debug, Clone, serde::Serialize)]
pub struct SuspiciousFileResult {
    pub is_suspicious: bool,
    pub severity: ThreatSeverity,
    pub threat_category: String,
}

impl PathValidator {
    /// Check if a file path looks suspicious, with severity classification.
    /// Returns a structured result with severity level and threat category.
    pub fn analyze_suspicious_path(path: &str) -> SuspiciousFileResult {
        let lower = path.to_lowercase();

        // HIGH: Double extensions commonly used in malware
        let high_extensions = [".exe.bat", ".exe.cmd", ".com.exe"];
        for ext in &high_extensions {
            if lower.contains(ext) {
                return SuspiciousFileResult {
                    is_suspicious: true,
                    severity: ThreatSeverity::High,
                    threat_category: format!("Double extension: {}", ext),
                };
            }
        }

        // MEDIUM: Suspicious single extensions
        let medium_extensions = [".scr", ".pif"];
        for ext in &medium_extensions {
            if lower.contains(ext) {
                return SuspiciousFileResult {
                    is_suspicious: true,
                    severity: ThreatSeverity::Medium,
                    threat_category: format!("Suspicious extension: {}", ext),
                };
            }
        }

        // HIGH: Known malware persistence paths
        let high_patterns = [
            ("\\appdata\\roaming\\microsoft\\windows\\start menu\\programs\\startup\\", "Startup folder persistence"),
            ("\\windows\\system32\\drivers\\etc\\", "Hosts file modification"),
        ];
        for (pattern, category) in &high_patterns {
            if lower.contains(pattern) {
                return SuspiciousFileResult {
                    is_suspicious: true,
                    severity: ThreatSeverity::High,
                    threat_category: category.to_string(),
                };
            }
        }

        // MEDIUM: Temp folder + executable content
        let medium_patterns = [
            ("\\appdata\\local\\temp\\", "Temp folder executable"),
            ("\\tmp\\", "Temp folder executable"),
        ];
        for (pattern, category) in &medium_patterns {
            if lower.contains(pattern) {
                // Only flag if it also has an executable-like extension
                let has_exec_ext = lower.ends_with(".exe") || lower.ends_with(".dll")
                    || lower.ends_with(".bat") || lower.ends_with(".cmd")
                    || lower.ends_with(".ps1") || lower.ends_with(".vbs")
                    || lower.ends_with(".js") || lower.ends_with(".wsf");
                if has_exec_ext {
                    return SuspiciousFileResult {
                        is_suspicious: true,
                        severity: ThreatSeverity::Medium,
                        threat_category: category.to_string(),
                    };
                }
            }
        }

        SuspiciousFileResult {
            is_suspicious: false,
            severity: ThreatSeverity::Low,
            threat_category: String::new(),
        }
    }

    /// Legacy wrapper — returns true if path is suspicious (for backwards compatibility).
    pub fn is_suspicious_path(path: &str) -> bool {
        Self::analyze_suspicious_path(path).is_suspicious
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_path() {
        assert!(PathValidator::validate_directory("").is_err());
        assert!(PathValidator::validate_directory("  ").is_err());
    }

    #[test]
    fn test_path_traversal() {
        assert!(PathValidator::validate_directory("C:\\Users\\..\\Windows").is_err());
        assert!(PathValidator::validate_directory("/home/../../../etc").is_err());
    }

    #[test]
    fn test_null_bytes() {
        assert!(PathValidator::validate_directory("C:\\Users\0\\test").is_err());
    }

    #[test]
    fn test_network_paths() {
        assert!(PathValidator::validate_directory("\\\\server\\share").is_err());
        assert!(PathValidator::validate_directory("//server/share").is_err());
    }

    #[test]
    fn test_suspicious_double_extension() {
        let r = PathValidator::analyze_suspicious_path("C:\\downloads\\document.exe.bat");
        assert!(r.is_suspicious);
        assert_eq!(r.severity, ThreatSeverity::High);
    }

    #[test]
    fn test_suspicious_scr_extension() {
        let r = PathValidator::analyze_suspicious_path("C:\\Users\\test\\screensaver.scr");
        assert!(r.is_suspicious);
        assert_eq!(r.severity, ThreatSeverity::Medium);
    }

    #[test]
    fn test_suspicious_startup_path() {
        let r = PathValidator::analyze_suspicious_path(
            "C:\\Users\\test\\AppData\\Roaming\\Microsoft\\Windows\\Start Menu\\Programs\\Startup\\payload.bat",
        );
        assert!(r.is_suspicious);
        assert_eq!(r.severity, ThreatSeverity::High);
    }

    #[test]
    fn test_clean_path_not_suspicious() {
        let r = PathValidator::analyze_suspicious_path("C:\\Users\\test\\Documents\\report.pdf");
        assert!(!r.is_suspicious);
    }
}
