use tauri::{AppHandle, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

/// All hotkey events emitted to the frontend
/// Hotkey bindings — Ctrl+Shift with conflict-free keys
/// Avoided: N (Chrome incognito), L (VS Code), S (Save As), I (DevTools), F (Find)
/// Hotkey bindings — Ctrl+Shift with browser-safe keys
/// Edge/Chrome steal these Ctrl+Shift combos, so we AVOID them:
///   A (search tabs), B (bookmarks), C (inspect), D (bookmark all),
///   E (search), I (DevTools), J (downloads), L (address bar),
///   M (profiles), N (incognito), P (InPrivate), Q (close all),
///   R (hard refresh), S (save), T (reopen tab), U (source),
///   V (paste plain), W (close), Y (collections)
/// Safe keys: F, H, X, Z, numbers, arrows, brackets, Backspace
/// NOT safe (verified): G (find prev), K (Edge dup tab), O (bookmark mgr)
const HOTKEY_EVENTS: &[(&str, &str)] = &[
    ("Ctrl+Shift+F6",      "hotkey:toggle-recording"),     // Record/stop
    ("Ctrl+Shift+F7",      "hotkey:screenshot"),            // Grab screenshot
    ("Ctrl+Shift+F",       "hotkey:analyze"),              // Find/analyze screenshots
    ("Ctrl+Shift+F4",      "hotkey:toggle-click-through"), // Toggle click-through
    ("Ctrl+Shift+H",       "hotkey:toggle-visibility"),    // Hide/show overlay
    ("Ctrl+Shift+Z",       "hotkey:toggle-night-mode"),    // Night mode
    ("Ctrl+Shift+F2",      "hotkey:snap-to-webcam"),       // Snap to webcam
    ("Ctrl+Shift+Q",       "hotkey:emergency-exit"),       // Emergency exit
    ("Ctrl+Shift+X",       "hotkey:copy-answer"),          // Copy answer
    ("Ctrl+Shift+F3",      "hotkey:open-settings"),        // Open settings
    ("Ctrl+Shift+Up",      "hotkey:move-up"),              // Move overlay
    ("Ctrl+Shift+Down",    "hotkey:move-down"),
    ("Ctrl+Shift+Left",    "hotkey:move-left"),
    ("Ctrl+Shift+Right",   "hotkey:move-right"),
    ("Ctrl+Shift+9",       "hotkey:opacity-up"),           // Opacity up
    ("Ctrl+Shift+0",       "hotkey:opacity-down"),         // Opacity down
    ("Ctrl+Shift+1",       "hotkey:model-sol"),            // Model selection
    ("Ctrl+Shift+2",       "hotkey:model-terra"),
    ("Ctrl+Shift+3",       "hotkey:model-luna"),
    ("Ctrl+Shift+4",       "hotkey:mode-cycle"),            // Cycle mode lock (dsa/oa/sd/lld/ai-int/ai-ml/cloud/backend/behavioral)
    ("Ctrl+Shift+5",       "hotkey:mode-unlock"),           // Unlock mode (back to auto-detect)
    ("Ctrl+Shift+Backspace","hotkey:clear-session"),       // Clear session + screenshots
    ("Ctrl+Shift+F5",      "hotkey:clear-screenshots"),    // Clear screenshots only
    ("Ctrl+Shift+F1",      "hotkey:show-help"),            // Show hotkey help
    ("Ctrl+Shift+.",       "hotkey:copy-code"),            // Copy code blocks only
    ("Ctrl+Shift+]",       "hotkey:resize-grow"),          // Make overlay bigger
    ("Ctrl+Shift+[",       "hotkey:resize-shrink"),        // Make overlay smaller
    ("Ctrl+Shift+F8",      "hotkey:scroll-up"),            // Scroll answer up
    ("Ctrl+Shift+F9",      "hotkey:scroll-down"),          // Scroll answer down
    ("Ctrl+Shift+6",       "hotkey:compact-mode"),         // Toggle compact/bullet mode
    ("Ctrl+Shift+7",       "hotkey:font-size-cycle"),      // Cycle font size (S/M/L)
    ("Ctrl+Shift+8",       "hotkey:auto-scroll"),          // Toggle auto-scroll (teleprompter)
];

pub fn register_all(app: &AppHandle) -> Result<(), String> {
    let global_shortcut = app.global_shortcut();
    let mut failures: Vec<String> = Vec::new();

    // Hotkeys that support hold-to-repeat (emit both pressed and released)
    const HOLDABLE: &[&str] = &[
        "hotkey:move-up", "hotkey:move-down", "hotkey:move-left", "hotkey:move-right",
        "hotkey:resize-grow", "hotkey:resize-shrink",
        "hotkey:opacity-up", "hotkey:opacity-down",
        "hotkey:scroll-up", "hotkey:scroll-down",
    ];

    for (shortcut_str, event_name) in HOTKEY_EVENTS {
        let shortcut: Shortcut = match shortcut_str.parse() {
            Ok(s) => s,
            Err(e) => {
                failures.push(format!("{shortcut_str}: {e}"));
                continue;
            }
        };

        let event = event_name.to_string();
        let handle = app.clone();
        let is_holdable = HOLDABLE.contains(event_name);

        match global_shortcut
            .on_shortcut(shortcut, move |_app, _shortcut, event_state: ShortcutEvent| {
                if event_state.state == ShortcutState::Pressed {
                    let _ = handle.emit(&event, ());
                } else if is_holdable && event_state.state == ShortcutState::Released {
                    let _ = handle.emit(&format!("{}:released", event), ());
                }
            })
        {
            Ok(_) => {}
            Err(e) => {
                failures.push(format!("{shortcut_str}: {e}"));
                continue;
            }
        }
    }

    if !failures.is_empty() {
        let msg = failures.join(", ");
        let _ = app.emit("hotkey:registration-error", &msg);
    }

    Ok(())
}

pub fn unregister_all(app: &AppHandle) {
    let _ = app.global_shortcut().unregister_all();
}
