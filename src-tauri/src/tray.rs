use tauri::{
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    menu::{Menu, MenuItem},
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder,
    image::Image,
};

/// Build the main window with memory-optimized WebView2 flags.
/// Consolidates window creation to avoid duplication and ensure
/// every recreation path gets the same performance tuning.
pub fn build_main_window(app: &AppHandle, emit_msg: Option<&str>) -> Result<tauri::WebviewWindow, String> {
    let url = WebviewUrl::App("index.html".into());

    // Load icon for the window (taskbar, Alt-Tab, title bar)
    let icon_bytes = include_bytes!("../icons/icon.png");
    let icon = Image::from_bytes(icon_bytes)
        .map_err(|e| format!("Failed to load window icon: {}", e))?
        .to_owned();

    let builder = WebviewWindowBuilder::new(app, "main", url)
        .title("What Changed?")
        .inner_size(1100.0, 700.0)
        .min_inner_size(900.0, 600.0)
        .center()
        .icon(icon)
        .map_err(|e| format!("Failed to set window icon: {}", e))?
        // --- WebView2 memory optimizations ---
        // Disable GPU compositing to save ~10-20MB VRAM;
        // the UI is static text/cards, no GPU needed
        .additional_browser_args(
            "--disable-gpu-compositing \
             --disable-background-networking \
             --disable-default-apps \
             --disable-sync \
             --disable-translate \
             --metrics-recording-only \
             --no-first-run \
             --disable-features=TranslateUI"
        );

    let window = builder.build()
        .map_err(|e| format!("Failed to build window: {}", e))?;

    // Navigate to settings view if requested
    if let Some(msg) = emit_msg {
        let _ = window.emit("navigate", msg);
    }

    Ok(window)
}

pub fn create_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let show_item = MenuItem::with_id(app, "show", "Show Dashboard", true, None::<&str>)?;
    let scan_item = MenuItem::with_id(app, "scan", "Scan Now", true, None::<&str>)?;
    let settings_item = MenuItem::with_id(app, "settings", "Settings", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    let menu = Menu::with_items(app, &[&show_item, &scan_item, &settings_item, &quit_item])?;

    // Load the app icon for the tray
    let icon_bytes = include_bytes!("../icons/icon.png");
    let icon = Image::from_bytes(icon_bytes)
        .map_err(|e| format!("Failed to load tray icon: {}", e))?
        .to_owned();

    let _tray = TrayIconBuilder::new()
        .icon(icon)
        .menu(&menu)
        .tooltip("What Changed?")
        .on_menu_event(move |app, event| {
            match event.id().as_ref() {
                "show" => {
                    // Recreate the window if it was destroyed, otherwise show/focus existing
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    } else {
                        match build_main_window(app, None) {
                            Ok(w) => { let _ = w.show(); let _ = w.set_focus(); }
                            Err(e) => log::warn!("{}", e),
                        }
                    }
                }
                "scan" => {
                    // Trigger a scan via the app
                    let app_clone = app.clone();
                    tauri::async_runtime::spawn(async move {
                        // Emit event to frontend
                        let _ = app_clone.emit("trigger-scan", ());
                    });
                }
                "settings" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                        let _ = window.emit("navigate", "settings");
                    } else {
                        match build_main_window(app, Some("settings")) {
                            Ok(w) => { let _ = w.show(); let _ = w.set_focus(); }
                            Err(e) => log::warn!("{}", e),
                        }
                    }
                }
                "quit" => {
                    // Show confirmation warning before quitting
                    let app_clone = app.clone();
                    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
                    app.dialog()
                        .message("Auto-scans will stop running in the background.\n\nAre you sure you want to quit?")
                        .title("Quit What Changed?")
                        .kind(MessageDialogKind::Warning)
                        .buttons(MessageDialogButtons::OkCancel)
                        .show(move |confirmed| {
                            if confirmed {
                                // Set the quit flag so the ExitRequested handler allows exit
                                crate::set_quit_flag();
                                app_clone.exit(0);
                            }
                        });
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                // Recreate window if it was destroyed
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                } else {
                    match build_main_window(app, None) {
                        Ok(w) => { let _ = w.show(); let _ = w.set_focus(); }
                        Err(e) => log::warn!("{}", e),
                    }
                }
            }
        })
        .build(app)?;

    Ok(())
}
