use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_autostart::ManagerExt;
use tauri_plugin_global_shortcut::ShortcutState;

#[cfg(target_os = "macos")]
use core_graphics::event::{CGEvent, CGEventTapLocation};

// macOS NSPanel imports
#[cfg(target_os = "macos")]
use objc2::msg_send;
#[cfg(target_os = "macos")]
use objc2::runtime::AnyObject;
#[cfg(target_os = "macos")]
use objc2_app_kit::{NSFloatingWindowLevel, NSWindowCollectionBehavior};

// Add input simulation dependencies
#[cfg(target_os = "windows")]
use winapi::um::winuser::{SendInput, INPUT, INPUT_KEYBOARD, KEYEVENTF_KEYUP, VK_TAB};

#[cfg(target_os = "linux")]
use x11::xlib;

// Security dependencies
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng as AesOsRng},
    Aes256Gcm, Key, Nonce,
};
use argon2::password_hash::rand_core::RngCore;
use argon2::password_hash::{rand_core::OsRng, SaltString};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use base64::{engine::general_purpose, Engine as _};

// Security-enhanced structures (keeping your existing structures)
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
    encrypted_data: String,
    nonce: String,
    salt: String,
    iterations: u32,
    version: u8,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct PasswordStore {
    entries: Vec<PasswordEntry>,
    next_id: u32,
    created_at: String,
    last_backup: Option<String>,
}

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

#[derive(Clone)]
struct FocusState {
    target_app_pid: Option<u32>,
    last_active_window: Option<String>,
}

// Global state for focus management
lazy_static::lazy_static! {
    static ref FOCUS_STATE: Arc<Mutex<FocusState>> = Arc::new(Mutex::new(FocusState {
        target_app_pid: None,
        last_active_window: None,
    }));
}

// Enhanced macOS focus management
#[cfg(target_os = "macos")]
fn capture_current_focus() -> Result<(), String> {
    unsafe {
        use objc2_app_kit::NSWorkspace;

        let workspace = NSWorkspace::sharedWorkspace();
        if let Some(front_app) = workspace.frontmostApplication() {
            let pid = front_app.processIdentifier();
            let bundle_id = front_app.bundleIdentifier();

            let mut focus_state = FOCUS_STATE.lock().unwrap();
            focus_state.target_app_pid = Some(pid as u32);
            focus_state.last_active_window = bundle_id.map(|id| id.to_string());
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn restore_target_focus() -> Result<(), String> {
    unsafe {
        use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};

        let focus_state = FOCUS_STATE.lock().unwrap();
        if let Some(pid) = focus_state.target_app_pid {
            if let Some(app) =
                NSRunningApplication::runningApplicationWithProcessIdentifier(pid as i32)
            {
                app.activateWithOptions(NSApplicationActivationOptions(0));
            }
        }
    }
    Ok(())
}

// Enhanced window configuration for better Spotlight-like behavior
#[cfg(target_os = "macos")]
fn configure_spotlight_panel(window: &tauri::WebviewWindow) -> Result<(), String> {
    unsafe {
        let ns_window = window
            .ns_window()
            .map_err(|e| format!("Failed to get NSWindow: {}", e))?;
        let ns_window_ptr = ns_window as *mut AnyObject;

        // Set to highest floating level (above fullscreen apps)
        let _: () = msg_send![ns_window_ptr, setLevel: NSFloatingWindowLevel + 1];

        // Enhanced collection behavior for Spotlight-like experience
        let collection_behavior = NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::IgnoresCycle
            | NSWindowCollectionBehavior::Transient;

        let _: () = msg_send![ns_window_ptr, setCollectionBehavior: collection_behavior.bits()];

        // Basic window configuration
        let _: () = msg_send![ns_window_ptr, setMovableByWindowBackground: true];
        let _: () = msg_send![ns_window_ptr, setAcceptsMouseMovedEvents: true];
        let _: () = msg_send![ns_window_ptr, setHidesOnDeactivate: false];
    }
    Ok(())
}

// Enhanced typing simulation with focus preservation
#[cfg(target_os = "macos")]
fn simulate_typing_with_focus_restore(text: &str) -> Result<(), String> {
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    // Restore focus to target application first
    restore_target_focus()?;

    // Small delay to ensure target application regains focus
    std::thread::sleep(std::time::Duration::from_millis(200));

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "Failed to create event source")?;

    for ch in text.chars() {
        if let Ok(event) = CGEvent::new_keyboard_event(source.clone(), 0, true) {
            event.set_string_from_utf16_unchecked(&[ch as u16]);
            event.post(CGEventTapLocation::HID);
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn simulate_enter() -> Result<(), String> {
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "Failed to create event source")?;

    // Enter key press (keycode 36 on macOS)
    if let Ok(event) = CGEvent::new_keyboard_event(source.clone(), 36, true) {
        event.post(CGEventTapLocation::HID);
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Enter key release
        if let Ok(event_up) = CGEvent::new_keyboard_event(source, 36, false) {
            event_up.post(CGEventTapLocation::HID);
        }
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn simulate_enter() -> Result<(), String> {
    use winapi::um::winuser::{SendInput, INPUT, INPUT_KEYBOARD, KEYEVENTF_KEYUP, VK_RETURN};

    let mut input_down = INPUT {
        type_: INPUT_KEYBOARD,
        u: unsafe { std::mem::zeroed() },
    };

    let mut input_up = INPUT {
        type_: INPUT_KEYBOARD,
        u: unsafe { std::mem::zeroed() },
    };

    unsafe {
        // Enter key down
        input_down.u.ki_mut().wVk = VK_RETURN as u16;
        input_down.u.ki_mut().dwFlags = 0;

        // Enter key up
        input_up.u.ki_mut().wVk = VK_RETURN as u16;
        input_up.u.ki_mut().dwFlags = KEYEVENTF_KEYUP;

        if SendInput(1, &mut input_down, std::mem::size_of::<INPUT>() as i32) != 1 {
            return Err("Failed to send enter key down".to_string());
        }

        std::thread::sleep(std::time::Duration::from_millis(10));

        if SendInput(1, &mut input_up, std::mem::size_of::<INPUT>() as i32) != 1 {
            return Err("Failed to send enter key up".to_string());
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn simulate_enter() -> Result<(), String> {
    use std::ptr;

    unsafe {
        let display = x11::xlib::XOpenDisplay(ptr::null());
        if display.is_null() {
            return Err("Failed to open X11 display".to_string());
        }

        let enter_keycode = 36; // Enter key on most X11 systems

        // Key press
        let mut event: x11::xlib::XKeyEvent = std::mem::zeroed();
        event.type_ = x11::xlib::KeyPress;
        event.display = display;
        event.keycode = enter_keycode;
        event.state = 0;

        x11::xlib::XSendEvent(
            display,
            x11::xlib::PointerWindow,
            x11::xlib::True,
            x11::xlib::KeyPressMask,
            &mut event as *mut _ as *mut x11::xlib::XEvent,
        );

        // Key release
        event.type_ = x11::xlib::KeyRelease;
        x11::xlib::XSendEvent(
            display,
            x11::xlib::PointerWindow,
            x11::xlib::True,
            x11::xlib::KeyReleaseMask,
            &mut event as *mut _ as *mut x11::xlib::XEvent,
        );

        x11::xlib::XFlush(display);
        x11::xlib::XCloseDisplay(display);
    }

    Ok(())
}

#[tauri::command]
async fn auto_fill_and_login_spotlight(
    entry_id: u32,
    master_password: String,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let store = load_password_store(&master_password)?;

    if let Some(entry) = store.entries.iter().find(|e| e.id == entry_id) {
        // Hide Cocoon window
        if let Some(window) = app_handle.get_webview_window("main") {
            let _ = window.hide();
        }

        #[cfg(target_os = "macos")]
        {
            // Restore focus to target application
            restore_target_focus()?;
            std::thread::sleep(std::time::Duration::from_millis(200));

            // Type credentials and login
            simulate_typing_with_focus_restore(&entry.username)?;
            simulate_tab()?;
            std::thread::sleep(std::time::Duration::from_millis(100));
            simulate_typing_with_focus_restore(&entry.password)?;
            std::thread::sleep(std::time::Duration::from_millis(200));
            simulate_enter()?; // Press Enter to login
        }

        #[cfg(not(target_os = "macos"))]
        {
            std::thread::sleep(std::time::Duration::from_millis(500));
            simulate_typing(&entry.username)?;
            simulate_tab()?;
            std::thread::sleep(std::time::Duration::from_millis(100));
            simulate_typing(&entry.password)?;
            std::thread::sleep(std::time::Duration::from_millis(200));
            simulate_enter()?; // Press Enter to login
        }
    } else {
        return Err("Entry not found".to_string());
    }

    Ok(())
}

#[tauri::command]
async fn press_enter_after_autofill(_app_handle: tauri::AppHandle) -> Result<(), String> {
    std::thread::sleep(std::time::Duration::from_millis(300));
    
    simulate_enter()?;
    
    Ok(())
}

// Enhanced commands with better focus management
#[tauri::command]
async fn type_username_spotlight(
    entry_id: u32,
    master_password: String,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let store = load_password_store(&master_password)?;

    if let Some(entry) = store.entries.iter().find(|e| e.id == entry_id) {
        // Hide Cocoon window
        if let Some(window) = app_handle.get_webview_window("main") {
            let _ = window.hide();
        }

        // Type with focus restoration
        #[cfg(target_os = "macos")]
        simulate_typing_with_focus_restore(&entry.username)?;

        #[cfg(not(target_os = "macos"))]
        {
            std::thread::sleep(std::time::Duration::from_millis(500));
            simulate_typing(&entry.username)?;
        }
    } else {
        return Err("Entry not found".to_string());
    }

    Ok(())
}

#[tauri::command]
async fn type_password_spotlight(
    entry_id: u32,
    master_password: String,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let store = load_password_store(&master_password)?;

    if let Some(entry) = store.entries.iter().find(|e| e.id == entry_id) {
        // Hide Cocoon window
        if let Some(window) = app_handle.get_webview_window("main") {
            let _ = window.hide();
        }

        // Type with focus restoration
        #[cfg(target_os = "macos")]
        simulate_typing_with_focus_restore(&entry.password)?;

        #[cfg(not(target_os = "macos"))]
        {
            std::thread::sleep(std::time::Duration::from_millis(1000));
            simulate_typing(&entry.password)?;
        }
    } else {
        return Err("Entry not found".to_string());
    }

    Ok(())
}

#[tauri::command]
async fn auto_fill_credentials_spotlight(
    entry_id: u32,
    master_password: String,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let store = load_password_store(&master_password)?;

    if let Some(entry) = store.entries.iter().find(|e| e.id == entry_id) {
        // Hide Cocoon window
        if let Some(window) = app_handle.get_webview_window("main") {
            let _ = window.hide();
        }

        #[cfg(target_os = "macos")]
        {
            // Restore focus to target application
            restore_target_focus()?;
            std::thread::sleep(std::time::Duration::from_millis(200));

            // Type credentials
            simulate_typing_with_focus_restore(&entry.username)?;
            simulate_tab()?;
            std::thread::sleep(std::time::Duration::from_millis(100));
            simulate_typing_with_focus_restore(&entry.password)?;
        }

        #[cfg(not(target_os = "macos"))]
        {
            std::thread::sleep(std::time::Duration::from_millis(500));
            simulate_typing(&entry.username)?;
            simulate_tab()?;
            std::thread::sleep(std::time::Duration::from_millis(100));
            simulate_typing(&entry.password)?;
        }
    } else {
        return Err("Entry not found".to_string());
    }

    Ok(())
}

// Add a command to focus the search input from the frontend
#[tauri::command]
async fn focus_search_input(app_handle: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app_handle.get_webview_window("main") {
        // Ensure window is key and focused
        let _ = window.set_focus();

        #[cfg(target_os = "macos")]
        unsafe {
            use objc2::msg_send;
            use objc2::runtime::AnyObject;

            if let Ok(ns_window) = window.ns_window() {
                let ns_window_ptr = ns_window as *mut AnyObject;
                let _: () = msg_send![ns_window_ptr, makeKeyAndOrderFront: std::ptr::null_mut::<AnyObject>()];
            }
        }
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn simulate_tab() -> Result<(), String> {
    use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};

    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| "Failed to create event source")?;

    // Tab key press
    if let Ok(event) = CGEvent::new_keyboard_event(source.clone(), 48, true) {
        event.post(CGEventTapLocation::HID);
        std::thread::sleep(std::time::Duration::from_millis(50));

        // Tab key release
        if let Ok(event_up) = CGEvent::new_keyboard_event(source, 48, false) {
            event_up.post(CGEventTapLocation::HID);
        }
    }

    Ok(())
}

// Windows and Linux implementations (keeping your existing code)
#[cfg(target_os = "windows")]
fn simulate_typing(text: &str) -> Result<(), String> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use winapi::um::winuser::{SendInput, INPUT, INPUT_KEYBOARD, KEYEVENTF_UNICODE};

    let wide_text: Vec<u16> = OsStr::new(text).encode_wide().collect();

    for &ch in &wide_text {
        let mut input = INPUT {
            type_: INPUT_KEYBOARD,
            u: unsafe { std::mem::zeroed() },
        };

        unsafe {
            input.u.ki_mut().wVk = 0;
            input.u.ki_mut().wScan = ch;
            input.u.ki_mut().dwFlags = KEYEVENTF_UNICODE;
            input.u.ki_mut().time = 0;
            input.u.ki_mut().dwExtraInfo = 0;

            if SendInput(1, &mut input, std::mem::size_of::<INPUT>() as i32) != 1 {
                return Err("Failed to send input".to_string());
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(10));
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn simulate_tab() -> Result<(), String> {
    use winapi::um::winuser::{SendInput, INPUT, INPUT_KEYBOARD, KEYEVENTF_KEYUP, VK_TAB};

    let mut input_down = INPUT {
        type_: INPUT_KEYBOARD,
        u: unsafe { std::mem::zeroed() },
    };

    let mut input_up = INPUT {
        type_: INPUT_KEYBOARD,
        u: unsafe { std::mem::zeroed() },
    };

    unsafe {
        // Tab key down
        input_down.u.ki_mut().wVk = VK_TAB as u16;
        input_down.u.ki_mut().dwFlags = 0;

        // Tab key up
        input_up.u.ki_mut().wVk = VK_TAB as u16;
        input_up.u.ki_mut().dwFlags = KEYEVENTF_KEYUP;

        if SendInput(1, &mut input_down, std::mem::size_of::<INPUT>() as i32) != 1 {
            return Err("Failed to send tab key down".to_string());
        }

        std::thread::sleep(std::time::Duration::from_millis(10));

        if SendInput(1, &mut input_up, std::mem::size_of::<INPUT>() as i32) != 1 {
            return Err("Failed to send tab key up".to_string());
        }
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn simulate_typing(text: &str) -> Result<(), String> {
    use std::ptr;

    unsafe {
        let display = x11::xlib::XOpenDisplay(ptr::null());
        if display.is_null() {
            return Err("Failed to open X11 display".to_string());
        }

        for ch in text.chars() {
            let keycode = ch as u32;

            // Key press
            let mut event: x11::xlib::XKeyEvent = std::mem::zeroed();
            event.type_ = x11::xlib::KeyPress;
            event.display = display;
            event.keycode = keycode;
            event.state = 0;

            x11::xlib::XSendEvent(
                display,
                x11::xlib::PointerWindow,
                x11::xlib::True,
                x11::xlib::KeyPressMask,
                &mut event as *mut _ as *mut x11::xlib::XEvent,
            );

            // Key release
            event.type_ = x11::xlib::KeyRelease;
            x11::xlib::XSendEvent(
                display,
                x11::xlib::PointerWindow,
                x11::xlib::True,
                x11::xlib::KeyReleaseMask,
                &mut event as *mut _ as *mut x11::xlib::XEvent,
            );

            x11::xlib::XFlush(display);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        x11::xlib::XCloseDisplay(display);
    }

    Ok(())
}

#[cfg(target_os = "linux")]
fn simulate_tab() -> Result<(), String> {
    use std::ptr;

    unsafe {
        let display = x11::xlib::XOpenDisplay(ptr::null());
        if display.is_null() {
            return Err("Failed to open X11 display".to_string());
        }

        let tab_keycode = 23; // Tab key on most X11 systems

        // Key press
        let mut event: x11::xlib::XKeyEvent = std::mem::zeroed();
        event.type_ = x11::xlib::KeyPress;
        event.display = display;
        event.keycode = tab_keycode;
        event.state = 0;

        x11::xlib::XSendEvent(
            display,
            x11::xlib::PointerWindow,
            x11::xlib::True,
            x11::xlib::KeyPressMask,
            &mut event as *mut _ as *mut x11::xlib::XEvent,
        );

        // Key release
        event.type_ = x11::xlib::KeyRelease;
        x11::xlib::XSendEvent(
            display,
            x11::xlib::PointerWindow,
            x11::xlib::True,
            x11::xlib::KeyReleaseMask,
            &mut event as *mut _ as *mut x11::xlib::XEvent,
        );

        x11::xlib::XFlush(display);
        x11::xlib::XCloseDisplay(display);
    }

    Ok(())
}

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

// Security utility functions (keeping existing functions)
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

// Authentication functions (keeping existing functions)
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
fn verify_master_password(password: &str) -> Result<Vec<u8>, String> {
    let hash_path = get_master_hash_path()?;
    if !hash_path.exists() {
        return Err("Master password not set".to_string());
    }

    let stored_hash = fs::read_to_string(&hash_path)
        .map_err(|e| format!("Failed to read master password hash: {}", e))?;

    let parsed_hash = PasswordHash::new(&stored_hash)
        .map_err(|e| format!("Failed to parse password hash: {}", e))?;

    let argon2 = Argon2::default();
    argon2
        .verify_password(password.as_bytes(), &parsed_hash)
        .map_err(|_| "Invalid master password".to_string())?;

    // Generate and return the key
    let salt = parsed_hash.salt.unwrap().as_str().as_bytes();
    generate_key_from_password(password, salt)
}

#[tauri::command]
async fn has_master_password() -> Result<bool, String> {
    let hash_path = get_master_hash_path()?;
    Ok(hash_path.exists())
}

// Encrypted store functions (keeping existing functions)
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

fn load_password_store(master_password: &str) -> Result<PasswordStore, String> {
    let key = verify_master_password(master_password)?;
    let encrypted_store = load_encrypted_store()?;
    let decrypted_data = decrypt_data(
        &encrypted_store.encrypted_data,
        &encrypted_store.nonce,
        &key,
    )?;

    serde_json::from_str(&decrypted_data)
        .map_err(|e| format!("Failed to parse decrypted store: {}", e))
}

fn save_password_store(store: &PasswordStore, master_password: &str) -> Result<(), String> {
    let key = verify_master_password(master_password)?;
    let store_json =
        serde_json::to_string(store).map_err(|e| format!("Failed to serialize store: {}", e))?;

    let (encrypted_data, nonce) = encrypt_data(&store_json, &key)?;

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

#[tauri::command]
async fn auto_fill_credentials_spotlight_with_login(
    entry_id: u32,
    master_password: String,
    press_enter: bool,
    app_handle: tauri::AppHandle,
) -> Result<(), String> {
    let store = load_password_store(&master_password)?;

    if let Some(entry) = store.entries.iter().find(|e| e.id == entry_id) {
        // Hide Cocoon window
        if let Some(window) = app_handle.get_webview_window("main") {
            let _ = window.hide();
        }

        #[cfg(target_os = "macos")]
        {
            // Restore focus to target application
            restore_target_focus()?;
            std::thread::sleep(std::time::Duration::from_millis(200));

            // Type credentials
            simulate_typing_with_focus_restore(&entry.username)?;
            simulate_tab()?;
            std::thread::sleep(std::time::Duration::from_millis(100));
            simulate_typing_with_focus_restore(&entry.password)?;
            
            // Optionally press Enter to login
            if press_enter {
                std::thread::sleep(std::time::Duration::from_millis(200));
                simulate_enter()?;
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            std::thread::sleep(std::time::Duration::from_millis(500));
            simulate_typing(&entry.username)?;
            simulate_tab()?;
            std::thread::sleep(std::time::Duration::from_millis(100));
            simulate_typing(&entry.password)?;
            
            // Optionally press Enter to login
            if press_enter {
                std::thread::sleep(std::time::Duration::from_millis(200));
                simulate_enter()?;
            }
        }
    } else {
        return Err("Entry not found".to_string());
    }

    Ok(())
}

#[tauri::command(async)]
async fn search_entries(
    query: String,
    master_password: String,
) -> Result<Vec<PasswordEntry>, String> {
    let store = load_password_store(&master_password)?;

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
    master_password: String,
) -> Result<u32, String> {
    let mut store = load_password_store(&master_password)?;
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

    save_password_store(&store, &master_password)?;

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
    master_password: String,
) -> Result<(), String> {
    let mut store = load_password_store(&master_password)?;

    if let Some(entry) = store.entries.iter_mut().find(|e| e.id == id) {
        entry.title = title;
        entry.username = username;
        entry.password = password.clone();
        entry.url = url;
        entry.notes = notes;
        entry.modified_at = chrono::Utc::now().to_rfc3339();
        entry.password_strength = calculate_password_strength(&password);

        save_password_store(&store, &master_password)?;
        Ok(())
    } else {
        Err("Entry not found".to_string())
    }
}

#[tauri::command]
async fn delete_entry(id: u32, master_password: String) -> Result<(), String> {
    let mut store = load_password_store(&master_password)?;

    if let Some(pos) = store.entries.iter().position(|e| e.id == id) {
        store.entries.remove(pos);
        save_password_store(&store, &master_password)?;
        Ok(())
    } else {
        Err("Entry not found".to_string())
    }
}

#[tauri::command]
async fn get_entry_by_id(id: u32, master_password: String) -> Result<PasswordEntry, String> {
    let store = load_password_store(&master_password)?;

    store
        .entries
        .into_iter()
        .find(|entry| entry.id == id)
        .ok_or_else(|| "Entry not found".to_string())
}

#[tauri::command]
async fn hide_window(app_handle: tauri::AppHandle) -> Result<(), String> {
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
async fn export_vault(export_password: String, master_password: String) -> Result<String, String> {
    let store = load_password_store(&master_password)?;
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ))
        .invoke_handler(tauri::generate_handler![
            setup_master_password,
            verify_master_password,
            has_master_password,
            search_entries,
            add_entry,
            update_entry,
            delete_entry,
            type_username_spotlight,
            type_password_spotlight,
            auto_fill_credentials_spotlight,
            generate_password,
            get_entry_by_id,
            export_vault,
            hide_window,
    auto_fill_and_login_spotlight,
    press_enter_after_autofill,
    auto_fill_credentials_spotlight_with_login,
            focus_search_input
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

            // Setup enhanced global shortcut with focus capture
            #[cfg(desktop)]
            {
                use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};
                let shortcut = Shortcut::new(Some(Modifiers::CONTROL), Code::KeyP);

                app.handle().plugin(
                    tauri_plugin_global_shortcut::Builder::new()
                        .with_handler(move |_app, received_shortcut, event| {
                            if received_shortcut == &shortcut {
                                match event.state() {
                                    ShortcutState::Pressed => {
                                        if let Some(window) = _app.get_webview_window("main") {
                                            let is_visible = window.is_visible().unwrap_or(false);

                                            if is_visible {
                                                // Hide like Spotlight
                                                let _ = window.hide();
                                            } else {
                                                // Capture current focus before showing Cocoon
                                                #[cfg(target_os = "macos")]
                                                let _ = capture_current_focus();

                                                // Configure as Spotlight-like panel
                                                #[cfg(target_os = "macos")]
                                                {
                                                    if let Err(e) = configure_spotlight_panel(&window) {
                                                        eprintln!("Failed to configure Spotlight panel: {}", e);
                                                    }
                                                }

                                                // Show and position like Spotlight
                                                let _ = window.show();
                                                let _ = window.center();
                                                let _ = window.set_focus();

                                                #[cfg(target_os = "macos")]
                                                unsafe {
                                                    use objc2::runtime::AnyObject;
                                                    use objc2::msg_send;

                                                    if let Ok(ns_window) = window.ns_window() {
                                                        let ns_window_ptr = ns_window as *mut AnyObject;

                                                        // Make key window like Spotlight
                                                        let _: () = msg_send![ns_window_ptr, makeKeyAndOrderFront: std::ptr::null_mut::<AnyObject>()];
                                                    }
                                                }

                                                // Focus search input
                                                let _ = window.emit("focus-search-input", ());
                                            }
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
                // Configure as Spotlight-like panel on macOS
                #[cfg(target_os = "macos")]
                {
                    if let Err(e) = configure_spotlight_panel(&window) {
                        eprintln!("Failed to configure Spotlight panel: {}", e);
                    }
                }

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
                }
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while running tauri application")
        .run(|app_handle, event| match event {
            RunEvent::WindowEvent { label, event, .. } => match event {
                WindowEvent::Focused(focused) => {
                    if !focused {
                        // Spotlight-like behavior: hide when losing focus
                        if let Some(window) = app_handle.get_webview_window(&label) {
                            let window_clone = window.clone();
                            std::thread::spawn(move || {
                                std::thread::sleep(std::time::Duration::from_millis(100));
                                if !window_clone.is_focused().unwrap_or(false) {
                                    let _ = window_clone.hide();
                                }
                            });
                        }
                    }
                }
                WindowEvent::CloseRequested { api, .. } => {
                    // Prevent closing, just hide
                    api.prevent_close();
                    if let Some(window) = app_handle.get_webview_window(&label) {
                        let _ = window.hide();
                    }
                }
                _ => {}
            },
            RunEvent::TrayIconEvent(event) => {
                #[cfg(desktop)]
                match event {
                    TrayIconEvent::Click { .. } => {
                        if let Some(window) = app_handle.get_webview_window("main") {
                            #[cfg(target_os = "macos")]
                            let _ = capture_current_focus();
                            let _ = window.show();
                            let _ = window.set_focus();
                            let _ = window.center();
                            let _ = window.emit("focus-search-input", ());
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        });
}
