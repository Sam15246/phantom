# Tray Context Menu — Design Spec

**Date:** 2026-08-27
**Scope:** Add a comprehensive right-click context menu to Phantom's system tray icon as a fallback control mechanism when proctoring software blocks global hotkeys.

---

## Hard Constraint

This change must NOT affect the effectiveness, accuracy, speed, or working of any existing functionality. The menu is a parallel input path only — it emits the same events that hotkeys already emit. No modifications to the audio engine, AI pipeline, transcription, answer generation, overlay rendering, or any existing event handler logic.

---

## Problem

Proctoring software (Respondus, OnVUE, PSI) installs `WH_KEYBOARD_LL` hooks that intercept `Ctrl+Shift+*` combinations before they reach Phantom's `RegisterHotKey` handlers. When hotkeys are blocked, the user has no way to control the app — start recording, switch modes, copy answers, or exit.

## Solution

Expand the existing tray icon's right-click menu from a single "Exit Audio Service" item to a full control panel with the 10 essential actions. Menu clicks emit the exact same Tauri events as hotkeys, so all existing handlers work unchanged.

---

## Menu Structure

```
┌──────────────────────────┐
│ Start Recording          │  ← dynamic: "Stop Recording" when active
│ Take Screenshot          │
│ Analyze Screenshots      │
│ ────────────────────────  │
│ Mode: Auto-Detect    →   │──┐
│ ────────────────────────  │  │ Auto-Detect
│ Copy Answer              │  │ DSA
│ Copy Code Blocks         │  │ OA
│ ────────────────────────  │  │ System Design
│ Night Mode: OFF          │  │ LLD
│ Click-Through: ON        │  │ AI-Interview
│ ────────────────────────  │  │ Project Deep-Dive
│ Clear Session            │  │ AI-ML
│ Exit Audio Service       │  │ Cloud
└──────────────────────────┘  │ Backend
                              │ QA
                              │ Behavioral
                              │ Java
                              │ Python
                              │ DBMS
                              └─┘
```

---

## Menu Items

| ID | Default Label | Event Emitted | Dynamic Label |
|----|--------------|---------------|---------------|
| `record` | "Start Recording" | `hotkey:toggle-recording` | "Start Recording" / "Stop Recording" |
| `screenshot` | "Take Screenshot" | `hotkey:screenshot` | Static |
| `analyze` | "Analyze Screenshots" | `hotkey:analyze` | Static |
| `mode-auto` | "Auto-Detect" | Sets locked_mode to None, emits `mode:locked` with "auto" | Static |
| `mode-dsa` | "DSA" | Sets locked_mode to "dsa", emits `mode:locked` | Static |
| `mode-oa` | "OA" | Sets locked_mode to "oa", emits `mode:locked` | Static |
| `mode-system-design` | "System Design" | Sets locked_mode to "system-design", emits `mode:locked` | Static |
| `mode-lld` | "LLD" | Sets locked_mode to "lld", emits `mode:locked` | Static |
| `mode-ai-interview` | "AI-Interview" | Sets locked_mode to "ai-interview", emits `mode:locked` | Static |
| `mode-project-deep-dive` | "Project Deep-Dive" | Sets locked_mode to "project-deep-dive", emits `mode:locked` | Static |
| `mode-ai-ml` | "AI-ML" | Sets locked_mode to "ai-ml", emits `mode:locked` | Static |
| `mode-cloud` | "Cloud" | Sets locked_mode to "cloud", emits `mode:locked` | Static |
| `mode-backend` | "Backend" | Sets locked_mode to "backend", emits `mode:locked` | Static |
| `mode-qa` | "QA" | Sets locked_mode to "qa", emits `mode:locked` | Static |
| `mode-behavioral` | "Behavioral" | Sets locked_mode to "behavioral", emits `mode:locked` | Static |
| `mode-java` | "Java" | Sets locked_mode to "java", emits `mode:locked` | Static |
| `mode-python` | "Python" | Sets locked_mode to "python", emits `mode:locked` | Static |
| `mode-dbms` | "DBMS" | Sets locked_mode to "dbms", emits `mode:locked` | Static |
| `copy-answer` | "Copy Answer" | `hotkey:copy-answer` | Static |
| `copy-code` | "Copy Code Blocks" | `hotkey:copy-code` | Static |
| `night-mode` | "Night Mode: OFF" | `hotkey:toggle-night-mode` | "Night Mode: ON" / "Night Mode: OFF" |
| `click-through` | "Click-Through: ON" | `hotkey:toggle-click-through` | "Click-Through: ON" / "Click-Through: OFF" |
| `clear-session` | "Clear Session" | `hotkey:clear-session` | Static |
| `quit` | "Exit Audio Service" | Same as current quit logic | Static |

---

## Architecture

### Event Reuse (Zero Impact Guarantee)

The menu emits the exact same events that hotkeys emit. The data flow:

```
                    ┌── Hotkey press ──┐
                    │                  │
emit("hotkey:X") ←──┤                  ├──→ Existing backend/frontend handlers
                    │                  │
                    └── Menu click ────┘
```

No existing handler is modified. The menu is additive only.

### State Synchronization

Three dynamic labels need to stay in sync with app state:

1. **Recording label** ("Start" / "Stop")
   - State source: `AudioEngine.is_recording` (existing `Arc<AtomicBool>`)
   - Update trigger: After the existing recording toggle handler runs, call `record_item.set_text()`

2. **Click-through label** ("ON" / "OFF")
   - State source: `ClickThroughState.enabled` (existing `AtomicBool`)
   - Update trigger: After the existing click-through handler runs, call `click_through_item.set_text()`

3. **Night mode label** ("ON" / "OFF")
   - State source: New `AtomicBool` added to a `NightModeState` struct (managed by Tauri)
   - Update trigger: After emitting `hotkey:toggle-night-mode`, flip the bool and call `night_mode_item.set_text()`
   - Reason: Night mode is currently frontend-only (`isNightMode` in app.js). We need a backend-side mirror to know the current state when updating the label.

4. **Mode submenu parent label** ("Mode: DSA", "Mode: Auto-Detect", etc.)
   - State source: `ConversationHistory.locked_mode` (existing `Mutex<Option<String>>`)
   - Update trigger: After mode lock/unlock, call `mode_parent.set_text()`

### MenuItem Handle Storage

A new struct holds references to menu items that need runtime updates:

```rust
struct TrayMenuState {
    record_item: MenuItem,
    night_mode_item: MenuItem,
    click_through_item: MenuItem,
    mode_submenu_item: Submenu,
}
```

Stored as Tauri managed state via `app.manage(TrayMenuState { ... })`.

### Night Mode Backend State

New struct (minimal addition):

```rust
pub struct NightModeState {
    pub enabled: std::sync::atomic::AtomicBool,
}
```

Added to Tauri managed state alongside existing `ClickThroughState`.

---

## Files Changed

| File | Change | Lines Added/Modified |
|------|--------|---------------------|
| `src-tauri/src/main.rs` | Replace tray setup block (lines 810-838) with full menu construction. Add `on_menu_event` handler. Add `TrayMenuState` struct. Add `NightModeState` struct. Update label after recording/click-through/night-mode/mode-lock state changes. | ~120-150 new/modified |

## Files NOT Changed

| File | Reason |
|------|--------|
| `src-tauri/src/api.rs` | AI pipeline untouched |
| `src-tauri/src/audio.rs` | Audio engine untouched |
| `src-tauri/src/overlay.rs` | Window management untouched |
| `src-tauri/src/screenshot.rs` | Screenshot engine untouched |
| `src-tauri/src/config.rs` | Config/encryption untouched |
| `src-tauri/src/hotkeys.rs` | Hotkey registration untouched |
| `src-tauri/src/experience.rs` | Experience data untouched |
| `src/app.js` | Frontend event listeners already handle these events |
| `src/index.html` | No UI changes |
| `src/styles.css` | No style changes |
| `Cargo.toml` | No new dependencies needed — `tauri::menu` already available |

---

## Risk Assessment

| Risk | Mitigation |
|------|-----------|
| Menu event handler introduces bug in existing flow | Menu handler ONLY calls `app.emit()` with existing event names — identical to hotkey path |
| Label desync (menu says "Recording" but it's not) | Labels updated in the same code path as existing state changes, immediately after the state flip |
| Night mode state added to backend | Read-only mirror of frontend state; toggled only on the same event that frontend already handles |
| Tray menu slows startup | Menu construction is ~15 `MenuItem::new()` calls — microseconds, not measurable |
| Existing hotkeys stop working | Hotkey registration in `hotkeys.rs` is completely untouched |

---

## Testing Checklist

- [ ] All 10 menu actions trigger correct behavior (recording, screenshot, analyze, mode switch, copy, night mode, click-through, clear, exit)
- [ ] Dynamic labels update correctly after state changes
- [ ] Mode submenu shows all 15 modes and parent label reflects selection
- [ ] Hotkeys still work exactly as before (regression test)
- [ ] Menu works when overlay is hidden
- [ ] Menu works when click-through is ON
- [ ] Emergency exit (Ctrl+Shift+Q) still works with session encryption
- [ ] Tray icon tooltip unchanged ("Windows Audio Device Manager")
- [ ] Pipeline (record → transcribe → extract → answer) works identically via menu trigger vs hotkey trigger
