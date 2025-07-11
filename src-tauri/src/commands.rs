use serde_json;
use std::fs;
use std::path::PathBuf;
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_global_shortcut::ShortcutState;

// Security dependencies
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng as AesOsRng},
    Aes256Gcm, Key, Nonce,
};
use argon2::password_hash::rand_core::RngCore;
use argon2::password_hash::{rand_core::OsRng, SaltString};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use base64::{engine::general_purpose, Engine as _};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant};
use zeroize::Zeroize;

// Security-enhanced structures
#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct PasswordEntry {
    id: u32,
    title: String,
    username: String,
    password: String,
    url: Option<String>,
    notes: Option<String>,
    created_at: String,
    modified_at: String,
    password_strength: u8,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct EncryptedPasswordStore {
    encrypted_data: String, // Base64 encoded encrypted JSON
    nonce: String,          // Base64 encoded nonce
    salt: String,           // Base64 encoded salt for key derivation
    iterations: u32,        // PBKDF2 iterations
    version: u8,            // Schema version for future migrations
}

#[derive(serde::Serialize, serde::Deserialize)]
struct PasswordStore {
    entries: Vec<PasswordEntry>,
    next_id: u32,
    created_at: String,
    last_backup: Option<String>,
}

// Security context for managing authentication state
struct SecurityContext {
    master_key: Option<Vec<u8>>,
    last_activity: Instant,
    session_timeout: Duration,
    failed_attempts: u32,
    locked_until: Option<Instant>,
}

impl SecurityContext {
    fn new() -> Self {
        Self {
            master_key: None,
            last_activity: Instant::now(),
            session_timeout: Duration::from_secs(300), // 5 minutes
            failed_attempts: 0,
            locked_until: None,
        }
    }

    fn is_authenticated(&self) -> bool {
        self.master_key.is_some()
            && self.last_activity.elapsed() < self.session_timeout
            && self
                .locked_until
                .map_or(true, |until| Instant::now() > until)
    }

    fn update_activity(&mut self) {
        self.last_activity = Instant::now();
    }

    fn lock_session(&mut self) {
        if let Some(ref mut key) = self.master_key {
            key.zeroize();
        }
        self.master_key = None;
    }

    fn record_failed_attempt(&mut self) {
        self.failed_attempts += 1;
        if self.failed_attempts >= 5 {
            self.locked_until = Some(Instant::now() + Duration::from_secs(300));
            // 5 min lockout
        }
    }

    fn reset_failed_attempts(&mut self) {
        self.failed_attempts = 0;
        self.locked_until = None;
    }
}

// Global security context
static SECURITY_CONTEXT: std::sync::LazyLock<Arc<RwLock<SecurityContext>>> =
    std::sync::LazyLock::new(|| Arc::new(RwLock::new(SecurityContext::new())));

impl Default for PasswordStore {
    fn default() -> Self {
        Self {
            entries: Vec::new(),
            next_id: 1,
            created_at: chrono::Utc::now().to_rfc3339(),
            last_backup: None,
        }
    }
}

// Security utility functions
fn generate_key_from_password(password: &str, salt: &[u8]) -> Result<Vec<u8>, String> {
    let argon2 = Argon2::default();
    let mut key = vec![0u8; 32]; // 256-bit key

    argon2
        .hash_password_into(password.as_bytes(), salt, &mut key)
        .map_err(|e| format!("Key derivation failed: {}", e))?;

    Ok(key)
}

fn encrypt_data(data: &str, key: &[u8]) -> Result<(String, String), String> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));
    let nonce = Aes256Gcm::generate_nonce(&mut AesOsRng);

    let ciphertext = cipher
        .encrypt(&nonce, data.as_bytes())
        .map_err(|e| format!("Encryption failed: {}", e))?;

    Ok((
        general_purpose::STANDARD.encode(&ciphertext),
        general_purpose::STANDARD.encode(&nonce),
    ))
}

fn decrypt_data(encrypted_data: &str, nonce: &str, key: &[u8]) -> Result<String, String> {
    let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(key));

    let ciphertext = general_purpose::STANDARD
        .decode(encrypted_data)
        .map_err(|e| format!("Failed to decode ciphertext: {}", e))?;

    let nonce_bytes = general_purpose::STANDARD
        .decode(nonce)
        .map_err(|e| format!("Failed to decode nonce: {}", e))?;

    let nonce = Nonce::from_slice(&nonce_bytes);

    let plaintext = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|e| format!("Decryption failed: {}", e))?;

    String::from_utf8(plaintext).map_err(|e| format!("Invalid UTF-8 in decrypted data: {}", e))
}

fn calculate_password_strength(password: &str) -> u8 {
    let mut score = 0u8;

    // Length scoring
    if password.len() >= 8 {
        score += 20;
    }
    if password.len() >= 12 {
        score += 15;
    }
    if password.len() >= 16 {
        score += 10;
    }

    // Character variety
    if password.chars().any(|c| c.is_ascii_lowercase()) {
        score += 5;
    }
    if password.chars().any(|c| c.is_ascii_uppercase()) {
        score += 5;
    }
    if password.chars().any(|c| c.is_ascii_digit()) {
        score += 5;
    }
    if password
        .chars()
        .any(|c| "!@#$%^&*()_+-=[]{}|;:,.<>?".contains(c))
    {
        score += 10;
    }

    // Complexity bonus
    let unique_chars = password
        .chars()
        .collect::<std::collections::HashSet<_>>()
        .len();
    if unique_chars > password.len() / 2 {
        score += 10;
    }

    // Penalty for common patterns
    if password.to_lowercase().contains("password")
        || password.to_lowercase().contains("123456")
        || password
            .chars()
            .collect::<Vec<_>>()
            .windows(3)
            .any(|w| w[0] == w[1] && w[1] == w[2])
    {
        score = score.saturating_sub(20);
    }

    score.min(100)
}

// File system functions
fn get_data_file_path() -> Result<PathBuf, String> {
    let app_data_dir = dirs::data_dir()
        .ok_or("Could not find data directory")?
        .join("cocoon-password-manager");

    fs::create_dir_all(&app_data_dir)
        .map_err(|e| format!("Failed to create data directory: {}", e))?;

    Ok(app_data_dir.join("vault.cocoon"))
}

fn get_master_hash_path() -> Result<PathBuf, String> {
    let app_data_dir = dirs::data_dir()
        .ok_or("Could not find data directory")?
        .join("cocoon-password-manager");

    Ok(app_data_dir.join("master.hash"))
}

// Authentication functions
#[tauri::command]
async fn setup_master_password(password: String) -> Result<(), String> {
    if password.len() < 8 {
        return Err("Master password must be at least 8 characters long".to_string());
    }

    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    let password_hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| format!("Failed to hash password: {}", e))?;

    let hash_path = get_master_hash_path()?;
    fs::write(&hash_path, password_hash.to_string())
        .map_err(|e| format!("Failed to save master password hash: {}", e))?;

    // Initialize empty encrypted store
    let empty_store = PasswordStore::default();
    let store_json = serde_json::to_string(&empty_store)
        .map_err(|e| format!("Failed to serialize empty store: {}", e))?;

    let salt_bytes = salt.as_str().as_bytes();
    let key = generate_key_from_password(&password, salt_bytes)?;
    let (encrypted_data, nonce) = encrypt_data(&store_json, &key)?;

    let encrypted_store = EncryptedPasswordStore {
        encrypted_data,
        nonce,
        salt: general_purpose::STANDARD.encode(salt_bytes),
        iterations: 100_000,
        version: 1,
    };

    save_encrypted_store(&encrypted_store)?;

    Ok(())
}

#[tauri::command]
async fn authenticate(password: String) -> Result<bool, String> {
    let mut context = SECURITY_CONTEXT.write().unwrap();

    // Check if locked out
    if let Some(locked_until) = context.locked_until {
        if Instant::now() < locked_until {
            return Err("Account temporarily locked due to failed attempts".to_string());
        }
    }

    let hash_path = get_master_hash_path()?;
    if !hash_path.exists() {
        return Err("Master password not set".to_string());
    }

    let stored_hash = fs::read_to_string(&hash_path)
        .map_err(|e| format!("Failed to read master password hash: {}", e))?;

    let parsed_hash = PasswordHash::new(&stored_hash)
        .map_err(|e| format!("Failed to parse password hash: {}", e))?;

    let argon2 = Argon2::default();
    match argon2.verify_password(password.as_bytes(), &parsed_hash) {
        Ok(_) => {
            // Generate and store master key
            let salt = parsed_hash.salt.unwrap().as_str().as_bytes();
            let key = generate_key_from_password(&password, salt)?;
            context.master_key = Some(key);
            context.update_activity();
            context.reset_failed_attempts();
            Ok(true)
        }
        Err(_) => {
            context.record_failed_attempt();
            Err("Invalid master password".to_string())
        }
    }
}

#[tauri::command]
async fn is_authenticated() -> Result<bool, String> {
    let context = SECURITY_CONTEXT.read().unwrap();
    Ok(context.is_authenticated())
}

#[tauri::command]
async fn lock_session() -> Result<(), String> {
    let mut context = SECURITY_CONTEXT.write().unwrap();
    context.lock_session();
    Ok(())
}

#[tauri::command]
async fn has_master_password() -> Result<bool, String> {
    let hash_path = get_master_hash_path()?;
    Ok(hash_path.exists())
}

// Encrypted store functions
fn save_encrypted_store(store: &EncryptedPasswordStore) -> Result<(), String> {
    let file_path = get_data_file_path()?;
    let content = serde_json::to_string_pretty(store)
        .map_err(|e| format!("Failed to serialize encrypted store: {}", e))?;

    fs::write(&file_path, content).map_err(|e| format!("Failed to write encrypted store: {}", e))
}

fn load_encrypted_store() -> Result<EncryptedPasswordStore, String> {
    let file_path = get_data_file_path()?;

    if !file_path.exists() {
        return Err("Encrypted store not found".to_string());
    }

    let content = fs::read_to_string(&file_path)
        .map_err(|e| format!("Failed to read encrypted store: {}", e))?;

    serde_json::from_str(&content).map_err(|e| format!("Failed to parse encrypted store: {}", e))
}

fn load_password_store() -> Result<PasswordStore, String> {
    let context = SECURITY_CONTEXT.read().unwrap();
    if !context.is_authenticated() {
        return Err("Not authenticated".to_string());
    }

    let key = context
        .master_key
        .as_ref()
        .ok_or("No master key available")?;
    let key_clone = key.clone();
    drop(context);

    let encrypted_store = load_encrypted_store()?;
    let decrypted_data = decrypt_data(
        &encrypted_store.encrypted_data,
        &encrypted_store.nonce,
        &key_clone,
    )?;

    serde_json::from_str(&decrypted_data)
        .map_err(|e| format!("Failed to parse decrypted store: {}", e))
}

fn save_password_store(store: &PasswordStore) -> Result<(), String> {
    let context = SECURITY_CONTEXT.read().unwrap();
    if !context.is_authenticated() {
        return Err("Not authenticated".to_string());
    }

    let key = context
        .master_key
        .as_ref()
        .ok_or("No master key available")?;
    let key_clone = key.clone();
    drop(context);

    let store_json =
        serde_json::to_string(store).map_err(|e| format!("Failed to serialize store: {}", e))?;

    let (encrypted_data, nonce) = encrypt_data(&store_json, &key_clone)?;

    // Load existing encrypted store to preserve salt and other metadata
    let mut encrypted_store = load_encrypted_store().unwrap_or_else(|_| {
        // Create new encrypted store if none exists
        EncryptedPasswordStore {
            encrypted_data: String::new(),
            nonce: String::new(),
            salt: String::new(),
            iterations: 100_000,
            version: 1,
        }
    });

    encrypted_store.encrypted_data = encrypted_data;
    encrypted_store.nonce = nonce;

    save_encrypted_store(&encrypted_store)
}

// Enhanced command functions
#[tauri::command(async)]
async fn search_entries(query: String) -> Result<Vec<PasswordEntry>, String> {
    let mut context = SECURITY_CONTEXT.write().unwrap();
    if !context.is_authenticated() {
        return Err("Not authenticated".to_string());
    }
    context.update_activity();
    drop(context);

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
    let mut context = SECURITY_CONTEXT.write().unwrap();
    if !context.is_authenticated() {
        return Err("Not authenticated".to_string());
    }
    context.update_activity();
    drop(context);

    let mut store = load_password_store()?;
    let password_strength = calculate_password_strength(&password);

    let entry = PasswordEntry {
        id: store.next_id,
        title,
        username,
        password,
        url,
        notes,
        created_at: chrono::Utc::now().to_rfc3339(),
        modified_at: chrono::Utc::now().to_rfc3339(),
        password_strength,
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
    let mut context = SECURITY_CONTEXT.write().unwrap();
    if !context.is_authenticated() {
        return Err("Not authenticated".to_string());
    }
    context.update_activity();
    drop(context);

    let mut store = load_password_store()?;

    if let Some(entry) = store.entries.iter_mut().find(|e| e.id == id) {
        entry.title = title;
        entry.username = username;
        entry.password = password.clone();
        entry.url = url;
        entry.notes = notes;
        entry.modified_at = chrono::Utc::now().to_rfc3339();
        entry.password_strength = calculate_password_strength(&password);

        save_password_store(&store)?;
        Ok(())
    } else {
        Err("Entry not found".to_string())
    }
}

#[tauri::command]
async fn delete_entry(id: u32) -> Result<(), String> {
    let mut context = SECURITY_CONTEXT.write().unwrap();
    if !context.is_authenticated() {
        return Err("Not authenticated".to_string());
    }
    context.update_activity();
    drop(context);

    let mut store = load_password_store()?;

    if let Some(pos) = store.entries.iter().position(|e| e.id == id) {
        store.entries.remove(pos);
        save_password_store(&store)?;
        Ok(())
    } else {
        Err("Entry not found".to_string())
    }
}

// Modified copy_password function without tokio::spawn
#[tauri::command]
async fn copy_password(entry_id: u32, app_handle: tauri::AppHandle) -> Result<(), String> {
    let mut context = SECURITY_CONTEXT.write().unwrap();
    if !context.is_authenticated() {
        return Err("Not authenticated".to_string());
    }
    context.update_activity();
    drop(context);

    let store = load_password_store()?;

    if let Some(entry) = store.entries.iter().find(|e| e.id == entry_id) {
        // Use clipboard plugin to copy password directly
        app_handle
            .clipboard()
            .write_text(entry.password.clone())
            .map_err(|e| format!("Failed to copy to clipboard: {}", e))?;

        println!("Password copied for entry: {}", entry.title);

        // Schedule clipboard clear using a separate command
        let clipboard_handle = app_handle.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(30));
            let _ = clipboard_handle.clipboard().write_text(String::new());
        });
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
    let mut context = SECURITY_CONTEXT.write().unwrap();
    if !context.is_authenticated() {
        return Err("Not authenticated".to_string());
    }
    context.update_activity();
    drop(context);

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

#[tauri::command]
async fn generate_password(
    length: usize,
    include_uppercase: bool,
    include_lowercase: bool,
    include_numbers: bool,
    include_symbols: bool,
) -> Result<String, String> {
    if length < 4 || length > 128 {
        return Err("Password length must be between 4 and 128 characters".to_string());
    }

    let mut charset = String::new();
    if include_lowercase {
        charset.push_str("abcdefghijklmnopqrstuvwxyz");
    }
    if include_uppercase {
        charset.push_str("ABCDEFGHIJKLMNOPQRSTUVWXYZ");
    }
    if include_numbers {
        charset.push_str("0123456789");
    }
    if include_symbols {
        charset.push_str("!@#$%^&*()_+-=[]{}|;:,.<>?");
    }

    if charset.is_empty() {
        return Err("At least one character type must be selected".to_string());
    }

    let chars: Vec<char> = charset.chars().collect();
    let mut password = String::new();
    let mut rng = OsRng;

    for _ in 0..length {
        let idx = (rng.next_u32() as usize) % chars.len();
        password.push(chars[idx]);
    }

    Ok(password)
}

#[tauri::command]
async fn export_vault(export_password: String) -> Result<String, String> {
    let mut context = SECURITY_CONTEXT.write().unwrap();
    if !context.is_authenticated() {
        return Err("Not authenticated".to_string());
    }
    context.update_activity();
    drop(context);

    let store = load_password_store()?;
    let export_data = serde_json::to_string_pretty(&store)
        .map_err(|e| format!("Failed to serialize vault: {}", e))?;

    // Encrypt export with provided password
    let salt = SaltString::generate(&mut OsRng);
    let key = generate_key_from_password(&export_password, salt.as_str().as_bytes())?;
    let (encrypted_data, nonce) = encrypt_data(&export_data, &key)?;

    let export_structure = serde_json::json!({
        "version": 1,
        "encrypted_data": encrypted_data,
        "nonce": nonce,
        "salt": general_purpose::STANDARD.encode(salt.as_str().as_bytes()),
        "exported_at": chrono::Utc::now().to_rfc3339()
    });

    serde_json::to_string_pretty(&export_structure)
        .map_err(|e| format!("Failed to serialize export: {}", e))
}

// Helper function to start session timeout checker
fn start_session_timeout_checker(app_handle: tauri::AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
            let mut context = SECURITY_CONTEXT.write().unwrap();
            if context.is_authenticated()
                && context.last_activity.elapsed() > context.session_timeout
            {
                context.lock_session();
                println!("Session timed out and locked");

                // Notify frontend to show login screen
                if let Some(window) = app_handle.get_webview_window("main") {
                    let _ = window.emit("session_timeout", ());
                }
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            setup_master_password,
            authenticate,
            is_authenticated,
            lock_session,
            has_master_password,
            search_entries,
            add_entry,
            update_entry,
            delete_entry,
            copy_password,
            copy_username,
            generate_password,
            export_vault
        ])
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

            // Configure main window
            #[cfg(desktop)]
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }

            // Enable autostart
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
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app_handle, event| {
            match event {
                RunEvent::Ready => {
                    // Start session timeout checker after the app is ready
                    start_session_timeout_checker(app_handle.clone());
                }
                RunEvent::WindowEvent { label, event, .. } => {
                    match event {
                        WindowEvent::Focused(focused) => {
                            if !focused {
                                // Update activity when window loses focus
                                let mut context = SECURITY_CONTEXT.write().unwrap();
                                if context.is_authenticated() {
                                    context.update_activity();
                                }
                                drop(context);
                                if let Some(window) = app_handle.get_webview_window(&label) {
                                    let _ = window.hide();
                                }
                            }
                        }
                        _ => {}
                    }
                }
                RunEvent::TrayIconEvent(event) =>
                {
                    #[cfg(desktop)]
                    match event {
                        TrayIconEvent::Click { .. } => {
                            if let Some(window) = app_handle.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.set_focus();
                                let _ = window.center();
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        });
}
