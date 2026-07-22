#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod audio;
mod config;
mod hotkeys;
mod overlay;
mod screenshot;
mod stealth;

use tauri::{Emitter, Listener};

pub struct ConversationHistory {
    pub messages: std::sync::Mutex<Vec<api::ChatMessage>>,
    pub last_mode: std::sync::Mutex<String>,
}

impl ConversationHistory {
    pub fn new() -> Self {
        Self {
            messages: std::sync::Mutex::new(Vec::new()),
            last_mode: std::sync::Mutex::new("general".to_string()),
        }
    }
}

async fn run_pipeline(app: tauri::AppHandle, wav_bytes: Vec<u8>) -> Result<(), String> {
    use tauri::Manager;

    let _ = app.emit("pipeline:started", ());

    let cfg = config::load_config_internal()?;

    if cfg.openai_api_key.is_empty() {
        let _ = app.emit("pipeline:error", "OpenAI API key not set. Press Ctrl+Shift+, to open settings.");
        return Err("No API key".into());
    }

    // Step 1: Transcribe
    let _ = app.emit("pipeline:status", "Transcribing...");
    let transcript = api::transcribe_audio(&cfg.openai_api_key, wav_bytes).await?;
    let _ = app.emit("pipeline:transcript", &transcript);

    if transcript.trim().is_empty() {
        let _ = app.emit("pipeline:error", "No speech detected");
        return Err("Empty transcript".into());
    }

    // Step 2: Extract question + detect mode
    let _ = app.emit("pipeline:status", "Analyzing question...");
    let extraction = if !cfg.groq_api_key.is_empty() {
        api::extract_question(&cfg.groq_api_key, &transcript).await
            .unwrap_or_else(|_| api::fallback_extraction(&transcript))
    } else {
        api::fallback_extraction(&transcript)
    };
    let _ = app.emit("pipeline:extraction", &extraction);

    // Update conversation history
    let history_state = app.state::<ConversationHistory>();
    *history_state.last_mode.lock().unwrap() = extraction.mode.clone();

    // Get current history for context
    let hist: Vec<api::ChatMessage> = history_state.messages.lock().unwrap().clone();

    // Step 3: Generate answer (streaming)
    let _ = app.emit("pipeline:status", "Generating answer...");
    let answer = api::generate_answer_streaming(
        &app,
        &cfg.openai_api_key,
        &extraction.question,
        &extraction.mode,
        &extraction.context,
        &hist,
        &cfg.resume_text,
        &cfg.job_description,
    ).await?;

    // Store in conversation history
    history_state.messages.lock().unwrap().push(api::ChatMessage {
        role: "user".to_string(),
        content: extraction.question,
    });
    history_state.messages.lock().unwrap().push(api::ChatMessage {
        role: "assistant".to_string(),
        content: answer,
    });

    Ok(())
}

#[tauri::command]
async fn ask_followup(
    app: tauri::AppHandle,
    question: String,
) -> Result<(), String> {
    use tauri::Manager;

    let cfg = config::load_config_internal()?;
    if cfg.openai_api_key.is_empty() {
        return Err("OpenAI API key not set".into());
    }

    let history_state = app.state::<ConversationHistory>();

    // Add user question
    history_state.messages.lock().unwrap().push(api::ChatMessage {
        role: "user".to_string(),
        content: question.clone(),
    });

    let mode = history_state.last_mode.lock().unwrap().clone();
    let hist: Vec<api::ChatMessage> = history_state.messages.lock().unwrap().clone();

    let answer = api::generate_answer_streaming(
        &app,
        &cfg.openai_api_key,
        &question,
        &mode,
        "",
        &hist,
        &cfg.resume_text,
        &cfg.job_description,
    ).await?;

    history_state.messages.lock().unwrap().push(api::ChatMessage {
        role: "assistant".to_string(),
        content: answer,
    });

    Ok(())
}

#[tauri::command]
async fn export_session(app: tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    use std::fs;

    let history = app.state::<ConversationHistory>();
    let messages = history.messages.lock().unwrap().clone();

    if messages.is_empty() {
        return Err("No conversation history to export".into());
    }

    // Create sessions directory in config dir
    let base = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let sessions_dir = base.join("AudioDeviceManager").join("sessions");
    fs::create_dir_all(&sessions_dir).map_err(|e| format!("Failed to create sessions dir: {e}"))?;

    // Generate filename with timestamp
    let now = chrono::Local::now();
    let filename = format!("session_{}.md", now.format("%Y-%m-%d_%H-%M-%S"));
    let filepath = sessions_dir.join(&filename);

    // Build markdown content
    let mut md = String::new();
    md.push_str(&format!("# Interview Session — {}\n\n", now.format("%B %d, %Y %H:%M")));
    md.push_str("---\n\n");

    let mut q_num = 0;
    for msg in &messages {
        match msg.role.as_str() {
            "user" => {
                q_num += 1;
                md.push_str(&format!("## Q{}: {}\n\n", q_num, msg.content));
            }
            "assistant" => {
                md.push_str(&format!("### Answer\n\n{}\n\n---\n\n", msg.content));
            }
            _ => {}
        }
    }

    fs::write(&filepath, &md).map_err(|e| format!("Failed to write session: {e}"))?;

    let path_str = filepath.to_string_lossy().to_string();
    Ok(path_str)
}

#[tauri::command]
async fn generate_summary(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;

    let history = app.state::<ConversationHistory>();
    let messages = history.messages.lock().unwrap().clone();

    if messages.is_empty() {
        return Err("No conversation history to summarize".into());
    }

    let cfg = config::load_config_internal()?;
    if cfg.openai_api_key.is_empty() {
        return Err("OpenAI API key not set".into());
    }

    // Build the full conversation as text
    let mut conversation = String::new();
    for msg in &messages {
        conversation.push_str(&format!("[{}]: {}\n\n", msg.role, msg.content));
    }

    let _ = app.emit("pipeline:started", ());
    let _ = app.emit("pipeline:status", "Generating interview summary...");
    let _ = app.emit("answer:mode", "SUMMARY");

    // Use streaming answer generation (reuse existing infrastructure)
    api::generate_answer_streaming(
        &app,
        &cfg.openai_api_key,
        &format!("Generate a post-interview summary for this conversation:\n\n{conversation}"),
        "general",
        "",
        &[],
        &cfg.resume_text,
        &cfg.job_description,
    ).await?;

    Ok(())
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .manage(audio::AudioEngine::new())
        .manage(audio::RecordingStore::new())
        .manage(ConversationHistory::new())
        .manage(screenshot::ScreenshotQueue::new())
        .invoke_handler(tauri::generate_handler![
            overlay::set_click_through,
            overlay::toggle_overlay_visibility,
            overlay::move_overlay,
            overlay::open_settings,
            config::load_config,
            config::save_config,
            audio::get_recording_data,
            ask_followup,
            export_session,
            generate_summary,
            screenshot::take_screenshot,
            screenshot::get_screenshot_count,
            screenshot::clear_screenshots,
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
                use tauri::Manager;
                // Auto-export session on exit if there's history
                let history = exit_handle.state::<ConversationHistory>();
                let messages = history.messages.lock().unwrap().clone();
                if !messages.is_empty() {
                    let base = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
                    let sessions_dir = base.join("AudioDeviceManager").join("sessions");
                    let _ = std::fs::create_dir_all(&sessions_dir);
                    let now = chrono::Local::now();
                    let filename = format!("session_{}.md", now.format("%Y-%m-%d_%H-%M-%S"));
                    let filepath = sessions_dir.join(&filename);

                    let mut md = format!("# Interview Session — {}\n\n---\n\n", now.format("%B %d, %Y %H:%M"));
                    let mut q_num = 0;
                    for msg in &messages {
                        match msg.role.as_str() {
                            "user" => {
                                q_num += 1;
                                md.push_str(&format!("## Q{}: {}\n\n", q_num, msg.content));
                            }
                            "assistant" => {
                                md.push_str(&format!("### Answer\n\n{}\n\n---\n\n", msg.content));
                            }
                            _ => {}
                        }
                    }
                    let _ = std::fs::write(&filepath, &md);
                }

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
                        *store.data.lock().unwrap() = Some(wav_bytes.clone());
                        let _ = rec_handle.emit("recording:stopped", byte_count);

                        // Spawn the AI pipeline
                        let pipeline_handle = rec_handle.clone();
                        let wav = wav_bytes;
                        tauri::async_runtime::spawn(async move {
                            if let Err(e) = run_pipeline(pipeline_handle, wav).await {
                                eprintln!("Pipeline error: {e}");
                            }
                        });
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

            // Handle clear session
            let clear_handle = handle.clone();
            app.listen("hotkey:clear-session", move |_| {
                use tauri::Manager;
                let history = clear_handle.state::<ConversationHistory>();
                history.messages.lock().unwrap().clear();
                *history.last_mode.lock().unwrap() = "general".to_string();
                let _ = clear_handle.emit("session:cleared", ());
            });

            // Handle screenshot capture (Ctrl+Shift+S)
            {
                use tauri::Manager;
                let ss_handle = handle.clone();
                app.listen("hotkey:screenshot", move |_| {
                    let queue = ss_handle.state::<screenshot::ScreenshotQueue>();
                    match screenshot::capture_screen(&ss_handle) {
                        Ok(png_bytes) => {
                            let mut q = queue.queue.lock().unwrap();
                            q.push(png_bytes);
                            let count = q.len();
                            let _ = ss_handle.emit("screenshot:taken", count);
                        }
                        Err(e) => {
                            let _ = ss_handle.emit("screenshot:error", &e);
                        }
                    }
                });
            }

            // Handle analyze screenshots (Ctrl+Shift+A)
            {
                use tauri::Manager;
                let analyze_handle = handle.clone();
                app.listen("hotkey:analyze", move |_| {
                    let ah = analyze_handle.clone();
                    let queue = ah.state::<screenshot::ScreenshotQueue>();
                    let screenshots_b64 = screenshot::get_screenshots_base64(queue.inner());

                    if screenshots_b64.is_empty() {
                        let _ = ah.emit("pipeline:error", "No screenshots captured. Press Ctrl+Shift+S to take screenshots first.");
                        return;
                    }

                    tauri::async_runtime::spawn(async move {
                        let _ = ah.emit("pipeline:started", ());
                        let _ = ah.emit("pipeline:status", "Analyzing screenshots...");

                        let cfg = match config::load_config_internal() {
                            Ok(c) => c,
                            Err(e) => {
                                let _ = ah.emit("pipeline:error", &format!("Config error: {e}"));
                                return;
                            }
                        };

                        if cfg.openai_api_key.is_empty() {
                            let _ = ah.emit("pipeline:error", "OpenAI API key not set. Press Ctrl+Shift+, to open settings.");
                            return;
                        }

                        let queue = ah.state::<screenshot::ScreenshotQueue>();
                        let screenshots_b64 = screenshot::get_screenshots_base64(queue.inner());

                        match api::analyze_screenshots(
                            &ah,
                            &cfg.openai_api_key,
                            &screenshots_b64,
                            &cfg.resume_text,
                            &cfg.job_description,
                        )
                        .await
                        {
                            Ok(answer) => {
                                let history = ah.state::<ConversationHistory>();
                                *history.last_mode.lock().unwrap() = "OA".to_string();
                                history.messages.lock().unwrap().push(api::ChatMessage {
                                    role: "user".to_string(),
                                    content: format!(
                                        "[Screenshot analysis of {} image(s)]",
                                        screenshots_b64.len()
                                    ),
                                });
                                history.messages.lock().unwrap().push(api::ChatMessage {
                                    role: "assistant".to_string(),
                                    content: answer,
                                });
                            }
                            Err(e) => {
                                let _ = ah.emit("pipeline:error", &format!("Analysis error: {e}"));
                            }
                        }
                    });
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
