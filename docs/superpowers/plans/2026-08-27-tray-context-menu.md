# Tray Context Menu Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a comprehensive right-click context menu to Phantom's system tray icon as a fallback control mechanism when proctoring software blocks global hotkeys.

**Architecture:** The tray menu is a parallel input path that emits the exact same Tauri events as hotkeys. A `TrayMenuState` struct stores handles to dynamic menu items for runtime label updates. A `NightModeState` struct mirrors the frontend's night mode boolean. All changes are confined to `main.rs` — no other files are touched.

**Tech Stack:** Tauri 2.11.5 menu API (`MenuItem`, `Submenu`, `SubmenuBuilder`, `MenuBuilder`), existing Tauri event system.

**Hard Constraint:** Zero impact on existing functionality. No changes to api.rs, audio.rs, overlay.rs, screenshot.rs, config.rs, hotkeys.rs, experience.rs, app.js, index.html, styles.css, or Cargo.toml.

---

### Task 1: Add NightModeState struct and TrayMenuState struct

**Files:**
- Modify: `src-tauri/src/main.rs:31-41` (after `ClickThroughState`)

- [ ] **Step 1: Add the NightModeState struct after ClickThroughState**

Add these two structs after the `ClickThroughState` impl block (after line 41):

```rust
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

pub struct TrayMenuState {
    pub record_item: tauri::menu::MenuItem<tauri::Wry>,
    pub night_mode_item: tauri::menu::MenuItem<tauri::Wry>,
    pub click_through_item: tauri::menu::MenuItem<tauri::Wry>,
    pub mode_submenu: tauri::menu::Submenu<tauri::Wry>,
}
```

- [ ] **Step 2: Register NightModeState in the Tauri builder**

In the `.manage()` chain (around line 330), add:

```rust
.manage(NightModeState::new())
```

Add it after `.manage(ClickThroughState::new())` (line 330).

- [ ] **Step 3: Verify it compiles**

Run: `cd c:/Dev/computerApplication/phantom/src-tauri && cargo check 2>&1 | tail -5`
Expected: Compiles with possible warnings about unused structs (TrayMenuState not used yet). No errors.

- [ ] **Step 4: Commit**

```bash
cd c:/Dev/computerApplication/phantom
git add src-tauri/src/main.rs
git commit -m "feat: add NightModeState and TrayMenuState structs for tray menu"
```

---

### Task 2: Build the full tray menu with submenu

**Files:**
- Modify: `src-tauri/src/main.rs:810-838` (replace the existing tray setup block)

- [ ] **Step 1: Replace the tray setup block**

Replace the entire `// System tray` block (lines 810-838) with the following:

```rust
            // System tray with full context menu
            {
                use tauri::menu::{MenuBuilder, MenuItem, Submenu, SubmenuBuilder};
                use tauri::tray::TrayIconBuilder;

                // --- Menu items that need runtime label updates ---
                let record_item = MenuItem::with_id(app, "record", "Start Recording", true, None::<&str>)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                let night_mode_item = MenuItem::with_id(app, "night-mode", "Night Mode: OFF", true, None::<&str>)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                let click_through_item = MenuItem::with_id(app, "click-through", "Click-Through: ON", true, None::<&str>)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

                // --- Static menu items ---
                let screenshot_item = MenuItem::with_id(app, "screenshot", "Take Screenshot", true, None::<&str>)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                let analyze_item = MenuItem::with_id(app, "analyze", "Analyze Screenshots", true, None::<&str>)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                let copy_answer_item = MenuItem::with_id(app, "copy-answer", "Copy Answer", true, None::<&str>)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                let copy_code_item = MenuItem::with_id(app, "copy-code", "Copy Code Blocks", true, None::<&str>)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                let clear_session_item = MenuItem::with_id(app, "clear-session", "Clear Session", true, None::<&str>)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
                let quit_item = MenuItem::with_id(app, "quit", "Exit Audio Service", true, None::<&str>)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

                // --- Mode submenu ---
                let mode_submenu = SubmenuBuilder::with_id(app, "mode-submenu", "Mode: Auto-Detect")
                    .item(&MenuItem::with_id(app, "mode-auto", "Auto-Detect", true, None::<&str>)
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .separator()
                    .item(&MenuItem::with_id(app, "mode-dsa", "DSA", true, None::<&str>)
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-oa", "OA", true, None::<&str>)
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-system-design", "System Design", true, None::<&str>)
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-lld", "LLD", true, None::<&str>)
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-ai-interview", "AI-Interview", true, None::<&str>)
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-project-deep-dive", "Project Deep-Dive", true, None::<&str>)
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-ai-ml", "AI-ML", true, None::<&str>)
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-cloud", "Cloud", true, None::<&str>)
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-backend", "Backend", true, None::<&str>)
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-qa", "QA", true, None::<&str>)
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-behavioral", "Behavioral", true, None::<&str>)
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-java", "Java", true, None::<&str>)
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-python", "Python", true, None::<&str>)
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .item(&MenuItem::with_id(app, "mode-dbms", "DBMS", true, None::<&str>)
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?)
                    .build()
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

                // --- Assemble the main menu ---
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
                    .item(&clear_session_item)
                    .item(&quit_item)
                    .build()
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

                // Store dynamic menu item handles for runtime label updates
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
                        let id = event.id().as_ref();
                        // Handled in Task 3
                        match id {
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
                            _ => {} // Other handlers added in Task 3
                        }
                    })
                    .build(app)
                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;
            }
```

- [ ] **Step 2: Add missing import for Manager trait**

The `on_menu_event` closure uses `tray_app.get_webview_window()` which requires `Manager`. Add inside the closure if not already in scope:

```rust
use tauri::Manager;
```

This import already exists in the current quit handler — just ensure it's present in the new closure.

- [ ] **Step 3: Verify it compiles**

Run: `cd c:/Dev/computerApplication/phantom/src-tauri && cargo check 2>&1 | tail -5`
Expected: Compiles successfully. The menu should appear with all items when Phantom is launched. Only "Exit Audio Service" (quit) works at this point — other items are no-ops.

- [ ] **Step 4: Commit**

```bash
cd c:/Dev/computerApplication/phantom
git add src-tauri/src/main.rs
git commit -m "feat: build full tray context menu with mode submenu"
```

---

### Task 3: Wire up menu event handler to emit hotkey events

**Files:**
- Modify: `src-tauri/src/main.rs` (the `on_menu_event` closure from Task 2)

- [ ] **Step 1: Replace the `on_menu_event` closure body**

Replace the `on_menu_event` closure in the tray builder (from Task 2) with the full handler:

```rust
                    .on_menu_event(move |tray_app, event| {
                        use tauri::Manager;
                        let id = event.id().as_ref();

                        match id {
                            // --- Actions that emit existing hotkey events ---
                            "record" => {
                                let _ = tray_app.emit("hotkey:toggle-recording", ());
                            }
                            "screenshot" => {
                                let _ = tray_app.emit("hotkey:screenshot", ());
                            }
                            "analyze" => {
                                let _ = tray_app.emit("hotkey:analyze", ());
                            }
                            "copy-answer" => {
                                let _ = tray_app.emit("hotkey:copy-answer", ());
                            }
                            "copy-code" => {
                                let _ = tray_app.emit("hotkey:copy-code", ());
                            }
                            "night-mode" => {
                                let _ = tray_app.emit("hotkey:toggle-night-mode", ());
                            }
                            "click-through" => {
                                let _ = tray_app.emit("hotkey:toggle-click-through", ());
                            }
                            "clear-session" => {
                                let _ = tray_app.emit("hotkey:clear-session", ());
                            }

                            // --- Mode selection (direct lock, no cycling) ---
                            _ if id.starts_with("mode-") => {
                                let history = tray_app.state::<ConversationHistory>();
                                let mode_key = &id[5..]; // strip "mode-" prefix

                                if mode_key == "auto" {
                                    *history.locked_mode.lock().unwrap_or_else(|e| e.into_inner()) = None;
                                    *history.last_mode.lock().unwrap_or_else(|e| e.into_inner()) = "general".to_string();
                                    let _ = tray_app.emit("mode:locked", "auto");
                                } else {
                                    let mode = mode_key.to_string();
                                    *history.locked_mode.lock().unwrap_or_else(|e| e.into_inner()) = Some(mode.clone());
                                    *history.last_mode.lock().unwrap_or_else(|e| e.into_inner()) = mode.clone();
                                    let _ = tray_app.emit("mode:locked", &mode);
                                }
                            }

                            // --- Quit ---
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

                            _ => {}
                        }
                    })
```

- [ ] **Step 2: Verify it compiles**

Run: `cd c:/Dev/computerApplication/phantom/src-tauri && cargo check 2>&1 | tail -5`
Expected: Compiles successfully. All menu items now emit the correct events. Recording, screenshot, analyze, copy, night mode, click-through, clear session, mode selection, and quit should all work via the tray menu.

- [ ] **Step 3: Commit**

```bash
cd c:/Dev/computerApplication/phantom
git add src-tauri/src/main.rs
git commit -m "feat: wire tray menu events to existing hotkey event handlers"
```

---

### Task 4: Add dynamic label updates for recording state

**Files:**
- Modify: `src-tauri/src/main.rs` (the existing `hotkey:toggle-recording` listener, around line 538)

- [ ] **Step 1: Add label update after recording state changes**

In the existing `hotkey:toggle-recording` listener, add label updates after each state transition. Find the block that starts with `app.listen("hotkey:toggle-recording", move |_| {` (around line 538).

After the line `let _ = rec_handle.emit("recording:stopped", byte_count);` (around line 546), add:

```rust
                        // Update tray menu label
                        if let Some(tray_state) = rec_handle.try_state::<TrayMenuState>() {
                            let _ = tray_state.record_item.set_text("Start Recording");
                        }
```

After the line `let _ = rec_handle.emit("recording:started", ());` (around line 574), add:

```rust
                            // Update tray menu label
                            if let Some(tray_state) = rec_handle.try_state::<TrayMenuState>() {
                                let _ = tray_state.record_item.set_text("Stop Recording");
                            }
```

After the `Err(e)` branch for recording errors (around line 580), add the same reset inside that branch (recording failed to start, so keep label as "Start Recording"):

```rust
                            Err(e) => {
                                let _ = rec_handle.emit("recording:error", e);
                                // Ensure tray label stays correct on failure
                                if let Some(tray_state) = rec_handle.try_state::<TrayMenuState>() {
                                    let _ = tray_state.record_item.set_text("Start Recording");
                                }
                            }
```

- [ ] **Step 2: Verify it compiles**

Run: `cd c:/Dev/computerApplication/phantom/src-tauri && cargo check 2>&1 | tail -5`
Expected: Compiles successfully.

- [ ] **Step 3: Commit**

```bash
cd c:/Dev/computerApplication/phantom
git add src-tauri/src/main.rs
git commit -m "feat: update tray recording label on state change"
```

---

### Task 5: Add dynamic label updates for click-through, night mode, and mode selection

**Files:**
- Modify: `src-tauri/src/main.rs` (existing hotkey listeners for click-through, night mode, and mode-cycle/unlock)

- [ ] **Step 1: Update click-through label**

In the existing `hotkey:toggle-click-through` listener (around line 447), after the line `let _ = ct_handle.emit("hotkey:toggle-click-through-ui", new_val);` add:

```rust
                    // Update tray menu label
                    if let Some(tray_state) = ct_handle.try_state::<TrayMenuState>() {
                        let label = if new_val { "Click-Through: ON" } else { "Click-Through: OFF" };
                        let _ = tray_state.click_through_item.set_text(label);
                    }
```

- [ ] **Step 2: Update night mode label**

In the existing `hotkey:toggle-night-mode` listener (around line 461), replace the body with:

```rust
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
```

- [ ] **Step 3: Update mode submenu label on mode-cycle**

In the existing `hotkey:mode-cycle` listener (around line 490), after the line `let _ = cycle_handle.emit("mode:locked", &mode_name);` add:

```rust
                    // Update tray submenu label
                    if let Some(tray_state) = cycle_handle.try_state::<TrayMenuState>() {
                        let display = match mode_name.as_str() {
                            "auto" => "Mode: Auto-Detect".to_string(),
                            other => format!("Mode: {}", other.to_uppercase()),
                        };
                        let _ = tray_state.mode_submenu.set_text(&display);
                    }
```

- [ ] **Step 4: Update mode submenu label on mode-unlock**

In the existing `hotkey:mode-unlock` listener (around line 526), after the line `let _ = unlock_handle.emit("mode:locked", "auto");` add:

```rust
                    if let Some(tray_state) = unlock_handle.try_state::<TrayMenuState>() {
                        let _ = tray_state.mode_submenu.set_text("Mode: Auto-Detect");
                    }
```

- [ ] **Step 5: Update mode submenu label from tray menu's own mode selection**

In the `on_menu_event` closure (Task 3), within the `_ if id.starts_with("mode-")` branch, after emitting `mode:locked`, add the same label update. Replace the mode branch with:

```rust
                            _ if id.starts_with("mode-") => {
                                let history = tray_app.state::<ConversationHistory>();
                                let mode_key = &id[5..]; // strip "mode-" prefix

                                if mode_key == "auto" {
                                    *history.locked_mode.lock().unwrap_or_else(|e| e.into_inner()) = None;
                                    *history.last_mode.lock().unwrap_or_else(|e| e.into_inner()) = "general".to_string();
                                    let _ = tray_app.emit("mode:locked", "auto");
                                    // Update tray submenu label
                                    if let Some(tray_state) = tray_app.try_state::<TrayMenuState>() {
                                        let _ = tray_state.mode_submenu.set_text("Mode: Auto-Detect");
                                    }
                                } else {
                                    let mode = mode_key.to_string();
                                    *history.locked_mode.lock().unwrap_or_else(|e| e.into_inner()) = Some(mode.clone());
                                    *history.last_mode.lock().unwrap_or_else(|e| e.into_inner()) = mode.clone();
                                    let _ = tray_app.emit("mode:locked", &mode);
                                    // Update tray submenu label
                                    if let Some(tray_state) = tray_app.try_state::<TrayMenuState>() {
                                        let _ = tray_state.mode_submenu.set_text(&format!("Mode: {}", mode.to_uppercase()));
                                    }
                                }
                            }
```

- [ ] **Step 6: Verify it compiles**

Run: `cd c:/Dev/computerApplication/phantom/src-tauri && cargo check 2>&1 | tail -5`
Expected: Compiles successfully.

- [ ] **Step 7: Commit**

```bash
cd c:/Dev/computerApplication/phantom
git add src-tauri/src/main.rs
git commit -m "feat: dynamic tray label updates for click-through, night mode, and mode selection"
```

---

### Task 6: Build release binary and manual verification

**Files:**
- No file changes — verification only.

- [ ] **Step 1: Build the release binary**

Run: `cd c:/Dev/computerApplication/phantom/src-tauri && cargo build --release 2>&1 | tail -5`
Expected: Compiles in release mode with no errors. Binary at `target/release/audiodvc.exe`.

- [ ] **Step 2: Verify binary size hasn't bloated**

Run: `ls -lh c:/Dev/computerApplication/phantom/src-tauri/target/release/audiodvc.exe | awk '{print $5}'`
Expected: ~11-12MB (same ballpark as before — menu items add negligible size).

- [ ] **Step 3: Document manual test checklist**

The following must be manually verified by running the built binary:

1. Right-click tray icon → full menu appears with all items and separators
2. "Start Recording" → starts recording, label changes to "Stop Recording"
3. "Stop Recording" → stops recording, pipeline runs, label changes back to "Start Recording"
4. "Take Screenshot" → screenshot captured, count updates in overlay
5. "Analyze Screenshots" → analysis pipeline runs
6. Mode submenu → select "DSA" → parent label changes to "Mode: DSA", mode locks
7. Mode submenu → select "Auto-Detect" → parent label changes to "Mode: Auto-Detect", mode unlocks
8. "Copy Answer" → answer text copied to clipboard
9. "Copy Code Blocks" → code blocks copied to clipboard
10. "Night Mode: OFF" → toggles night mode, label changes to "Night Mode: ON"
11. "Click-Through: ON" → toggles click-through, label changes to "Click-Through: OFF"
12. "Clear Session" → session cleared, overlay resets
13. "Exit Audio Service" → app exits cleanly
14. All existing hotkeys still work (Ctrl+Shift+F6, F7, F, F4, H, Z, X, Q, etc.)
15. Hotkey-triggered state changes update tray labels (e.g., Ctrl+Shift+F6 changes "Start Recording" to "Stop Recording")
16. Pipeline (record → transcribe → extract → answer) produces identical results whether triggered via hotkey or tray menu
17. Tray icon tooltip still shows "Windows Audio Device Manager"

- [ ] **Step 4: Commit (if any fixes were needed)**

```bash
cd c:/Dev/computerApplication/phantom
git add src-tauri/src/main.rs
git commit -m "fix: tray menu verification fixes"
```

Only commit if fixes were made during verification. Skip if everything passed cleanly.
