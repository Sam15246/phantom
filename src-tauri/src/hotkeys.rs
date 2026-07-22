use tauri::{AppHandle, Emitter};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState};

/// All hotkey events emitted to the frontend
const HOTKEY_EVENTS: &[(&str, &str)] = &[
    ("Ctrl+Shift+Y",       "hotkey:toggle-recording"),
    ("Ctrl+Shift+S",       "hotkey:screenshot"),
    ("Ctrl+Shift+A",       "hotkey:analyze"),
    ("Ctrl+Shift+L",       "hotkey:toggle-click-through"),
    ("Ctrl+Shift+H",       "hotkey:toggle-visibility"),
    ("Ctrl+Shift+N",       "hotkey:toggle-night-mode"),
    ("Ctrl+Shift+W",       "hotkey:snap-to-webcam"),
    ("Ctrl+Shift+Q",       "hotkey:emergency-exit"),
    ("Ctrl+Shift+C",       "hotkey:copy-answer"),
    ("Ctrl+Shift+Up",      "hotkey:move-up"),
    ("Ctrl+Shift+Down",    "hotkey:move-down"),
    ("Ctrl+Shift+Left",    "hotkey:move-left"),
    ("Ctrl+Shift+Right",   "hotkey:move-right"),
    ("Ctrl+Alt+Up",        "hotkey:opacity-up"),
    ("Ctrl+Alt+Down",      "hotkey:opacity-down"),
    ("Ctrl+Shift+1",       "hotkey:model-sol"),
    ("Ctrl+Shift+2",       "hotkey:model-terra"),
    ("Ctrl+Shift+3",       "hotkey:model-luna"),
    ("Ctrl+Shift+Delete",  "hotkey:clear-session"),
];

pub fn register_all(app: &AppHandle) -> Result<(), String> {
    let global_shortcut = app.global_shortcut();

    for (shortcut_str, event_name) in HOTKEY_EVENTS {
        let shortcut: Shortcut = shortcut_str
            .parse()
            .map_err(|e| format!("Failed to parse shortcut '{shortcut_str}': {e}"))?;

        let event = event_name.to_string();
        let handle = app.clone();

        global_shortcut
            .on_shortcut(shortcut, move |_app, _shortcut, event_state: ShortcutEvent| {
                if event_state.state == ShortcutState::Pressed {
                    let _ = handle.emit(&event, ());
                }
            })
            .map_err(|e| format!("Failed to register '{shortcut_str}': {e}"))?;

        println!("Registered hotkey: {shortcut_str} \u{2192} {event_name}");
    }

    Ok(())
}

pub fn unregister_all(app: &AppHandle) {
    let _ = app.global_shortcut().unregister_all();
}
