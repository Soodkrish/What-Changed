/// Auto-start on boot support.
/// Windows: HKCU\Software\Microsoft\Windows\CurrentVersion\Run
/// Linux: ~/.config/autostart/<app>.desktop

const APP_NAME: &str = "WhatChanged";

/// Get the current executable path (normalized).
fn current_exe_path() -> Result<String, String> {
    let path = std::env::current_exe()
        .map_err(|e| format!("Failed to get exe path: {}", e))?
        .to_string_lossy()
        .to_string();
    Ok(path)
}

/// Enable auto-start on boot.
#[cfg(target_os = "windows")]
pub fn enable_autostart() -> Result<(), String> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = r"Software\Microsoft\Windows\CurrentVersion\Run";
    let run_key = hkcu.open_subkey_with_flags(path, KEY_SET_VALUE)
        .map_err(|e| format!("Failed to open registry key: {}", e))?;

    let exe_path = current_exe_path()?;

    run_key.set_value(APP_NAME, &format!("\"{}\" --minimized", exe_path))
        .map_err(|e| format!("Failed to set registry value: {}", e))?;

    log::info!("Auto-start enabled: {}", exe_path);
    Ok(())
}

/// Disable auto-start on boot.
#[cfg(target_os = "windows")]
pub fn disable_autostart() -> Result<(), String> {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = r"Software\Microsoft\Windows\CurrentVersion\Run";
    let run_key = hkcu.open_subkey_with_flags(path, KEY_SET_VALUE)
        .map_err(|e| format!("Failed to open registry key: {}", e))?;

    // delete_value fails if the value doesn't exist — that's fine, already disabled
    let _ = run_key.delete_value(APP_NAME);

    log::info!("Auto-start disabled");
    Ok(())
}

/// Check if auto-start is enabled AND the registry path matches the current executable.
/// Returns false if the key exists but points to a different (stale) path.
#[cfg(target_os = "windows")]
pub fn is_autostart_enabled() -> bool {
    use winreg::enums::*;
    use winreg::RegKey;

    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let path = r"Software\Microsoft\Windows\CurrentVersion\Run";
    if let Ok(run_key) = hkcu.open_subkey_with_flags(path, KEY_READ) {
        if let Ok(stored_value) = run_key.get_value::<String, _>(APP_NAME) {
            // Verify the stored path points to the current exe
            let current = match current_exe_path() {
                Ok(p) => p,
                Err(_) => return true, // can't verify — assume enabled
            };
            // Registry value is like: "C:\path\app.exe" --minimized
            // Extract the exe path (before the first quote after the opening quote)
            let trimmed = stored_value.trim();
            let stored_exe = if trimmed.starts_with('"') {
                // "C:\path\app.exe" --minimized → C:\path\app.exe
                let inner = &trimmed[1..];
                match inner.find('"') {
                    Some(q) => &inner[..q],
                    None => trimmed,
                }
            } else {
                trimmed.split_whitespace().next().unwrap_or(trimmed)
            };
            // Case-insensitive path comparison
            stored_exe.eq_ignore_ascii_case(&current)
        } else {
            false
        }
    } else {
        false
    }
}

/// Enable auto-start on boot (Linux).
#[cfg(target_os = "linux")]
pub fn enable_autostart() -> Result<(), String> {
    let autostart_dir = dirs::home_dir()
        .ok_or("Failed to get home directory")?
        .join(".config")
        .join("autostart");

    std::fs::create_dir_all(&autostart_dir)
        .map_err(|e| format!("Failed to create autostart dir: {}", e))?;

    let desktop_file = autostart_dir.join(format!("{}.desktop", APP_NAME));
    let exe_path = current_exe_path()?;

    let content = format!(
        r#"[Desktop Entry]
Type=Application
Name=What Changed?
Exec={} --minimized
Hidden=false
NoDisplay=false
X-GNOME-Autostart-enabled=true
Comment=Monitor file system changes
"#,
        exe_path
    );

    std::fs::write(&desktop_file, content)
        .map_err(|e| format!("Failed to write .desktop file: {}", e))?;

    log::info!("Auto-start enabled (Linux): {}", desktop_file.display());
    Ok(())
}

/// Disable auto-start on boot (Linux).
#[cfg(target_os = "linux")]
pub fn disable_autostart() -> Result<(), String> {
    let autostart_dir = dirs::home_dir()
        .ok_or("Failed to get home directory")?
        .join(".config")
        .join("autostart");

    let desktop_file = autostart_dir.join(format!("{}.desktop", APP_NAME));
    if desktop_file.exists() {
        std::fs::remove_file(&desktop_file)
            .map_err(|e| format!("Failed to remove .desktop file: {}", e))?;
        log::info!("Auto-start disabled (Linux)");
    }
    Ok(())
}

/// Check if auto-start is enabled (Linux).
#[cfg(target_os = "linux")]
pub fn is_autostart_enabled() -> bool {
    dirs::home_dir()
        .map(|h| {
            h.join(".config")
                .join("autostart")
                .join(format!("{}.desktop", APP_NAME))
                .exists()
        })
        .unwrap_or(false)
}

/// Fallback for unsupported platforms.
#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub fn enable_autostart() -> Result<(), String> {
    Err("Auto-start not supported on this platform".to_string())
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub fn disable_autostart() -> Result<(), String> {
    Err("Auto-start not supported on this platform".to_string())
}

#[cfg(not(any(target_os = "windows", target_os = "linux")))]
pub fn is_autostart_enabled() -> bool {
    false
}
