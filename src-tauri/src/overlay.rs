use tauri::{AppHandle, Manager};

#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowDisplayAffinity, SetWindowLongPtrW, GWL_EXSTYLE,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
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

/// Hide the main window from Alt+Tab / Win+Tab by setting WS_EX_TOOLWINDOW + WS_EX_NOACTIVATE
#[cfg(target_os = "windows")]
pub fn hide_from_alt_tab(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        if let Ok(hwnd) = window.hwnd() {
            unsafe {
                let ex_style = GetWindowLongPtrW(hwnd.0, GWL_EXSTYLE);
                SetWindowLongPtrW(
                    hwnd.0,
                    GWL_EXSTYLE,
                    ex_style | WS_EX_TOOLWINDOW as isize | WS_EX_NOACTIVATE as isize,
                );
            }
        }
    }
}

/// Update WS_EX_NOACTIVATE based on click-through state.
/// When interactive (click-through off), remove NOACTIVATE so user can type in follow-up.
/// When passive (click-through on), add NOACTIVATE so window never appears in task switcher.
#[cfg(target_os = "windows")]
pub fn update_noactivate(app: &AppHandle, click_through: bool) {
    if let Some(window) = app.get_webview_window("main") {
        if let Ok(hwnd) = window.hwnd() {
            unsafe {
                let ex_style = GetWindowLongPtrW(hwnd.0, GWL_EXSTYLE);
                let new_style = if click_through {
                    ex_style | WS_EX_NOACTIVATE as isize
                } else {
                    ex_style & !(WS_EX_NOACTIVATE as isize)
                };
                SetWindowLongPtrW(hwnd.0, GWL_EXSTYLE, new_style);
            }
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn update_noactivate(_app: &AppHandle, _click_through: bool) {}

#[cfg(not(target_os = "windows"))]
pub fn hide_from_alt_tab(_app: &AppHandle) {}

#[cfg(not(target_os = "windows"))]
pub fn apply_content_protection(_app: &AppHandle) -> Result<(), String> {
    Ok(())
}

/// Make the process blend into Task Manager as a background system process
#[cfg(target_os = "windows")]
pub fn apply_process_stealth() {
    use windows_sys::Win32::System::Threading::{
        GetCurrentProcess, SetPriorityClass, BELOW_NORMAL_PRIORITY_CLASS,
    };

    unsafe {
        // Set priority to Below Normal — process appears as background/service in Task Manager
        SetPriorityClass(GetCurrentProcess(), BELOW_NORMAL_PRIORITY_CLASS);
    }

    // Windows 11 efficiency mode (PROCESS_POWER_THROTTLING) — shows green leaf icon
    // This uses SetProcessInformation which may not be available on older Windows
    #[allow(non_snake_case)]
    {
        use windows_sys::Win32::System::Threading::{
            GetCurrentProcess, SetProcessInformation, ProcessPowerThrottling,
            PROCESS_POWER_THROTTLING_STATE, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            PROCESS_POWER_THROTTLING_CURRENT_VERSION,
        };

        let mut state = PROCESS_POWER_THROTTLING_STATE {
            Version: PROCESS_POWER_THROTTLING_CURRENT_VERSION,
            ControlMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
            StateMask: PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
        };

        unsafe {
            SetProcessInformation(
                GetCurrentProcess(),
                ProcessPowerThrottling,
                &mut state as *mut _ as *mut _,
                std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
            );
        }
    }
}

#[cfg(not(target_os = "windows"))]
pub fn apply_process_stealth() {}

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

/// Resize overlay window by delta pixels
#[tauri::command]
pub fn resize_overlay(app: AppHandle, dw: i32, dh: i32) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or("Failed to get main window")?;

    let size = window
        .outer_size()
        .map_err(|e| format!("Failed to get size: {e}"))?;

    let new_w = (size.width as i32 + dw).max(280) as u32;
    let new_h = (size.height as i32 + dh).max(200) as u32;

    window
        .set_size(tauri::Size::Physical(tauri::PhysicalSize::new(new_w, new_h)))
        .map_err(|e| format!("Failed to resize: {e}"))?;

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
                // Hide from Alt+Tab / Win+Tab
                let ex_style = GetWindowLongPtrW(hwnd.0, GWL_EXSTYLE);
                SetWindowLongPtrW(
                    hwnd.0,
                    GWL_EXSTYLE,
                    ex_style | WS_EX_TOOLWINDOW as isize | WS_EX_NOACTIVATE as isize,
                );
            }
        }
    }

    Ok(())
}
