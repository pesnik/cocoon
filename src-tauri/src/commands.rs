use serde_json;
use std::fs;
use std::path::PathBuf;
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::Manager;
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::ShortcutState;

#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct PasswordEntry {
    id: u32,
    title: String,
    username: String,
    password: String,
    url: Option<String>,
    notes: Option<String>,
    created_at: String,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct PasswordStore {
    entries: Vec<PasswordEntry>,
    next_id: u32,
}

impl Default for PasswordStore {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            next_id: 1,
        }
    }
}

fn get_data_file_path() -> Result<PathBuf, String> {
    let app_data_dir = dirs::data_dir()
        .ok_or("Could not find data directory")?
        .join("cocoon-password-manager");

    fs::create_dir_all(&app_data_dir)
        .map_err(|e| format!("Failed to create data directory: {}", e))?;

    Ok(app_data_dir.join("passwords.json"))
}

fn load_password_store() -> Result<PasswordStore, String> {
    let file_path = get_data_file_path()?;

    if !file_path.exists() {
        return Ok(PasswordStore::default());
    }

    let content = fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read password file: {}", e))?;

    serde_json::from_str(&content).map_err(|e| format!("Failed to parse password file: {}", e))
}

fn save_password_store(store: &PasswordStore) -> Result<(), String> {
    let file_path = get_data_file_path()?;
    let content = serde_json::to_string_pretty(store)
        .map_err(|e| format!("Failed to serialize password store: {}", e))?;

    fs::write(&file_path, content).map_err(|e| format!("Failed to write password file: {}", e))
}

#[tauri::command(async)]
async fn search_entries(query: String) -> Result<Vec<PasswordEntry>, String> {
    let store = load_password_store()?;

    if query.is_empty() {
        return Ok(store.entries);
    }

    let filtered_entries: Vec<PasswordEntry> = store
        .entries
        .into_iter()
        .filter(|entry| {
            entry.title.to_lowercase().contains(&query.to_lowercase())
                || entry
                    .username
                    .to_lowercase()
                    .contains(&query.to_lowercase())
                || entry.url.as_ref().map_or(false, |url| {
                    url.to_lowercase().contains(&query.to_lowercase())
                })
        })
        .collect();

    Ok(filtered_entries)
}

#[tauri::command]
async fn add_entry(
    title: String,
    username: String,
    password: String,
    url: Option<String>,
    notes: Option<String>,
) -> Result<u32, String> {
    let mut store = load_password_store()?;

    let entry = PasswordEntry {
        id: store.next_id,
        title,
        username,
        password,
        url,
        notes,
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    let entry_id = entry.id;
    store.entries.push(entry);
    store.next_id += 1;

    save_password_store(&store)?;

    Ok(entry_id)
}

#[tauri::command]
async fn update_entry(
    id: u32,
    title: String,
    username: String,
    password: String,
    url: Option<String>,
    notes: Option<String>,
) -> Result<(), String> {
    let mut store = load_password_store()?;

    if let Some(entry) = store.entries.iter_mut().find(|e| e.id == id) {
        entry.title = title;
        entry.username = username;
        entry.password = password;
        entry.url = url;
        entry.notes = notes;

        save_password_store(&store)?;
        Ok(())
    } else {
        Err("Entry not found".to_string())
    }
}

#[tauri::command]
async fn delete_entry(id: u32) -> Result<(), String> {
    let mut store = load_password_store()?;

    if let Some(pos) = store.entries.iter().position(|e| e.id == id) {
        store.entries.remove(pos);
        save_password_store(&store)?;
        Ok(())
    } else {
        Err("Entry not found".to_string())
    }
}

#[tauri::command]
async fn copy_password(entry_id: u32, app_handle: tauri::AppHandle) -> Result<(), String> {
    let store = load_password_store()?;

    if let Some(entry) = store.entries.iter().find(|e| e.id == entry_id) {
        // Use clipboard plugin to copy password directly
        app_handle
            .clipboard()
            .write_text(entry.password.clone())
            .map_err(|e| format!("Failed to copy to clipboard: {}", e))?;

        println!("Password copied for entry: {}", entry.title);
    } else {
        return Err("Entry not found".to_string());
    }

    // Hide the window after copying
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.hide();
    }

    Ok(())
}

#[tauri::command]
async fn copy_username(username: String, app_handle: tauri::AppHandle) -> Result<(), String> {
    // Use clipboard plugin to copy username directly
    app_handle
        .clipboard()
        .write_text(username.clone())
        .map_err(|e| format!("Failed to copy to clipboard: {}", e))?;

    println!("Username copied: {}", username);

    // Hide the window after copying
    if let Some(window) = app_handle.get_webview_window("main") {
        let _ = window.hide();
    }

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        )) // Initialize autostart plugin
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

                let shortcut = Shortcut::new(Some(Modifiers::ALT | Modifiers::SHIFT), Code::KeyP);

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
                                    ShortcutState::Released => {}
                                }
                            }
                        })
                        .build(),
                )?;

                app.global_shortcut().register(shortcut)?;
            }

            // Configure main window to not show in taskbar
            #[cfg(desktop)]
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }

            // Enable autostart on application startup
            #[cfg(desktop)]
            {
                let autostart_manager = app.autolaunch();
                if !autostart_manager.is_enabled().unwrap_or(false) {
                    autostart_manager
                        .enable()
                        .expect("Failed to enable autostart");
                    println!("Autostart enabled successfully.");
                }
            }

            Ok(())
        })
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::Focused(focused) => {
                if !focused {
                    window.hide().unwrap();
                }
            }
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
            add_entry,
            update_entry,
            delete_entry,
            copy_password,
            copy_username
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
