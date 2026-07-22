#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod config;
mod hotkeys;
mod overlay;
mod stealth;

use tauri::{Emitter, Listener};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(audio::AudioEngine::new())
        .manage(audio::RecordingStore::new())
        .invoke_handler(tauri::generate_handler![
            overlay::set_click_through,
            overlay::toggle_overlay_visibility,
            overlay::move_overlay,
            overlay::open_settings,
            config::load_config,
            config::save_config,
            audio::get_recording_data,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // Apply content protection
            if let Err(e) = overlay::apply_content_protection(&handle) {
                eprintln!("Warning: Could not apply content protection: {e}");
            }

            // Enable click-through by default
            if let Err(e) = overlay::set_click_through(handle.clone(), true) {
                eprintln!("Warning: Could not enable click-through: {e}");
            }

            // Register global hotkeys
            if let Err(e) = hotkeys::register_all(&handle) {
                eprintln!("Warning: Could not register hotkeys: {e}");
            }

            // Handle emergency exit (Ctrl+Shift+Q)
            let exit_handle = handle.clone();
            app.listen("hotkey:emergency-exit", move |_| {
                hotkeys::unregister_all(&exit_handle);
                std::process::exit(0);
            });

            // Handle toggle visibility (Ctrl+Shift+H)
            let vis_handle = handle.clone();
            app.listen("hotkey:toggle-visibility", move |_| {
                let _ = overlay::toggle_overlay_visibility(vis_handle.clone());
            });

            // Handle move hotkeys
            let h = handle.clone();
            app.listen("hotkey:move-up", move |_| { let _ = overlay::move_overlay(h.clone(), 0, -20); });
            let h = handle.clone();
            app.listen("hotkey:move-down", move |_| { let _ = overlay::move_overlay(h.clone(), 0, 20); });
            let h = handle.clone();
            app.listen("hotkey:move-left", move |_| { let _ = overlay::move_overlay(h.clone(), -20, 0); });
            let h = handle.clone();
            app.listen("hotkey:move-right", move |_| { let _ = overlay::move_overlay(h.clone(), 20, 0); });

            // Handle open settings
            let settings_handle = handle.clone();
            app.listen("hotkey:open-settings", move |_| {
                let _ = overlay::open_settings(settings_handle.clone());
            });

            // Handle toggle recording (Ctrl+Shift+Y)
            {
                use tauri::Manager;
                let rec_handle = handle.clone();
                app.listen("hotkey:toggle-recording", move |_| {
                    let engine = rec_handle.state::<audio::AudioEngine>();
                    let store = rec_handle.state::<audio::RecordingStore>();

                    if engine.is_recording.load(std::sync::atomic::Ordering::SeqCst) {
                        let wav_bytes = engine.stop_recording();
                        let byte_count = wav_bytes.len();
                        *store.data.lock().unwrap() = Some(wav_bytes);
                        let _ = rec_handle.emit("recording:stopped", byte_count);
                    } else {
                        match engine.start_recording() {
                            Ok(()) => {
                                let _ = rec_handle.emit("recording:started", ());
                            }
                            Err(e) => {
                                let _ = rec_handle.emit("recording:error", e);
                            }
                        }
                    }
                });
            }

            // System tray
            {
                use tauri::menu::{Menu, MenuItem};
                use tauri::tray::TrayIconBuilder;

                let quit_item = MenuItem::with_id(app, "quit", "Exit Audio Service", true, None::<&str>)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                let menu = Menu::with_items(app, &[&quit_item])
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

                let _tray = TrayIconBuilder::new()
                    .tooltip("Windows Audio Device Manager")
                    .menu(&menu)
                    .on_menu_event(move |tray_app, event| {
                        if event.id() == "quit" {
                            hotkeys::unregister_all(tray_app);
                            std::process::exit(0);
                        }
                    })
                    .build(app)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            }

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running phantom");
}
