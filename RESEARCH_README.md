# Phantom: Real-Time AI Overlay Platform for Proctoring Security Research

A research platform for evaluating the detection capabilities of online proctoring software. Phantom simulates an adversarial overlay system that captures audio, performs real-time transcription and AI-driven analysis, and renders responses in a transparent window — all while employing a suite of evasion techniques. The goal is to systematically identify gaps in proctoring defenses so that vendors and institutions can harden their monitoring pipelines.

> **Disclaimer:** This project is intended strictly for **authorized security research, red-team assessments, and academic study** of online proctoring systems. It is provided as a proof-of-concept to help proctoring vendors, educational institutions, and security researchers understand attack surfaces and improve detection mechanisms. Use responsibly and only in environments where you have explicit authorization.

---

## Research Motivation

Online proctoring has become a critical component of remote education and professional certification. However, the security posture of these systems is rarely evaluated against sophisticated adversarial tooling. Most proctoring vendors test against naive screen-sharing or browser-extension attacks, leaving blind spots around:

- **Kernel-level capture protection** (e.g., `WDA_EXCLUDEFROMCAPTURE`) that makes windows invisible to screen recording
- **Process masquerading** via PE metadata spoofing, benign naming, and priority manipulation
- **System-level audio interception** via WASAPI loopback capture that requires no browser extension
- **Transparent overlay rendering** that sits on top of exam windows without triggering focus-change events
- **Hotkey-based control** that operates through global shortcuts, avoiding detectable UI interactions
- **Tray-based fallback control** when proctoring software intercepts keyboard hooks

This platform serves as a comprehensive red-team tool to test these vectors end-to-end.

---

## Architecture Overview

```
                          +------------------+
                          |   System Tray    |
                          |  (fallback UI)   |
                          +--------+---------+
                                   |
+------------------+    +----------v-----------+    +-------------------+
| Audio Capture    |    |    Pipeline Core     |    | Screenshot Engine |
| (WASAPI loopback +--->+ Transcribe (OpenAI)  +<---+ (GDI capture,    |
|  + mic via cpal) |    | Extract (Groq LLM)   |    |  PNG queue,      |
+------------------+    | Generate (GPT-5.6)   |    |  vision analysis)|
                        +----------+-----------+    +-------------------+
                                   |
                        +----------v-----------+
                        |   Overlay Window     |
                        | (transparent, AOT,   |
                        |  capture-protected,  |
                        |  click-through)      |
                        +----------------------+
```

### System Components

| Layer | Technology | Purpose |
|-------|-----------|---------|
| Runtime | Tauri 2.0 (Rust + JS) | Cross-boundary IPC, window management, system tray |
| Audio | WASAPI via `cpal` | Dual-channel capture (system loopback + microphone) |
| Transcription | OpenAI Whisper/gpt-transcribe | Real-time speech-to-text with bilingual support |
| Question Analysis | Groq LLM (fast inference) | Mode detection and question extraction |
| Answer Generation | GPT-5.6 (Sol/Terra/Luna) | Streaming responses with mode-specific prompting |
| Screen Capture | Win32 GDI (BitBlt) | Multi-monitor screenshot with overflow-safe allocation |
| Encryption | AES-256-GCM | At-rest encryption for config and session data |
| Overlay | Win32 API (unsafe) | Transparent, always-on-top, capture-protected window |

---

## Core Research Areas

### 1. Display Capture Protection

The overlay window uses `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` to become invisible to all standard screen capture APIs (BitBlt, DXGI, Windows.Graphics.Capture). This tests whether proctoring software:

- Detects the presence of protected windows via `GetWindowDisplayAffinity()` enumeration
- Monitors for `SetWindowDisplayAffinity` API calls in real-time
- Uses alternative capture methods that bypass this flag

**Relevant code:** [overlay.rs](src-tauri/src/overlay.rs) — `apply_content_protection()`

### 2. Process Identity Masquerading

The binary mimics a legitimate Windows audio driver service:

- **Process name:** `audiodvc.exe` (resembles Windows audio device controller)
- **PE metadata:** Publisher set to "Microsoft Corporation" via Tauri bundle config
- **Window title:** "Audio Device Properties"
- **Priority:** `BELOW_NORMAL_PRIORITY_CLASS` + Windows Efficiency Mode (green leaf in Task Manager)
- **Task manager visibility:** Hidden from Alt+Tab/Win+Tab via `WS_EX_TOOLWINDOW`
- **Taskbar:** Skipped entirely; only visible as a system tray icon

This tests whether proctoring software:

- Validates PE signatures (not just metadata strings)
- Maintains allowlists vs. blocklists for process names
- Detects priority/efficiency mode manipulation
- Enumerates all windows including `WS_EX_TOOLWINDOW` flagged ones

**Relevant code:** [overlay.rs](src-tauri/src/overlay.rs) — `apply_process_stealth()`, `hide_from_alt_tab()`

### 3. Audio Interception Without Browser Extensions

Audio capture is performed at the OS level via WASAPI loopback, requiring no browser extension or injected code:

- Captures system audio output (what the speakers/headphones play) via the default output device's loopback stream
- Simultaneously captures microphone input via the default input device
- Downmixes multi-channel audio to mono, resamples to a common rate
- Trims silence to reduce transcription latency and cost
- Encodes to WAV in-memory (never writes audio files to disk)

This tests whether proctoring software:

- Monitors `IAudioClient` / WASAPI stream creation
- Detects loopback capture on the default output device
- Scans for audio-processing libraries in loaded modules

**Relevant code:** [audio.rs](src-tauri/src/audio.rs) — `AudioEngine`, `start_recording()`, `build_input_stream_f32()`

### 4. Real-Time AI Pipeline

The core research contribution is a low-latency pipeline that processes live audio into actionable responses:

```
Audio Buffer  -->  Transcribe (~2-4s)  -->  Extract Question + Detect Mode (~0.5s)  -->  Stream Answer (~1-3s first token)
```

**Transcription:** Uses OpenAI's gpt-transcribe model with domain-specific keyword hints (e.g., "microservices", "Kubernetes", "binary search") to improve accuracy on technical vocabulary. Supports bilingual EN/HI transcription.

**Mode Detection:** A Groq-hosted LLM classifies the question into one of 15 categories. When Groq is unavailable, a keyword-based fallback classifier (`fallback_extraction`) routes questions using pattern matching.

**Answer Generation:** Three model tiers are used depending on question complexity:

| Tier | Model | Modes |
|------|-------|-------|
| Sol | gpt-5.6-sol | DSA, OA, AI-Interview, AI-ML, Project Deep-Dive |
| Terra | gpt-5.6-terra | System Design, LLD, DBMS, Cloud, Java, Backend, Python, QA |
| Luna | gpt-5.6-luna | Behavioral, General |

Each mode has a dedicated system prompt engineered for that question type. Responses are streamed token-by-token to minimize perceived latency.

**Relevant code:** [api.rs](src-tauri/src/api.rs) — `transcribe_audio()`, `extract_question()`, `generate_answer_streaming()`, `fallback_extraction()`, `select_model()`

### 5. Hotkey-Based Interaction (Input Hook Evasion)

All controls are bound to global keyboard shortcuts using `Ctrl+Shift+*` combinations, chosen to avoid conflicts with browsers and common applications. This tests whether proctoring software:

- Blocks specific key combinations via `WH_KEYBOARD_LL` hooks
- Detects `RegisterHotKey` / `tauri-plugin-global-shortcut` registrations
- Monitors for unusual global shortcut patterns

| Key | Action | Key | Action |
|-----|--------|-----|--------|
| F6 | Record / Stop | F4 | Toggle Click-Through |
| F7 | Take Screenshot | H | Hide / Show Overlay |
| F | Analyze Screenshots | Z | Night Mode |
| X | Copy Answer | . | Copy Code Blocks |
| 4 | Cycle Mode Lock | 5 | Unlock Mode (Auto) |
| 1 / 2 / 3 | Model: Sol / Terra / Luna | 6 | Toggle Compact Mode |
| 7 | Cycle Font Size | 8 | Toggle Auto-Scroll |
| 9 / 0 | Opacity Up / Down | Arrows | Move Overlay |
| [ / ] | Resize Smaller / Bigger | F8 / F9 | Scroll Up / Down |
| F2 | Snap to Webcam Corner | F3 | Open Settings |
| F5 | Clear Screenshots | Backspace | Clear Session |
| Q | Emergency Exit | F1 | Help Overlay |

**Relevant code:** [hotkeys.rs](src-tauri/src/hotkeys.rs) — `HOTKEY_EVENTS`, `register_all()`

### 6. Tray Menu as Fallback Control Channel

When proctoring software installs `WH_KEYBOARD_LL` hooks that intercept global shortcuts, the system tray icon provides an alternative control path via right-click context menu:

- Start/Stop Recording
- Take Screenshot / Analyze Screenshots
- Mode selection (submenu with all 15 modes + Auto-Detect)
- Copy Answer / Copy Code Blocks
- Night Mode / Click-Through toggle
- Clear Session / Exit

The tray menu emits the same Tauri events as hotkeys (`hotkey:toggle-recording`, `hotkey:screenshot`, etc.), making it a parallel input path with zero duplication in handler logic. Menu labels update dynamically to reflect current state.

This tests whether proctoring software:

- Monitors `Shell_NotifyIcon` / tray icon registration
- Intercepts `WM_CONTEXTMENU` on tray icons
- Detects unusual system tray entries

**Relevant code:** [main.rs](src-tauri/src/main.rs) — `TrayMenuState`, `on_menu_event`

### 7. Screenshot-Based Problem Solving

The GDI capture engine takes full-screen screenshots, queues them (up to 5), and submits them to a vision model for analysis. The prompt adapts to the current mode:

- **Live interview context** (DSA, System Design, etc.): Narrated think-aloud format, as if the candidate is explaining their thought process
- **Online assessment context** (OA): Direct, paste-ready code solutions optimized for speed

Buffer allocation uses `checked_mul` for overflow-safe computation, preventing heap corruption on extreme multi-monitor virtual desktop configurations.

**Relevant code:** [screenshot.rs](src-tauri/src/screenshot.rs) — `capture_screen()`, `ScreenshotQueue`

---

## Evasion Techniques Inventory

The following techniques are implemented and can be individually toggled or studied:

| Technique | API / Mechanism | Detection Vector |
|-----------|----------------|-----------------|
| Capture exclusion | `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` | `GetWindowDisplayAffinity()` enumeration |
| Alt+Tab hiding | `WS_EX_TOOLWINDOW` extended style | `EnumWindows()` with style filtering |
| Click-through | `WS_EX_TRANSPARENT + WS_EX_LAYERED` | Window style enumeration |
| No-activate | `WS_EX_NOACTIVATE` | Focus event monitoring |
| Process name spoof | Binary named `audiodvc.exe` | Signature verification, hash-based blocklist |
| PE metadata spoof | "Microsoft Corporation" publisher | Authenticode signature validation |
| Priority stealth | `BELOW_NORMAL_PRIORITY_CLASS` + efficiency mode | Process priority enumeration |
| Taskbar hiding | `skipTaskbar: true` in Tauri config | Taskbar entry enumeration |
| Config encryption | AES-256-GCM with machine-derived key | Filesystem forensics |
| In-memory audio | WAV encoded in RAM, never written to disk | Temp file monitoring (not applicable) |
| OS-level audio tap | WASAPI loopback (no browser extension) | Audio stream enumeration |
| Tray-only UI | No visible window in taskbar/Alt+Tab | Tray icon enumeration |

---

## Project Structure

```
phantom/
  src/                          # Frontend (vanilla HTML/CSS/JS)
    index.html                  # Overlay window — status bar, answer box, hotkey help
    styles.css                  # Theming (normal/night), layout, font sizes, compact mode
    app.js                      # Event handling, markdown rendering, streaming, UI state
    settings.html               # Settings window — API keys, audio source, resume/JD upload
    lib/                        # Vendored client-side libraries
      marked.min.js             #   Markdown parser
      highlight.min.js          #   Syntax highlighting
      purify.min.js             #   XSS sanitization (DOMPurify)
      mermaid.min.js            #   Diagram rendering

  src-tauri/                    # Backend (Rust)
    src/
      main.rs                   # App setup, state management, pipeline orchestration,
                                #   event wiring, tray menu, concurrency guard (RAII)
      api.rs                    # AI API integration — transcription, extraction, streaming
                                #   answer generation, vision analysis, TTS, 15 mode prompts
      audio.rs                  # WASAPI audio capture — loopback + mic, downmix, trim,
                                #   WAV encoding, dual-stream management
      config.rs                 # AES-256-GCM encrypted settings, config cache,
                                #   machine-derived key, PDF parsing for resume/JD
      hotkeys.rs                # Global shortcut registration (30+ bindings),
                                #   hold-to-repeat for continuous actions
      overlay.rs                # Win32 window manipulation — click-through, capture
                                #   protection, Alt+Tab hiding, process stealth, move/resize
      screenshot.rs             # GDI screen capture, overflow-safe buffer allocation,
                                #   PNG encoding, screenshot queue (cap 5)
      experience.rs             # Embedded professional context for AI prompts

    Cargo.toml                  # Rust dependencies
    tauri.conf.json             # Window config, bundle metadata, CSP policy
    icons/                      # Application icons (32x32, 128x128, ico, icns)

  test_phantom.py               # Integration test harness
  README.md                     # Quick-start guide
  RESEARCH_README.md            # This file
```

---

## Dependencies

### Rust (Backend)

| Crate | Version | Purpose |
|-------|---------|---------|
| `tauri` | 2.x | Application framework, IPC, window management, tray |
| `cpal` | 0.15 | Cross-platform audio I/O (WASAPI on Windows) |
| `hound` | 3.5 | WAV file encoding/decoding |
| `reqwest` | 0.12 | HTTP client for API calls (with streaming) |
| `tokio` | 1.x | Async runtime |
| `futures-util` | 0.3 | Stream combinators for chunked responses |
| `aes-gcm` | 0.10 | AES-256-GCM authenticated encryption |
| `sha2` | 0.10 | SHA-256 for key derivation |
| `rand` | 0.8 | Cryptographic random nonce generation |
| `base64` | 0.22 | Base64 encoding for API payloads |
| `image` | 0.25 | PNG encoding for screenshots |
| `pdf-extract` | 0.8 | PDF text extraction for resume/JD upload |
| `windows-sys` | 0.59 | Win32 API bindings (GDI, window management) |
| `serde` / `serde_json` | 1.x | Serialization for config and API responses |
| `chrono` | 0.4 | Timestamps for session management |
| `dirs` | 5.x | Platform-specific config directory resolution |

### JavaScript (Frontend)

| Library | Purpose |
|---------|---------|
| `marked` | Markdown-to-HTML rendering |
| `highlight.js` | Syntax highlighting in code blocks |
| `DOMPurify` | XSS sanitization of rendered content |
| `mermaid` | Diagram rendering (flowcharts, sequence diagrams) |

---

## Setup & Build

### Prerequisites

- [Rust](https://rustup.rs/) (stable toolchain)
- [Node.js](https://nodejs.org/) (LTS)
- Windows 10/11 (required for WASAPI and Win32 APIs)

### API Keys

The platform requires two API keys (configured via the settings window):

1. **OpenAI API Key** — Used for audio transcription (gpt-transcribe) and answer generation (GPT-5.6)
2. **Groq API Key** (optional) — Used for fast question extraction and mode detection. Falls back to keyword-based classification if unavailable.

### Build

```bash
cd phantom
npm install
npm run tauri build
```

The release binary is output to `src-tauri/target/release/audiodvc.exe` (~13 MB, stripped + LTO optimized).

### Development Mode

```bash
npm run tauri dev
```

Hot-reloads frontend changes. Rust changes require a rebuild.

---

## Concurrency & Safety

### Pipeline Concurrency Guard

Only one AI pipeline (voice or screenshot) can run at a time. This is enforced via an atomic `pipeline_running` flag wrapped in a RAII guard (`PipelineGuard`):

```rust
struct PipelineGuard { app: AppHandle }

impl PipelineGuard {
    fn try_acquire(app: &AppHandle) -> Option<Self> {
        // compare_exchange(false, true, SeqCst, SeqCst)
    }
}

impl Drop for PipelineGuard {
    fn drop(&mut self) {
        // .store(false, SeqCst) — auto-releases on any exit path
    }
}
```

This prevents the "stuck pipeline" bug where a manual `.store(false)` on an early-return path could be forgotten, permanently locking the pipeline until restart.

### Poisoned Mutex Recovery

All `Mutex::lock()` calls use the poison-tolerant pattern:

```rust
mutex.lock().unwrap_or_else(|e| e.into_inner())
```

The data behind each mutex (audio buffers, screenshot queue, conversation history) carries no cross-field invariant, making recovery from a poisoned state safe. This prevents a single unrelated panic from permanently breaking unrelated features.

### Memory Safety

Screenshot buffer allocation uses `checked_mul` to prevent integer overflow on extreme multi-monitor configurations:

```rust
let pixel_count = (width as usize).checked_mul(height as usize).ok_or("overflow")?;
let buf_size = pixel_count.checked_mul(4).ok_or("overflow")?;
```

---

## Testing

### Unit Tests

```bash
cd src-tauri
cargo test
```

**Test coverage includes:**

- **Audio DSP** (`audio::tests`) — `mix_audio` (empty passthrough, averaging, unequal lengths), `trim_silence` (empty, all-quiet, loud preservation), `downsample` (identity, 2x decimation), `encode_wav` (round-trip, clamping)
- **Config Encryption** (`config::tests`) — AES-256-GCM round-trip, nonce uniqueness, tamper rejection, truncated input rejection, `PhantomConfig` serde defaults
- **Mode Classification** (`api::tests`) — `fallback_extraction` keyword mapping for all major categories (DSA, behavioral, system-design, ai-interview, skip, general), `select_model` routing for all 15 modes to correct model tier

All tests are behind `#[cfg(test)]` and do not affect the release binary.

### Manual Verification Checklist

1. `cargo build` compiles without errors
2. `cargo test` — all unit tests green
3. `grep -R "\.lock()\.map_err(" src` — returns nothing (consistent poison policy)
4. Smoke test: record + analyze produces streamed answer
5. Smoke test: trigger two pipelines back-to-back; second reports "still running", subsequent trigger works normally
6. Smoke test: with mic disabled and `audio_source = "both"`, warning names the failing source's reason

---

## Known Detection Vectors (Open Research Questions)

These are areas where current proctoring software can or may detect the platform, representing opportunities for further research:

| Vector | Status | Details |
|--------|--------|---------|
| `GetWindowDisplayAffinity()` enumeration | **Detectable** | Aiseptor and similar tools enumerate all HWNDs and check for `WDA_EXCLUDEFROMCAPTURE`. Mitigation: research alternative capture protection methods. |
| Network traffic analysis (SNI/DNS) | **Partially detectable** | Direct API calls to `api.openai.com` / `api.groq.com` are visible to network monitoring. Mitigation: proxy relay through benign-looking endpoints. |
| Audio watermarking | **Research needed** | Some proctoring systems (e.g., Honorlock) embed inaudible audio watermarks. If transcription captures these, it could fingerprint the tool's presence. |
| Response latency uniformity | **Research needed** | AI-generated answers have characteristic timing patterns (consistent first-token latency). This is considered the strongest AI-detection signal. |
| Gaze tracking | **Not addressed** | Webcam-based gaze analysis can detect when the candidate is reading from an overlay positioned outside the exam window. |
| Process signature verification | **Partially mitigated** | PE metadata is spoofed, but Authenticode signature is not valid. Advanced process scanners will flag unsigned binaries claiming to be Microsoft. |
| WASAPI stream enumeration | **Research needed** | Whether proctoring software monitors for unexpected audio loopback streams. |
| Input hook detection | **Mitigated via tray** | `WH_KEYBOARD_LL` hooks can block hotkeys; tray menu provides fallback. Detection of tray icon registration itself is unexplored. |

---

## Contributing

This is a research platform. Contributions that expand detection coverage, improve evasion techniques for testing purposes, or add new proctoring vendor analysis are welcome. When contributing:

1. **No behavioral changes** to existing functionality without explicit justification
2. **All new code** must include unit tests where applicable
3. **Document detection vectors** — for every evasion technique added, document the corresponding detection method
4. **Follow existing patterns** — poison-tolerant mutexes, RAII guards, event-based IPC

---

## License

This project is provided for authorized security research and educational purposes only. See the disclaimer at the top of this document.
