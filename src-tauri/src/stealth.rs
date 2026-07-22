/// Stealth utilities for process disguise.
/// Main stealth is handled at build time (PE metadata) and config (window title, product name).

#[cfg(target_os = "windows")]
use windows_sys::Win32::UI::WindowsAndMessaging::SetWindowTextW;

/// Set window title to something innocuous
#[cfg(target_os = "windows")]
#[allow(dead_code)]
pub fn set_stealth_window_title(hwnd: *mut core::ffi::c_void, title: &str) {
    let wide: Vec<u16> = title.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        SetWindowTextW(hwnd, wide.as_ptr());
    }
}

#[cfg(not(target_os = "windows"))]
pub fn set_stealth_window_title(_hwnd: *mut core::ffi::c_void, _title: &str) {}
