#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod api;
mod audio;
mod config;
mod hotkeys;
mod overlay;
mod proctor_detect;
mod screenshot;
mod experience;

use tauri::{Emitter, Listener};

pub struct ConversationHistory {
    pub messages: std::sync::Mutex<Vec<api::ChatMessage>>,
    pub last_mode: std::sync::Mutex<String>,
    pub locked_mode: std::sync::Mutex<Option<String>>,
    pub pipeline_running: std::sync::atomic::AtomicBool,
}

impl ConversationHistory {
    pub fn new() -> Self {
        Self {
            messages: std::sync::Mutex::new(Vec::new()),
            last_mode: std::sync::Mutex::new("general".to_string()),
            locked_mode: std::sync::Mutex::new(None),
            pipeline_running: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

/// RAII guard for the pipeline_running flag. Resets the flag to false on Drop,
/// ensuring the lock is always released regardless of how the scope exits
/// (normal return, early return, `?` propagation, or panic).
struct PipelineGuard {
    app: tauri::AppHandle,
}

impl PipelineGuard {
    /// Try to acquire the pipeline lock. Returns `Some(guard)` on success,
    /// `None` if another pipeline is already running.
    fn try_acquire(app: &tauri::AppHandle) -> Option<Self> {
        use tauri::Manager;
        let history = app.state::<ConversationHistory>();
        history
            .pipeline_running
            .compare_exchange(
                false,
                true,
                std::sync::atomic::Ordering::SeqCst,
                std::sync::atomic::Ordering::SeqCst,
            )
            .ok()
            .map(|_| PipelineGuard { app: app.clone() })
    }
}

impl Drop for PipelineGuard {
    fn drop(&mut self) {
        use tauri::Manager;
        let history = self.app.state::<ConversationHistory>();
        history
            .pipeline_running
            .store(false, std::sync::atomic::Ordering::SeqCst);
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

pub struct NightModeState {
    pub enabled: std::sync::atomic::AtomicBool,
}

impl NightModeState {
    pub fn new() -> Self {
        Self {
            enabled: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

/// Stores the latest proctoring scan result for adaptive behavior.
pub struct ProctorState {
    pub report: std::sync::Mutex<Option<proctor_detect::ProctorReport>>,
}

impl ProctorState {
    pub fn new() -> Self {
        Self {
            report: std::sync::Mutex::new(None),
        }
    }
}

pub struct TrayMenuState {
    pub record_item: tauri::menu::MenuItem<tauri::Wry>,
    pub night_mode_item: tauri::menu::MenuItem<tauri::Wry>,
    pub click_through_item: tauri::menu::MenuItem<tauri::Wry>,
    pub mode_submenu: tauri::menu::Submenu<tauri::Wry>,
}

async fn run_pipeline(app: tauri::AppHandle, wav_bytes: Vec<u8>) -> Result<(), String> {
    use tauri::Manager;

    let _ = app.emit("pipeline:started", ());

    let cfg = app.state::<config::ConfigCache>().get()?;

    if cfg.openai_api_key.is_empty() {
        let _ = app.emit("pipeline:error", "OpenAI API key not set. Press Ctrl+Shift+, to open settings.");
        return Err("No API key".into());
    }

    let http = app.state::<api::SharedHttpClient>();

    // Step 1: Transcribe
    let _ = app.emit("pipeline:status", "Transcribing...");
    let transcript = api::transcribe_audio(&http.client, &cfg.openai_api_key, wav_bytes, cfg.openai_url()).await?;
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
            api::extract_question(&http.client, &cfg.groq_api_key, &transcript, cfg.groq_url()).await
                .unwrap_or_else(|e| {
                    eprintln!("[phantom] Groq extraction failed, using keyword fallback: {e}");
                    api::fallback_extraction(&transcript)
                })
        } else {
            eprintln!("[phantom] No Groq API key — using keyword fallback for mode detection");
            api::fallback_extraction(&transcript)
        };
        let _ = app.emit("pipeline:extraction", &extraction);

        // Skip mode — small talk, greetings, audio checks
        if extraction.mode == "skip" {
            let _ = app.emit("answer:done", &format!(
                "Skipped (detected as small talk).\n\n**Transcript:** \"{}\"\n\n**Extracted:** \"{}\"\n\nIf this was a real question, use Ctrl+Shift+4 to lock a mode.",
                transcript, extraction.question
            ));
            return Ok(());
        }

        let mode = extraction.mode.clone();
        *history_state.last_mode.lock().unwrap_or_else(|e| e.into_inner()) = mode.clone();
        (extraction.question, mode, extraction.context)
    };

    // Get recent history for context (cap at last 20 messages — ~10 Q&A pairs)
    // Keeping this lean reduces input tokens and speeds up time-to-first-token
    let hist: Vec<api::ChatMessage> = {
        let msgs = history_state.messages.lock().unwrap_or_else(|e| e.into_inner());
        if msgs.len() > 20 {
            msgs[msgs.len() - 20..].to_vec()
        } else {
            msgs.clone()
        }
    };

    // Step 3: Generate answer (streaming)
    let _ = app.emit("pipeline:status", "Generating answer...");
    let answer = api::generate_answer_streaming(
        &app,
        &http.client,
        &cfg.openai_api_key,
        &question,
        &effective_mode,
        &context,
        &hist,
        &cfg.resume_text,
        &cfg.job_description,
        cfg.openai_url(),
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

    let cfg = app.state::<config::ConfigCache>().get()?;
    if cfg.openai_api_key.is_empty() {
        return Err("OpenAI API key not set".into());
    }

    let _ = app.emit("pipeline:started", ());
    let _ = app.emit("pipeline:status", "Generating answer...");

    let http = app.state::<api::SharedHttpClient>();
    let history_state = app.state::<ConversationHistory>();
    // Use locked_mode if set, otherwise fall back to last_mode
    let mode = {
        let locked = history_state.locked_mode.lock().unwrap_or_else(|e| e.into_inner()).clone();
        if let Some(locked_mode) = locked {
            locked_mode
        } else {
            history_state.last_mode.lock().unwrap_or_else(|e| e.into_inner()).clone()
        }
    };
    let hist: Vec<api::ChatMessage> = {
        let msgs = history_state.messages.lock().unwrap_or_else(|e| e.into_inner());
        if msgs.len() > 20 {
            msgs[msgs.len() - 20..].to_vec()
        } else {
            msgs.clone()
        }
    };

    let answer = api::generate_answer_streaming(
        &app,
        &http.client,
        &cfg.openai_api_key,
        &question,
        &mode,
        "",
        &hist,
        &cfg.resume_text,
        &cfg.job_description,
        cfg.openai_url(),
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
async fn speak_answer(app: tauri::AppHandle, text: String) -> Result<String, String> {
    use tauri::Manager;
    let cfg = app.state::<config::ConfigCache>().get()?;
    if !cfg.tts_enabled {
        return Err("TTS is disabled".into());
    }
    if cfg.openai_api_key.is_empty() {
        return Err("OpenAI API key not set".into());
    }
    let http = app.state::<api::SharedHttpClient>();
    api::text_to_speech(&http.client, &cfg.openai_api_key, &text, cfg.openai_url()).await
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

    let cfg = app.state::<config::ConfigCache>().get()?;
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
    let http = app.state::<api::SharedHttpClient>();
    api::generate_answer_streaming(
        &app,
        &http.client,
        &cfg.openai_api_key,
        &format!("Generate a post-interview summary for this conversation:\n\n{conversation}"),
        "general",
        "",
        &[],
        &cfg.resume_text,
        &cfg.job_description,
        cfg.openai_url(),
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
        .manage(NightModeState::new())
        .manage(screenshot::ScreenshotQueue::new())
        .manage(ProctorState::new())
        .manage(api::SharedHttpClient::new())
        .manage(config::ConfigCache::new())
        .invoke_handler(tauri::generate_handler![
            overlay::set_click_through,
            overlay::toggle_overlay_visibility,
            overlay::move_overlay,
            overlay::resize_overlay,
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
            proctor_detect::proctor_scan,
            proctor_detect::proctor_quick_scan,
            api::test_api_keys,
        ])
        .setup(|app| {
            let handle = app.handle().clone();

            // Process-level stealth: below-normal priority + efficiency mode
            overlay::apply_process_stealth();

            // Apply content protection
            if let Err(e) = overlay::apply_content_protection(&handle) {
                let _ = handle.emit("pipeline:error", format!("Content protection failed: {e}"));
            }

            // Hide main window from Alt+Tab / Win+Tab
            overlay::hide_from_alt_tab(&handle);

            // Enable click-through by default
            if let Err(e) = overlay::set_click_through(handle.clone(), true) {
                let _ = handle.emit("pipeline:error", format!("Click-through failed: {e}"));
            }

            // Register global hotkeys
            if let Err(e) = hotkeys::register_all(&handle) {
                let _ = handle.emit("pipeline:error", format!("Hotkey registration failed: {e}"));
            }

            // Run proctoring detection scan on startup (background thread)
            {
                let scan_handle = handle.clone();
                std::thread::spawn(move || {
                    use tauri::Manager;
                    let report = proctor_detect::full_scan();
                    let _ = scan_handle.emit("proctor:scan-result", &report);

                    // Store in managed state for other systems to query
                    if let Some(state) = scan_handle.try_state::<ProctorState>() {
                        *state.report.lock().unwrap_or_else(|e| e.into_inner()) =
                            Some(report.clone());
                    }

                    // --- Adaptive behavior based on detection ---

                    // If WDA detection is active, disable capture protection
                    // (being detected is worse than being visible to capture)
                    if report.active_capabilities.any_wda_detection {
                        #[cfg(target_os = "windows")]
                        {
                            if let Some(window) = scan_handle.get_webview_window("main") {
                                if let Ok(hwnd) = window.hwnd() {
                                    unsafe {
                                        // Reset to WDA_NONE (0) — removes the detectable flag
                                        windows_sys::Win32::UI::WindowsAndMessaging::SetWindowDisplayAffinity(
                                            hwnd.0, 0
                                        );
                                    }
                                }
                            }
                        }
                        let _ = scan_handle.emit("proctor:adaptation",
                            "WDA scanner detected — capture protection disabled to avoid detection");
                    }

                    // If keyboard hooks detected, notify frontend to suggest tray usage
                    if report.active_capabilities.any_keyboard_hook {
                        let _ = scan_handle.emit("proctor:adaptation",
                            "Keyboard hooks detected — use tray menu if hotkeys stop working");
                    }

                    // If network monitoring detected, warn about API visibility
                    if report.active_capabilities.any_network_monitor {
                        let _ = scan_handle.emit("proctor:adaptation",
                            "Network monitoring detected — API calls may be logged");
                    }

                    // Log to console in debug mode
                    #[cfg(debug_assertions)]
                    {
                        if report.vendors.is_empty() {
                            println!("[proctor] No proctoring software detected");
                        } else {
                            println!("[proctor] DETECTED {} vendor(s): {:?}",
                                report.vendors.len(),
                                report.vendors.iter().map(|v| v.name).collect::<Vec<_>>()
                            );
                            println!("[proctor] Threat level: {:?}", report.threat_level);
                            for rec in &report.recommendations {
                                println!("[proctor] >> {}", rec);
                            }
                        }
                    }
                });
            }

            // Periodic proctoring re-scan (every 60s, quick scan — process only)
            {
                let periodic_handle = handle.clone();
                std::thread::spawn(move || {
                    use tauri::Manager;
                    loop {
                        std::thread::sleep(std::time::Duration::from_secs(60));
                        let report = proctor_detect::quick_scan();

                        // Only emit if something changed (vendors detected or cleared)
                        let prev_empty = periodic_handle
                            .try_state::<ProctorState>()
                            .map(|s| {
                                s.report.lock().unwrap_or_else(|e| e.into_inner())
                                    .as_ref()
                                    .map(|r| r.vendors.is_empty())
                                    .unwrap_or(true)
                            })
                            .unwrap_or(true);
                        let now_empty = report.vendors.is_empty();

                        if prev_empty != now_empty || !now_empty {
                            let _ = periodic_handle.emit("proctor:scan-result", &report);
                            if let Some(state) = periodic_handle.try_state::<ProctorState>() {
                                *state.report.lock().unwrap_or_else(|e| e.into_inner()) =
                                    Some(report);
                            }
                        }
                    }
                });
            }

            // Pre-warm HTTP connections (TLS handshake in background)
            {
                use tauri::Manager;
                let client = handle.state::<api::SharedHttpClient>().client.clone();
                let cfg = handle.state::<config::ConfigCache>().get().unwrap_or_default();
                let openai_url = cfg.openai_url().to_string();
                let groq_url = cfg.groq_url().to_string();
                tauri::async_runtime::spawn(async move {
                    let targets = [openai_url, groq_url];
                    for url in targets {
                        let _ = client.head(&url).send().await;
                    }
                });
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
                // Force-hide via Win32 — ShowWindow + SetWindowPos works cross-thread
                // (DestroyWindow silently fails from non-UI threads)
                overlay::force_hide_window(&exit_handle);
                // Also hide tray icon
                if let Some(tray) = exit_handle.tray_by_id("main") {
                    let _ = tray.set_visible(false);
                }
                // Use Tauri's exit for proper cleanup instead of std::process::exit
                let handle_for_exit = exit_handle.clone();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(200));
                    handle_for_exit.exit(0);
                });
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
                    // Toggle WS_EX_NOACTIVATE: on when passive (click-through), off when interactive
                    overlay::update_noactivate(&ct_handle, new_val);
                    let _ = ct_handle.emit("hotkey:toggle-click-through-ui", new_val);
                    // Update tray menu label
                    if let Some(tray_state) = ct_handle.try_state::<TrayMenuState>() {
                        let label = if new_val { "Click-Through: ON" } else { "Click-Through: OFF" };
                        let _ = tray_state.click_through_item.set_text(label);
                    }
                });
            }

            // Handle toggle night mode (Ctrl+Shift+Z)
            let nm_handle = handle.clone();
            app.listen("hotkey:toggle-night-mode", move |_| {
                use tauri::Manager;
                let _ = nm_handle.emit("hotkey:toggle-night-mode-ui", ());
                // Toggle backend state mirror and update tray label
                let nm_state = nm_handle.state::<NightModeState>();
                let was = nm_state.enabled.load(std::sync::atomic::Ordering::SeqCst);
                nm_state.enabled.store(!was, std::sync::atomic::Ordering::SeqCst);
                if let Some(tray_state) = nm_handle.try_state::<TrayMenuState>() {
                    let label = if !was { "Night Mode: ON" } else { "Night Mode: OFF" };
                    let _ = tray_state.night_mode_item.set_text(label);
                }
            });

            // Handle toggle visibility (Ctrl+Shift+H)
            let vis_handle = handle.clone();
            app.listen("hotkey:toggle-visibility", move |_| {
                let _ = overlay::toggle_overlay_visibility(vis_handle.clone());
            });

            // Move and resize hotkeys are handled in the frontend (app.js)
            // with hold-to-repeat support via intervals

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

                    // Cycle: None → dsa → oa → system-design → lld → ai-interview → project-deep-dive → ai-ml → cloud → backend → qa → behavioral → None
                    let next = match locked.as_deref() {
                        None              => Some("dsa".to_string()),
                        Some("dsa")       => Some("oa".to_string()),
                        Some("oa")        => Some("system-design".to_string()),
                        Some("system-design") => Some("lld".to_string()),
                        Some("lld")       => Some("ai-interview".to_string()),
                        Some("ai-interview") => Some("project-deep-dive".to_string()),
                        Some("project-deep-dive") => Some("ai-ml".to_string()),
                        Some("ai-ml")     => Some("cloud".to_string()),
                        Some("cloud")     => Some("backend".to_string()),
                        Some("backend")   => Some("qa".to_string()),
                        Some("qa")        => Some("behavioral".to_string()),
                        Some("behavioral") => None,
                        Some(_)           => None,
                    };

                    if let Some(ref mode) = next {
                        *history.last_mode.lock().unwrap_or_else(|e| e.into_inner()) = mode.clone();
                    }

                    let mode_name = next.as_deref().unwrap_or("auto").to_string();
                    *locked = next;

                    let _ = cycle_handle.emit("mode:locked", &mode_name);
                    // Update tray submenu label
                    if let Some(tray_state) = cycle_handle.try_state::<TrayMenuState>() {
                        let display = match mode_name.as_str() {
                            "auto" => "Mode: Auto-Detect".to_string(),
                            other => format!("Mode: {}", other.to_uppercase()),
                        };
                        let _ = tray_state.mode_submenu.set_text(&display);
                    }
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
                    if let Some(tray_state) = unlock_handle.try_state::<TrayMenuState>() {
                        let _ = tray_state.mode_submenu.set_text("Mode: Auto-Detect");
                    }
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
                        // Update tray menu label
                        if let Some(tray_state) = rec_handle.try_state::<TrayMenuState>() {
                            let _ = tray_state.record_item.set_text("Start Recording");
                        }

                        // Spawn the AI pipeline (with concurrency guard)
                        let pipeline_handle = rec_handle.clone();
                        let wav = wav_bytes;
                        tauri::async_runtime::spawn(async move {
                            // _guard must be bound to a named variable (not `_`) so it lives
                            // for the entire async block and is dropped on any exit path.
                            let _guard = match PipelineGuard::try_acquire(&pipeline_handle) {
                                Some(g) => g,
                                None => {
                                    let _ = pipeline_handle.emit("pipeline:error", "Another pipeline is still running");
                                    return;
                                }
                            };
                            let result = run_pipeline(pipeline_handle.clone(), wav).await;
                            if let Err(e) = result {
                                let _ = pipeline_handle.emit("pipeline:error", &e);
                            }
                        });
                    } else {
                        let audio_source = rec_handle.state::<config::ConfigCache>().get()
                            .map(|c| c.audio_source)
                            .unwrap_or_else(|_| "both".to_string());
                        match engine.start_recording(&audio_source) {
                            Ok(warning) => {
                                let _ = rec_handle.emit("recording:started", ());
                                // Update tray menu label
                                if let Some(tray_state) = rec_handle.try_state::<TrayMenuState>() {
                                    let _ = tray_state.record_item.set_text("Stop Recording");
                                }
                                if let Some(msg) = warning {
                                    let _ = rec_handle.emit("recording:warning", &msg);
                                }
                            }
                            Err(e) => {
                                let _ = rec_handle.emit("recording:error", e);
                                // Ensure tray label stays correct on failure
                                if let Some(tray_state) = rec_handle.try_state::<TrayMenuState>() {
                                    let _ = tray_state.record_item.set_text("Start Recording");
                                }
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
                        use tauri::Manager;
                        if let Ok(cfg) = summary_handle.state::<config::ConfigCache>().get() {
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
                                let http = summary_handle.state::<api::SharedHttpClient>();
                                if let Ok(summary) = api::generate_answer_silent(
                                    &http.client,
                                    &cfg.openai_api_key,
                                    &summary_prompt,
                                    cfg.openai_url(),
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

                    // Smart analyze: if no screenshots queued, auto-capture one first
                    {
                        let q = queue.queue.lock().unwrap_or_else(|e| e.into_inner());
                        if q.is_empty() {
                            drop(q); // release lock before capture
                            match screenshot::capture_screen(&ah) {
                                Ok(png_bytes) => {
                                    let mut q = queue.queue.lock().unwrap_or_else(|e| e.into_inner());
                                    q.push(png_bytes);
                                    let count = q.len();
                                    let _ = ah.emit("screenshot:taken", count);
                                }
                                Err(e) => {
                                    let _ = ah.emit("pipeline:error", &format!("Screenshot failed: {e}"));
                                    return;
                                }
                            }
                        }
                    }

                    tauri::async_runtime::spawn(async move {
                        // _guard must be bound to a named variable (not `_`) so it lives
                        // for the entire async block and is dropped on any exit path.
                        let _guard = match PipelineGuard::try_acquire(&ah) {
                            Some(g) => g,
                            None => {
                                let _ = ah.emit("pipeline:error", "Another pipeline is still running");
                                return;
                            }
                        };

                        let _ = ah.emit("pipeline:started", ());
                        let _ = ah.emit("pipeline:status", "Analyzing screenshots...");

                        let cfg = match ah.state::<config::ConfigCache>().get() {
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
                        let history_guard = ah.state::<ConversationHistory>();
                        let current_mode = {
                            let locked = history_guard.locked_mode.lock().unwrap_or_else(|e| e.into_inner()).clone();
                            if let Some(locked_mode) = locked {
                                locked_mode
                            } else {
                                history_guard.last_mode.lock().unwrap_or_else(|e| e.into_inner()).clone()
                            }
                        };

                        // Snapshot conversation history for the model to see previous solutions
                        let history_snapshot = {
                            let msgs = history_guard.messages.lock().unwrap_or_else(|e| e.into_inner());
                            msgs.clone()
                        };

                        let http = ah.state::<api::SharedHttpClient>();
                        match api::analyze_screenshots(
                            &ah,
                            &http.client,
                            &cfg.openai_api_key,
                            &screenshots_b64,
                            &current_mode,
                            &history_snapshot,
                            cfg.openai_url(),
                        )
                        .await
                        {
                            Ok(answer) => {
                                // Only set last_mode to OA if not locked and not in a live interview context
                                let is_locked = history_guard.locked_mode.lock().unwrap_or_else(|e| e.into_inner()).is_some();
                                if !is_locked {
                                    let is_live = matches!(current_mode.as_str(), "dsa" | "ai-interview" | "system-design" | "lld" | "behavioral" | "ai-ml" | "backend" | "java" | "python" | "dbms" | "cloud" | "qa" | "project-deep-dive");
                                    if !is_live {
                                        *history_guard.last_mode.lock().unwrap_or_else(|e| e.into_inner()) = "OA".to_string();
                                    }
                                }
                                let mut msgs = history_guard.messages.lock().unwrap_or_else(|e| e.into_inner());
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
                        // _guard is dropped here, automatically releasing the pipeline lock
                    });
                });
            }

            // System tray
            {
                use tauri::Manager;
                use tauri::menu::{MenuBuilder, MenuItem, SubmenuBuilder};
                use tauri::tray::TrayIconBuilder;

                let record_item = MenuItem::with_id(app, "record", "Start Recording", true, None::<&str>)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                let screenshot_item = MenuItem::with_id(app, "screenshot", "Take Screenshot", true, None::<&str>)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                let analyze_item = MenuItem::with_id(app, "analyze", "Analyze Screenshots", true, None::<&str>)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

                let mode_submenu = SubmenuBuilder::with_id(app, "mode-submenu", "Mode: Auto-Detect")
                    .item(&MenuItem::with_id(app, "mode-auto", "Auto-Detect", true, None::<&str>).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .separator()
                    .item(&MenuItem::with_id(app, "mode-dsa", "DSA", true, None::<&str>).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-oa", "OA", true, None::<&str>).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-system-design", "System Design", true, None::<&str>).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-lld", "LLD", true, None::<&str>).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-ai-interview", "AI-Interview", true, None::<&str>).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-project-deep-dive", "Project Deep-Dive", true, None::<&str>).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-ai-ml", "AI-ML", true, None::<&str>).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-cloud", "Cloud", true, None::<&str>).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-backend", "Backend", true, None::<&str>).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-qa", "QA", true, None::<&str>).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-behavioral", "Behavioral", true, None::<&str>).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-java", "Java", true, None::<&str>).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-python", "Python", true, None::<&str>).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-dbms", "DBMS", true, None::<&str>).map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .build()
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

                let copy_answer_item = MenuItem::with_id(app, "copy-answer", "Copy Answer", true, None::<&str>)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                let copy_code_item = MenuItem::with_id(app, "copy-code", "Copy Code Blocks", true, None::<&str>)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

                let night_mode_item = MenuItem::with_id(app, "night-mode", "Night Mode: OFF", true, None::<&str>)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                let click_through_item = MenuItem::with_id(app, "click-through", "Click-Through: ON", true, None::<&str>)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

                let rescan_item = MenuItem::with_id(app, "proctor-rescan", "Rescan Environment", true, None::<&str>)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                let clear_session_item = MenuItem::with_id(app, "clear-session", "Clear Session", true, None::<&str>)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                let quit_item = MenuItem::with_id(app, "quit", "Exit Audio Service", true, None::<&str>)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

                let menu = MenuBuilder::new(app)
                    .item(&record_item)
                    .item(&screenshot_item)
                    .item(&analyze_item)
                    .separator()
                    .item(&mode_submenu)
                    .separator()
                    .item(&copy_answer_item)
                    .item(&copy_code_item)
                    .separator()
                    .item(&night_mode_item)
                    .item(&click_through_item)
                    .separator()
                    .item(&rescan_item)
                    .item(&clear_session_item)
                    .item(&quit_item)
                    .build()
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

                app.manage(TrayMenuState {
                    record_item,
                    night_mode_item,
                    click_through_item,
                    mode_submenu,
                });

                let _tray = TrayIconBuilder::new()
                    .tooltip("Windows Audio Device Manager")
                    .menu(&menu)
                    .on_menu_event(move |tray_app, event| {
                        use tauri::Manager;
                        let id = event.id().as_ref();
                        match id {
                            "record" => { let _ = tray_app.emit("hotkey:toggle-recording", ()); }
                            "screenshot" => { let _ = tray_app.emit("hotkey:screenshot", ()); }
                            "analyze" => { let _ = tray_app.emit("hotkey:analyze", ()); }
                            "copy-answer" => { let _ = tray_app.emit("hotkey:copy-answer", ()); }
                            "copy-code" => { let _ = tray_app.emit("hotkey:copy-code", ()); }
                            "night-mode" => { let _ = tray_app.emit("hotkey:toggle-night-mode", ()); }
                            "click-through" => { let _ = tray_app.emit("hotkey:toggle-click-through", ()); }
                            "proctor-rescan" => {
                                let rescan_handle = tray_app.clone();
                                std::thread::spawn(move || {
                                    let report = proctor_detect::full_scan();
                                    let _ = rescan_handle.emit("proctor:scan-result", &report);
                                    if let Some(state) = rescan_handle.try_state::<ProctorState>() {
                                        *state.report.lock().unwrap_or_else(|e| e.into_inner()) =
                                            Some(report);
                                    }
                                });
                            }
                            "clear-session" => { let _ = tray_app.emit("hotkey:clear-session", ()); }
                            "quit" => {
                                hotkeys::unregister_all(tray_app);
                                if let Some(win) = tray_app.get_webview_window("main") {
                                    let _ = win.close();
                                }
                                std::thread::spawn(|| {
                                    std::thread::sleep(std::time::Duration::from_millis(200));
                                    std::process::exit(0);
                                });
                            }
                            mode_id if mode_id.starts_with("mode-") => {
                                let mode = &mode_id["mode-".len()..];
                                let history = tray_app.state::<ConversationHistory>();
                                if mode == "auto" {
                                    *history.locked_mode.lock().unwrap_or_else(|e| e.into_inner()) = None;
                                    *history.last_mode.lock().unwrap_or_else(|e| e.into_inner()) = "general".to_string();
                                    let _ = tray_app.emit("mode:locked", "auto");
                                    if let Some(tray_state) = tray_app.try_state::<TrayMenuState>() {
                                        let _ = tray_state.mode_submenu.set_text("Mode: Auto-Detect");
                                    }
                                } else {
                                    *history.locked_mode.lock().unwrap_or_else(|e| e.into_inner()) = Some(mode.to_string());
                                    *history.last_mode.lock().unwrap_or_else(|e| e.into_inner()) = mode.to_string();
                                    let _ = tray_app.emit("mode:locked", mode);
                                    if let Some(tray_state) = tray_app.try_state::<TrayMenuState>() {
                                        let _ = tray_state.mode_submenu.set_text(&format!("Mode: {}", mode.to_uppercase()));
                                    }
                                }
                            }
                            _ => {}
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
