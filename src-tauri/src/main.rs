#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod overlay;

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            overlay::set_click_through,
            overlay::toggle_overlay_visibility,
            overlay::move_overlay,
            overlay::open_settings,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // Apply content protection on startup
            if let Err(e) = overlay::apply_content_protection(&handle) {
                eprintln!("Warning: Could not apply content protection: {e}");
            }

            // Start with click-through enabled
            if let Err(e) = overlay::set_click_through(handle.clone(), true) {
                eprintln!("Warning: Could not enable click-through: {e}");
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running phantom");
}
