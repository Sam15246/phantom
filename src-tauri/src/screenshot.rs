use std::sync::Mutex;
use tauri::{AppHandle, Manager};

// ---------------------------------------------------------------------------
// Screenshot Queue
// ---------------------------------------------------------------------------

pub struct ScreenshotQueue {
    pub queue: Mutex<Vec<Vec<u8>>>,
}

impl ScreenshotQueue {
    pub fn new() -> Self {
        Self {
            queue: Mutex::new(Vec::new()),
        }
    }
}

// ---------------------------------------------------------------------------
// GDI Screen Capture (Windows)
// ---------------------------------------------------------------------------

#[cfg(target_os = "windows")]
pub fn capture_screen(app: &AppHandle) -> Result<Vec<u8>, String> {
    use image::ImageFormat;
    use std::io::Cursor;
    use windows_sys::Win32::Graphics::Gdi::{
        BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDIBits,
        GetDC, ReleaseDC, SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS,
        SRCCOPY,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetDesktopWindow, GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN,
    };

    // Hide overlay window
    let window = app
        .get_webview_window("main")
        .ok_or("Failed to get main window")?;
    let _ = window.hide();

    // Wait for window to hide
    std::thread::sleep(std::time::Duration::from_millis(100));

    let result = (|| -> Result<Vec<u8>, String> {
        unsafe {
            let width = GetSystemMetrics(SM_CXSCREEN);
            let height = GetSystemMetrics(SM_CYSCREEN);

            if width <= 0 || height <= 0 {
                return Err("Failed to get screen dimensions".into());
            }

            let hwnd = GetDesktopWindow();
            let hdc_screen = GetDC(hwnd);
            if hdc_screen.is_null() {
                return Err("Failed to get screen DC".into());
            }

            let hdc_mem = CreateCompatibleDC(hdc_screen);
            if hdc_mem.is_null() {
                ReleaseDC(hwnd, hdc_screen);
                return Err("Failed to create compatible DC".into());
            }

            let hbitmap = CreateCompatibleBitmap(hdc_screen, width, height);
            if hbitmap.is_null() {
                DeleteDC(hdc_mem);
                ReleaseDC(hwnd, hdc_screen);
                return Err("Failed to create compatible bitmap".into());
            }

            let old_bitmap = SelectObject(hdc_mem, hbitmap);
            let blt_result = BitBlt(hdc_mem, 0, 0, width, height, hdc_screen, 0, 0, SRCCOPY);
            if blt_result == 0 {
                SelectObject(hdc_mem, old_bitmap);
                DeleteObject(hbitmap);
                DeleteDC(hdc_mem);
                ReleaseDC(hwnd, hdc_screen);
                return Err("BitBlt failed".into());
            }

            // Prepare BITMAPINFO
            let mut bmi: BITMAPINFO = std::mem::zeroed();
            bmi.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
            bmi.bmiHeader.biWidth = width;
            bmi.bmiHeader.biHeight = -height; // top-down
            bmi.bmiHeader.biPlanes = 1;
            bmi.bmiHeader.biBitCount = 32;
            bmi.bmiHeader.biCompression = BI_RGB;

            let pixel_count = (width * height) as usize;
            let mut pixels: Vec<u8> = vec![0u8; pixel_count * 4];

            let lines = GetDIBits(
                hdc_mem,
                hbitmap,
                0,
                height as u32,
                pixels.as_mut_ptr() as *mut _,
                &mut bmi,
                DIB_RGB_COLORS,
            );

            // Cleanup GDI objects
            SelectObject(hdc_mem, old_bitmap);
            DeleteObject(hbitmap);
            DeleteDC(hdc_mem);
            ReleaseDC(hwnd, hdc_screen);

            if lines == 0 {
                return Err("GetDIBits failed".into());
            }

            // Convert BGRA to RGBA
            for i in 0..pixel_count {
                let offset = i * 4;
                pixels.swap(offset, offset + 2); // swap B and R
            }

            // Encode as PNG
            let img = image::RgbaImage::from_raw(width as u32, height as u32, pixels)
                .ok_or("Failed to create image from raw pixels")?;

            let mut png_bytes = Cursor::new(Vec::new());
            img.write_to(&mut png_bytes, ImageFormat::Png)
                .map_err(|e| format!("Failed to encode PNG: {e}"))?;

            Ok(png_bytes.into_inner())
        }
    })();

    // Show overlay again regardless of capture result
    let _ = window.show();

    result
}

#[cfg(not(target_os = "windows"))]
pub fn capture_screen(_app: &AppHandle) -> Result<Vec<u8>, String> {
    Err("Screenshot capture is only supported on Windows".into())
}

// ---------------------------------------------------------------------------
// Tauri Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn take_screenshot(
    app: AppHandle,
    queue: tauri::State<'_, ScreenshotQueue>,
) -> Result<usize, String> {
    let png_bytes = capture_screen(&app)?;
    let mut q = queue.queue.lock().map_err(|e| format!("Lock error: {e}"))?;
    q.push(png_bytes);
    Ok(q.len())
}

#[tauri::command]
pub fn get_screenshot_count(queue: tauri::State<'_, ScreenshotQueue>) -> usize {
    queue.queue.lock().map(|q| q.len()).unwrap_or(0)
}

#[tauri::command]
pub fn clear_screenshots(queue: tauri::State<'_, ScreenshotQueue>) -> Result<(), String> {
    let mut q = queue.queue.lock().map_err(|e| format!("Lock error: {e}"))?;
    q.clear();
    Ok(())
}

// ---------------------------------------------------------------------------
// Helper: base64 encoding
// ---------------------------------------------------------------------------

pub fn get_screenshots_base64(queue: &ScreenshotQueue) -> Vec<String> {
    use base64::Engine;
    let q = queue.queue.lock().unwrap_or_else(|e| e.into_inner());
    q.iter()
        .map(|png| base64::engine::general_purpose::STANDARD.encode(png))
        .collect()
}
