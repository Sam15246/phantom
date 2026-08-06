use tauri::{AppHandle, Manager};

#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowDisplayAffinity, SetWindowLongPtrW, GWL_EXSTYLE,
    WS_EX_TOOLWINDOW,
};

/// WDA_EXCLUDEFROMCAPTURE = 0x00000011
#[cfg(target_os = "windows")]
const WDA_EXCLUDEFROMCAPTURE: u32 = 0x00000011;

/// Apply content protection — makes window invisible to all screen capture
#[cfg(target_os = "windows")]
pub fn apply_content_protection(app: &AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("Failed to get main window")?;

    let hwnd = window.hwnd().map_err(|e| format!("Failed to get HWND: {e}"))?;

    let result = unsafe { SetWindowDisplayAffinity(hwnd.0, WDA_EXCLUDEFROMCAPTURE) };

    if result == 0 {
        Err("SetWindowDisplayAffinity failed".to_string())
    } else {
        #[cfg(debug_assertions)]
        println!("Content protection applied successfully");
        Ok(())
    }
}

/// Hide the main window from Alt+Tab by setting WS_EX_TOOLWINDOW
#[cfg(target_os = "windows")]
pub fn hide_from_alt_tab(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if let Ok(hwnd) = window.hwnd() {
            unsafe {
                let ex_style = GetWindowLongPtrW(hwnd.0, GWL_EXSTYLE);
                SetWindowLongPtrW(hwnd.0, GWL_EXSTYLE, ex_style | WS_EX_TOOLWINDOW as isize);
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn hide_from_alt_tab(_app: &AppHandle) {}

#[cfg(not(target_os = "windows"))]
pub fn apply_content_protection(_app: &AppHandle) -> Result<(), String> {
    Ok(())
}

/// Toggle click-through mode
#[tauri::command]
pub fn set_click_through(app: AppHandle, enabled: bool) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("Failed to get main window")?;

    window
        .set_ignore_cursor_events(enabled)
        .map_err(|e| format!("Failed to set click-through: {e}"))?;

    Ok(())
}

/// Show or hide the overlay window
#[tauri::command]
pub fn toggle_overlay_visibility(app: AppHandle) -> Result<bool, String> {
    let window = app
        .get_webview_window("main")
        .ok_or("Failed to get main window")?;

    let visible = window
        .is_visible()
        .map_err(|e| format!("Failed to check visibility: {e}"))?;

    if visible {
        window.hide().map_err(|e| format!("Failed to hide: {e}"))?;
    } else {
        window.show().map_err(|e| format!("Failed to show: {e}"))?;
    }

    Ok(!visible)
}

/// Move overlay window by delta pixels
#[tauri::command]
pub fn move_overlay(app: AppHandle, dx: i32, dy: i32) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("Failed to get main window")?;

    let pos = window
        .outer_position()
        .map_err(|e| format!("Failed to get position: {e}"))?;

    window
        .set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(
            pos.x + dx,
            pos.y + dy,
        )))
        .map_err(|e| format!("Failed to move window: {e}"))?;

    Ok(())
}

/// Snap overlay to top-center (near webcam)
#[tauri::command]
pub fn snap_to_webcam(app: AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("Failed to get main window")?;

    let monitor = window
        .current_monitor()
        .map_err(|e| format!("Failed to get monitor: {e}"))?
        .ok_or("No monitor found")?;

    let screen_width = monitor.size().width;
    let win_size = window
        .outer_size()
        .map_err(|e| format!("Failed to get window size: {e}"))?;

    let x = (screen_width as i32 - win_size.width as i32) / 2;
    let y = 40;

    window
        .set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(x, y)))
        .map_err(|e| format!("Failed to set position: {e}"))?;

    Ok(())
}

/// Open the settings window
#[tauri::command]
pub fn open_settings(app: AppHandle) -> Result<(), String> {
    if app.get_webview_window("settings").is_some() {
        return Ok(());
    }

    let _settings_window = tauri::WebviewWindowBuilder::new(
        &app,
        "settings",
        tauri::WebviewUrl::App("settings.html".into()),
    )
    .title("Audio Device Properties")
    .inner_size(420.0, 600.0)
    .resizable(false)
    .decorations(true)
    .center()
    .build()
    .map_err(|e| format!("Failed to open settings: {e}"))?;

    // Apply content protection to settings window
    #[cfg(target_os = "windows")]
    {
        if let Ok(hwnd) = _settings_window.hwnd() {
            unsafe {
                SetWindowDisplayAffinity(hwnd.0, WDA_EXCLUDEFROMCAPTURE);
                // Hide from Alt+Tab
                let ex_style = GetWindowLongPtrW(hwnd.0, GWL_EXSTYLE);
                SetWindowLongPtrW(hwnd.0, GWL_EXSTYLE, ex_style | WS_EX_TOOLWINDOW as isize);
            }
        }
    }

    Ok(())
}
