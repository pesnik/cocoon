use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::Manager;
use tauri_plugin_global_shortcut::ShortcutState;

#[tauri::command(async)]
async fn search_entries(query: String) -> Vec<PasswordEntry> {
    // Mock data for now - replace with actual database queries later
    let mock_entries = vec![
        PasswordEntry {
            id: 1,
            title: "GitHub".to_string(),
            username: "john.doe@email.com".to_string(),
            url: Some("https://github.com".to_string()),
            notes: None,
        },
        PasswordEntry {
            id: 2,
            title: "Gmail Personal".to_string(),
            username: "john.personal@gmail.com".to_string(),
            url: Some("https://gmail.com".to_string()),
            notes: Some("Personal email account".to_string()),
        },
        PasswordEntry {
            id: 3,
            title: "AWS Console".to_string(),
            username: "admin@company.com".to_string(),
            url: Some("https://aws.amazon.com".to_string()),
            notes: Some("Production access".to_string()),
        },
    ];

    if query.is_empty() {
        return mock_entries;
    }

    // Simple fuzzy search - improve this later
    mock_entries
        .into_iter()
        .filter(|entry| {
            entry.title.to_lowercase().contains(&query.to_lowercase())
                || entry
                    .username
                    .to_lowercase()
                    .contains(&query.to_lowercase())
        })
        .collect()
}

#[tauri::command]
async fn copy_password(entry_id: u32, app_handle: tauri::AppHandle) -> Result<(), String> {
    // Mock password copy - replace with actual encrypted password retrieval
    let mock_password = format!("SecurePass{}", entry_id);
    println!(
        "Would copy password for entry {}: {}",
        entry_id, mock_password
    );

    // Hide the window after copying
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.hide();
    }

    Ok(())
}

#[tauri::command]
async fn copy_username(username: String, app_handle: tauri::AppHandle) -> Result<(), String> {
    // Similar to copy_password, this will use the clipboard plugin
    println!("Would copy username: {}", username);

    // Hide the window after copying
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.hide();
    }

    Ok(())
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct PasswordEntry {
    id: u32,
    title: String,
    username: String,
    url: Option<String>,
    notes: Option<String>,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            // Create tray icon
            #[cfg(desktop)]
            {
                TrayIconBuilder::with_id("main")
                    .tooltip("Cocoon Password Manager")
                    .icon(app.default_window_icon().unwrap().clone())
                    .build(app)?;
            }

            // Setup global shortcut
            #[cfg(desktop)]
            {
                use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

                // Define shortcut: Command+Alt+C for macOS, Ctrl+Shift+C for others
                let shortcut = if cfg!(target_os = "macos") {
                    Shortcut::new(Some(Modifiers::SUPER | Modifiers::ALT), Code::KeyC)
                } else {
                    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyC)
                };

                // Register global shortcut plugin with handler
                app.handle().plugin(
                    tauri_plugin_global_shortcut::Builder::new()
                        .with_handler(move |_app, received_shortcut, event| {
                            if received_shortcut == &shortcut {
                                match event.state() {
                                    ShortcutState::Pressed => {
                                        if let Some(window) = _app.get_webview_window("main") {
                                            let _ = window.show();
                                            let _ = window.set_focus();
                                            let _ = window.center();
                                        }
                                    }
                                    ShortcutState::Released => {
                                        // Optionally handle release event
                                    }
                                }
                            }
                        })
                        .build(),
                )?;

                // Register the shortcut
                app.global_shortcut().register(shortcut)?;
            }

            // Configure main window to not show in taskbar
            #[cfg(desktop)]
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }

            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::Focused(focused) => {
                // hide window whenever it loses focus
                if !focused {
                    window.hide().unwrap();
                }
            }
            // tauri::WindowEvent::CloseRequested { .. } => {
            //     // Prevent window from closing - just hide it
            //     event.window().hide().unwrap();
            // }
            _ => {}
        })
        .on_tray_icon_event(|app, event| {
            #[cfg(desktop)]
            match event {
                TrayIconEvent::Click { .. } => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                        let _ = window.center();
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            search_entries,
            copy_password,
            copy_username
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
