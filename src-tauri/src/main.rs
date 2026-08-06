#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod audio;
mod config;
mod hotkeys;
mod overlay;
mod screenshot;


use tauri::{Emitter, Listener};

pub struct ConversationHistory {
    pub messages: std::sync::Mutex<Vec<api::ChatMessage>>,
    pub last_mode: std::sync::Mutex<String>,
    pub locked_mode: std::sync::Mutex<Option<String>>,
}

impl ConversationHistory {
    pub fn new() -> Self {
        Self {
            messages: std::sync::Mutex::new(Vec::new()),
            last_mode: std::sync::Mutex::new("general".to_string()),
            locked_mode: std::sync::Mutex::new(None),
        }
    }
}

pub struct ClickThroughState {
    pub enabled: std::sync::atomic::AtomicBool,
}

impl ClickThroughState {
    pub fn new() -> Self {
        Self {
            enabled: std::sync::atomic::AtomicBool::new(true),
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

    // Step 2: Extract question + detect mode (skip extraction if mode is locked)
    let history_state = app.state::<ConversationHistory>();
    let locked = history_state.locked_mode.lock().unwrap_or_else(|e| e.into_inner()).clone();

    let (question, effective_mode, context) = if let Some(ref locked_mode) = locked {
        // Mode is locked — skip extraction entirely, use transcript as-is
        let _ = app.emit("pipeline:status", &format!("Mode locked: {}",
            locked_mode.to_uppercase()));
        *history_state.last_mode.lock().unwrap_or_else(|e| e.into_inner()) = locked_mode.clone();
        let _ = app.emit("pipeline:extraction", &api::ExtractionResult {
            question: transcript.clone(),
            mode: locked_mode.clone(),
            context: String::new(),
        });
        (transcript, locked_mode.clone(), String::new())
    } else {
        // Auto-detect mode via Groq extraction
        let _ = app.emit("pipeline:status", "Analyzing question...");
        let extraction = if !cfg.groq_api_key.is_empty() {
            api::extract_question(&cfg.groq_api_key, &transcript).await
                .unwrap_or_else(|_| api::fallback_extraction(&transcript))
        } else {
            api::fallback_extraction(&transcript)
        };
        let _ = app.emit("pipeline:extraction", &extraction);

        // Skip mode — small talk, greetings, audio checks
        if extraction.mode == "skip" {
            let _ = app.emit("answer:done", "No interview question detected — just small talk or audio check.");
            return Ok(());
        }

        let mode = extraction.mode.clone();
        *history_state.last_mode.lock().unwrap_or_else(|e| e.into_inner()) = mode.clone();
        (extraction.question, mode, extraction.context)
    };

    // Get current history for context (cap at last 40 messages to limit memory)
    let hist: Vec<api::ChatMessage> = {
        let msgs = history_state.messages.lock().unwrap_or_else(|e| e.into_inner());
        if msgs.len() > 40 {
            msgs[msgs.len() - 40..].to_vec()
        } else {
            msgs.clone()
        }
    };

    // Step 3: Generate answer (streaming)
    let _ = app.emit("pipeline:status", "Generating answer...");
    let answer = api::generate_answer_streaming(
        &app,
        &cfg.openai_api_key,
        &question,
        &effective_mode,
        &context,
        &hist,
        &cfg.resume_text,
        &cfg.job_description,
    ).await?;

    // Store in conversation history
    {
        let mut msgs = history_state.messages.lock().unwrap_or_else(|e| e.into_inner());
        msgs.push(api::ChatMessage {
            role: "user".to_string(),
            content: question,
        });
        msgs.push(api::ChatMessage {
            role: "assistant".to_string(),
            content: answer,
        });
    }

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
    let mode = history_state.last_mode.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let hist: Vec<api::ChatMessage> = {
        let msgs = history_state.messages.lock().unwrap_or_else(|e| e.into_inner());
        if msgs.len() > 40 {
            msgs[msgs.len() - 40..].to_vec()
        } else {
            msgs.clone()
        }
    };

    let answer = api::generate_answer_streaming(
        &app,
        &cfg.openai_api_key,
        &question,
        &mode,
        "",
        &hist,
        &cfg.resume_text,
        &cfg.job_description,
    ).await;

    // Only add to history if generation succeeded
    match answer {
        Ok(answer_text) => {
            let mut msgs = history_state.messages.lock().unwrap_or_else(|e| e.into_inner());
            msgs.push(api::ChatMessage {
                role: "user".to_string(),
                content: question,
            });
            msgs.push(api::ChatMessage {
                role: "assistant".to_string(),
                content: answer_text,
            });
            Ok(())
        }
        Err(e) => Err(e),
    }
}

#[tauri::command]
async fn speak_answer(_app: tauri::AppHandle, text: String) -> Result<String, String> {
    let cfg = config::load_config_internal()?;
    if !cfg.tts_enabled {
        return Err("TTS is disabled".into());
    }
    if cfg.openai_api_key.is_empty() {
        return Err("OpenAI API key not set".into());
    }
    api::text_to_speech(&cfg.openai_api_key, &text).await
}

#[tauri::command]
async fn export_session(app: tauri::AppHandle) -> Result<String, String> {
    use tauri::Manager;
    use std::fs;

    let history = app.state::<ConversationHistory>();
    let messages = history.messages.lock().unwrap_or_else(|e| e.into_inner()).clone();

    if messages.is_empty() {
        return Err("No conversation history to export".into());
    }

    // Create sessions directory in config dir
    let base = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
    let sessions_dir = base.join("AudioDeviceManager").join("sessions");
    fs::create_dir_all(&sessions_dir).map_err(|e| format!("Failed to create sessions dir: {e}"))?;

    // Generate innocuous filename
    let now = chrono::Local::now();
    let filename = format!("devlog_{}.dat", now.format("%Y%m%d_%H%M%S"));
    let filepath = sessions_dir.join(&filename);

    // Build session content
    let mut md = String::new();
    md.push_str(&format!("# Session — {}\n\n", now.format("%B %d, %Y %H:%M")));
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

    // Encrypt session file using same mechanism as config
    let encrypted = config::encrypt_data(md.as_bytes())?;
    fs::write(&filepath, &encrypted).map_err(|e| format!("Failed to write session: {e}"))?;

    let path_str = filepath.to_string_lossy().to_string();
    Ok(path_str)
}

#[tauri::command]
async fn generate_summary(app: tauri::AppHandle) -> Result<(), String> {
    use tauri::Manager;

    let history = app.state::<ConversationHistory>();
    let messages = history.messages.lock().unwrap_or_else(|e| e.into_inner()).clone();

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
        .manage(ClickThroughState::new())
        .manage(screenshot::ScreenshotQueue::new())
        .invoke_handler(tauri::generate_handler![
            overlay::set_click_through,
            overlay::toggle_overlay_visibility,
            overlay::move_overlay,
            overlay::open_settings,
            overlay::snap_to_webcam,
            config::load_config,
            config::save_config,
            config::parse_pdf,
            audio::get_recording_data,
            ask_followup,
            speak_answer,
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
                let _ = handle.emit("pipeline:error", format!("Content protection failed: {e}"));
            }

            // Hide main window from Alt+Tab
            overlay::hide_from_alt_tab(&handle);

            // Enable click-through by default
            if let Err(e) = overlay::set_click_through(handle.clone(), true) {
                let _ = handle.emit("pipeline:error", format!("Click-through failed: {e}"));
            }

            // Register global hotkeys
            if let Err(e) = hotkeys::register_all(&handle) {
                let _ = handle.emit("pipeline:error", format!("Hotkey registration failed: {e}"));
            }

            // Handle emergency exit (Ctrl+Shift+Q)
            let exit_handle = handle.clone();
            app.listen("hotkey:emergency-exit", move |_| {
                use tauri::Manager;
                // Auto-export encrypted session on exit if there's history
                let history = exit_handle.state::<ConversationHistory>();
                let messages = history.messages.lock().unwrap_or_else(|e| e.into_inner()).clone();
                if !messages.is_empty() {
                    let base = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
                    let sessions_dir = base.join("AudioDeviceManager").join("sessions");
                    let _ = std::fs::create_dir_all(&sessions_dir);
                    let now = chrono::Local::now();
                    let filename = format!("devlog_{}.dat", now.format("%Y%m%d_%H%M%S"));
                    let filepath = sessions_dir.join(&filename);

                    let mut md = format!("# Session — {}\n\n---\n\n", now.format("%B %d, %Y %H:%M"));
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
                    // Encrypt session data before writing
                    if let Ok(encrypted) = config::encrypt_data(md.as_bytes()) {
                        let _ = std::fs::write(&filepath, &encrypted);
                    }
                }

                hotkeys::unregister_all(&exit_handle);
                std::process::exit(0);
            });

            // Handle toggle click-through (Ctrl+Shift+F4)
            {
                use tauri::Manager;
                let ct_handle = handle.clone();
                app.listen("hotkey:toggle-click-through", move |_| {
                    let state = ct_handle.state::<ClickThroughState>();
                    let was_enabled = state.enabled.load(std::sync::atomic::Ordering::SeqCst);
                    let new_val = !was_enabled;
                    state.enabled.store(new_val, std::sync::atomic::Ordering::SeqCst);
                    let _ = overlay::set_click_through(ct_handle.clone(), new_val);
                    let _ = ct_handle.emit("hotkey:toggle-click-through-ui", new_val);
                });
            }

            // Handle toggle night mode (Ctrl+Shift+Z)
            let nm_handle = handle.clone();
            app.listen("hotkey:toggle-night-mode", move |_| {
                let _ = nm_handle.emit("hotkey:toggle-night-mode-ui", ());
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

            // Handle resize hotkeys
            {
                use tauri::Manager;
                let h = handle.clone();
                app.listen("hotkey:resize-grow", move |_| {
                    if let Some(window) = h.get_webview_window("main") {
                        if let Ok(size) = window.outer_size() {
                            let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize::new(
                                size.width + 40,
                                size.height + 30,
                            )));
                        }
                    }
                });
                let h = handle.clone();
                app.listen("hotkey:resize-shrink", move |_| {
                    if let Some(window) = h.get_webview_window("main") {
                        if let Ok(size) = window.outer_size() {
                            let new_w = if size.width > 280 { size.width - 40 } else { size.width };
                            let new_h = if size.height > 200 { size.height - 30 } else { size.height };
                            let _ = window.set_size(tauri::Size::Physical(tauri::PhysicalSize::new(new_w, new_h)));
                        }
                    }
                });
            }

            // Handle open settings
            let settings_handle = handle.clone();
            app.listen("hotkey:open-settings", move |_| {
                let _ = overlay::open_settings(settings_handle.clone());
            });

            // Handle snap to webcam
            let snap_handle = handle.clone();
            app.listen("hotkey:snap-to-webcam", move |_| {
                let _ = overlay::snap_to_webcam(snap_handle.clone());
            });

            // Handle mode cycle (Ctrl+Shift+4) — cycle through lock modes
            {
                use tauri::Manager;
                let cycle_handle = handle.clone();
                app.listen("hotkey:mode-cycle", move |_| {
                    let history = cycle_handle.state::<ConversationHistory>();
                    let mut locked = history.locked_mode.lock().unwrap_or_else(|e| e.into_inner());

                    // Cycle: None → dsa → oa → system-design → lld → ai-interview → None
                    let next = match locked.as_deref() {
                        None              => Some("dsa".to_string()),
                        Some("dsa")       => Some("oa".to_string()),
                        Some("oa")        => Some("system-design".to_string()),
                        Some("system-design") => Some("lld".to_string()),
                        Some("lld")       => Some("ai-interview".to_string()),
                        Some("ai-interview") => None,
                        Some(_)           => None,
                    };

                    if let Some(ref mode) = next {
                        *history.last_mode.lock().unwrap_or_else(|e| e.into_inner()) = mode.clone();
                    }

                    let mode_name = next.as_deref().unwrap_or("auto").to_string();
                    *locked = next;

                    let _ = cycle_handle.emit("mode:locked", &mode_name);
                });
            }

            // Handle mode unlock (Ctrl+Shift+5) — clears mode lock, returns to auto-detect
            {
                use tauri::Manager;
                let unlock_handle = handle.clone();
                app.listen("hotkey:mode-unlock", move |_| {
                    let history = unlock_handle.state::<ConversationHistory>();
                    *history.locked_mode.lock().unwrap_or_else(|e| e.into_inner()) = None;
                    *history.last_mode.lock().unwrap_or_else(|e| e.into_inner()) = "general".to_string();
                    let _ = unlock_handle.emit("mode:locked", "auto");
                });
            }

            // Handle toggle recording (Ctrl+Shift+F6)
            {
                use tauri::Manager;
                let rec_handle = handle.clone();
                app.listen("hotkey:toggle-recording", move |_| {
                    let engine = rec_handle.state::<audio::AudioEngine>();
                    let store = rec_handle.state::<audio::RecordingStore>();

                    if engine.is_recording.load(std::sync::atomic::Ordering::SeqCst) {
                        let wav_bytes = engine.stop_recording();
                        let byte_count = wav_bytes.len();
                        *store.data.lock().unwrap_or_else(|e| e.into_inner()) = Some(wav_bytes.clone());
                        let _ = rec_handle.emit("recording:stopped", byte_count);

                        // Spawn the AI pipeline
                        let pipeline_handle = rec_handle.clone();
                        let wav = wav_bytes;
                        tauri::async_runtime::spawn(async move {
                            if let Err(e) = run_pipeline(pipeline_handle.clone(), wav).await {
                                let _ = pipeline_handle.emit("pipeline:error", &e);
                            }
                        });
                    } else {
                        let audio_source = config::load_config_internal()
                            .map(|c| c.audio_source)
                            .unwrap_or_else(|_| "both".to_string());
                        match engine.start_recording(&audio_source) {
                            Ok(warning) => {
                                let _ = rec_handle.emit("recording:started", ());
                                if let Some(msg) = warning {
                                    let _ = rec_handle.emit("recording:warning", &msg);
                                }
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

                // Clone messages before clearing for background summary
                let messages: Vec<api::ChatMessage> = history.messages.lock().unwrap_or_else(|e| e.into_inner()).clone();

                // Clear everything immediately
                history.messages.lock().unwrap_or_else(|e| e.into_inner()).clear();
                *history.last_mode.lock().unwrap_or_else(|e| e.into_inner()) = "general".to_string();
                *history.locked_mode.lock().unwrap_or_else(|e| e.into_inner()) = None;
                let queue = clear_handle.state::<screenshot::ScreenshotQueue>();
                queue.queue.lock().unwrap_or_else(|e| e.into_inner()).clear();
                let _ = clear_handle.emit("mode:locked", "auto");
                let _ = clear_handle.emit("session:cleared", ());

                // Background summary generation (non-blocking)
                if !messages.is_empty() {
                    let summary_handle = clear_handle.clone();
                    tauri::async_runtime::spawn(async move {
                        if let Ok(cfg) = config::load_config_internal() {
                            if !cfg.openai_api_key.is_empty() {
                                let mut conversation = String::new();
                                for msg in &messages {
                                    conversation.push_str(&format!("[{}]: {}\n\n", msg.role, msg.content));
                                }

                                let summary_prompt = format!(
                                    "Generate a brief post-interview summary for this conversation. Include: topics covered, question types, key areas discussed. Keep it under 200 words.\n\n{}",
                                    conversation
                                );

                                // Use non-streaming request for background summary
                                if let Ok(summary) = api::generate_answer_silent(
                                    &cfg.openai_api_key,
                                    &summary_prompt,
                                ).await {
                                    // Save encrypted summary
                                    let base = dirs::config_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
                                    let sessions_dir = base.join("AudioDeviceManager").join("sessions");
                                    let _ = std::fs::create_dir_all(&sessions_dir);
                                    let now = chrono::Local::now();
                                    let filename = format!("summary_{}.dat", now.format("%Y%m%d_%H%M%S"));
                                    let filepath = sessions_dir.join(&filename);

                                    let mut content = format!("# Session Summary — {}\n\n", now.format("%B %d, %Y %H:%M"));
                                    content.push_str(&summary);
                                    content.push_str("\n\n---\n\n# Full Conversation\n\n");
                                    for msg in &messages {
                                        match msg.role.as_str() {
                                            "user" => content.push_str(&format!("## Q: {}\n\n", msg.content)),
                                            "assistant" => content.push_str(&format!("### A:\n{}\n\n---\n\n", msg.content)),
                                            _ => {}
                                        }
                                    }

                                    if let Ok(encrypted) = config::encrypt_data(content.as_bytes()) {
                                        let _ = std::fs::write(&filepath, &encrypted);
                                    }
                                }
                            }
                        }
                        drop(summary_handle); // ensure handle lives until task completes
                    });
                }
            });

            // Handle clear screenshots only (Ctrl+Shift+F5)
            let ss_clear_handle = handle.clone();
            app.listen("hotkey:clear-screenshots", move |_| {
                use tauri::Manager;
                let queue = ss_clear_handle.state::<screenshot::ScreenshotQueue>();
                queue.queue.lock().unwrap_or_else(|e| e.into_inner()).clear();
                let _ = ss_clear_handle.emit("screenshot:cleared", ());
            });

            // Handle screenshot capture (Ctrl+Shift+F7)
            {
                use tauri::Manager;
                let ss_handle = handle.clone();
                app.listen("hotkey:screenshot", move |_| {
                    let queue = ss_handle.state::<screenshot::ScreenshotQueue>();
                    match screenshot::capture_screen(&ss_handle) {
                        Ok(png_bytes) => {
                            let mut q = queue.queue.lock().unwrap_or_else(|e| e.into_inner());
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

            // Handle analyze screenshots (Ctrl+Shift+F)
            {
                use tauri::Manager;
                let analyze_handle = handle.clone();
                app.listen("hotkey:analyze", move |_| {
                    let ah = analyze_handle.clone();
                    let queue = ah.state::<screenshot::ScreenshotQueue>();
                    let screenshots_b64 = screenshot::get_screenshots_base64(queue.inner());

                    if screenshots_b64.is_empty() {
                        let _ = ah.emit("pipeline:error", "No screenshots captured. Press Ctrl+Shift+F7 to take screenshots first.");
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

                        // Determine mode: locked_mode takes priority, then last_mode
                        let history = ah.state::<ConversationHistory>();
                        let current_mode = {
                            let locked = history.locked_mode.lock().unwrap_or_else(|e| e.into_inner()).clone();
                            if let Some(locked_mode) = locked {
                                locked_mode
                            } else {
                                history.last_mode.lock().unwrap_or_else(|e| e.into_inner()).clone()
                            }
                        };

                        match api::analyze_screenshots(
                            &ah,
                            &cfg.openai_api_key,
                            &screenshots_b64,
                            &current_mode,
                        )
                        .await
                        {
                            Ok(answer) => {
                                let history = ah.state::<ConversationHistory>();
                                // Only set last_mode to OA if not locked and not in a live interview context
                                let is_locked = history.locked_mode.lock().unwrap_or_else(|e| e.into_inner()).is_some();
                                if !is_locked {
                                    let is_live = matches!(current_mode.as_str(), "dsa" | "ai-interview" | "system-design" | "lld" | "behavioral" | "ai-ml" | "backend" | "java" | "python" | "dbms" | "cloud");
                                    if !is_live {
                                        *history.last_mode.lock().unwrap_or_else(|e| e.into_inner()) = "OA".to_string();
                                    }
                                }
                                let mut msgs = history.messages.lock().unwrap_or_else(|e| e.into_inner());
                                msgs.push(api::ChatMessage {
                                    role: "user".to_string(),
                                    content: format!(
                                        "[Screenshot analysis of {} image(s)]",
                                        screenshots_b64.len()
                                    ),
                                });
                                msgs.push(api::ChatMessage {
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
        .expect("error while running application");
}
