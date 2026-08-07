# Phantom

A desktop overlay that listens to interview audio, transcribes it, detects the question type, and streams an AI-generated answer in real time. Built with Tauri 2.0 (Rust + vanilla JS). Windows only.

## How It Works

```
Mic/System Audio  -->  Transcribe (OpenAI)  -->  Detect Mode (Groq)  -->  Generate Answer (GPT-5.6)
                                                                              |
Screenshots  -------->  Vision Analysis (GPT-5.6)  --------->  Stream to overlay
```

1. **Record** interview audio (system loopback + mic via WASAPI)
2. **Transcribe** using OpenAI's gpt-transcribe (bilingual EN/HI, keyword hints)
3. **Extract** the question and auto-detect mode via Groq (or skip if mode is locked)
4. **Generate** a streaming answer using the mode-specific prompt and model
5. **Display** in a transparent, always-on-top overlay with markdown rendering

## Modes

The tool auto-detects question type and routes to the right prompt + model:

| Mode | What it handles | Model |
|------|----------------|-------|
| DSA | Algorithms, data structures, live coding | Sol |
| OA | Timed online assessments | Sol |
| AI-Interview | Resume walkthrough, "tell me about yourself" | Sol |
| AI-ML | LLMs, RAG, agents, ML concepts | Sol |
| System Design | HLD, scalability, distributed systems | Terra |
| LLD | Low-level design, OOP, design patterns | Terra |
| Backend | REST, microservices, caching, auth | Terra |
| Java | Spring Boot, JVM, concurrency | Terra |
| Python | FastAPI, Django, async, decorators | Terra |
| DBMS | SQL, indexing, transactions, normalization | Terra |
| Cloud | AWS, K8s, Docker, CI/CD | Terra |
| Behavioral | STAR method, culture fit, situational | Luna |
| General | OS, networking, security, misc CS | Luna |

**Mode Lock:** Press Ctrl+Shift+4 to pre-set a mode before the interview starts. This skips auto-detection and saves 1-2 seconds per question.

## Screenshots

Capture the screen (Ctrl+Shift+F7) and analyze (Ctrl+Shift+F) to solve problems shown on screen. The prompt style adapts to the current mode -- narrated think-aloud for live interviews, fast paste-ready for OAs.

## Hotkeys

All hotkeys use **Ctrl+Shift** prefix (conflict-free with Chrome/Edge):

| Key | Action |
|-----|--------|
| F6 | Record / Stop |
| F7 | Take Screenshot |
| F | Analyze Screenshots |
| F4 | Toggle Click-through |
| H | Hide / Show Overlay |
| Z | Night Mode |
| X | Copy Answer |
| . | Copy Code Blocks Only |
| 4 | Cycle Mode Lock (DSA > OA > SD > LLD > AI-Interview > AI-ML > Auto) |
| 5 | Unlock Mode (Auto-detect) |
| 1 / 2 / 3 | Model: Sol / Terra / Luna |
| 9 / 0 | Opacity Up / Down |
| Arrows | Move Overlay |
| [ / ] | Resize Smaller / Bigger |
| F8 / F9 | Scroll Up / Down |
| F2 | Snap to Webcam |
| F3 | Open Settings |
| F5 | Clear Screenshots |
| Backspace | Clear Session |
| Q | Emergency Exit |
| F1 | Help Overlay |

## Setup

### Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Node.js](https://nodejs.org/) (LTS)
- Windows 10/11

### API Keys

Open settings (Ctrl+Shift+F3) and add:
- **OpenAI API Key** -- for transcription and answer generation
- **Groq API Key** -- for fast question extraction (optional, falls back to basic extraction)

### Build

```bash
npm install
npm run tauri build
```

The release binary is at `src-tauri/target/release/audiodvc.exe` (~11 MB).

### Dev Mode

```bash
npm run tauri dev
```

## Project Structure

```
phantom/
  src/                    # Frontend (vanilla HTML/CSS/JS)
    index.html            # Main overlay UI
    styles.css            # Styling + night mode
    app.js                # Event listeners, markdown rendering
    settings.html         # Settings window
    lib/                  # Vendored JS libs (marked, mermaid, highlight, purify)
  src-tauri/              # Backend (Rust)
    src/
      main.rs             # App setup, hotkey handlers, pipeline orchestration
      api.rs              # All AI calls: transcription, extraction, prompts, streaming
      audio.rs            # WASAPI audio capture (loopback + mic)
      config.rs           # Encrypted settings (AES-256-GCM)
      hotkeys.rs          # Global shortcut registration
      overlay.rs          # Window manipulation (click-through, move, opacity)
      screenshot.rs       # GDI screen capture + queue
    tauri.conf.json       # Tauri config (window, bundle, security)
    Cargo.toml            # Rust dependencies
```

## Stealth

- Process name: `audiodvc.exe` (looks like a Windows audio driver)
- Window title: "Audio Device Properties"
- PE metadata: Microsoft Corporation publisher
- Content protection: `WDA_EXCLUDEFROMCAPTURE` (invisible to screen sharing)
- No browser extension, no network fingerprint
- Settings and sessions encrypted at rest (AES-256-GCM)
- Tray icon only, skip taskbar

## Settings

Configurable via the settings window (Ctrl+Shift+F3):

- OpenAI / Groq API keys
- Audio source (system, mic, or both)
- Resume text (used for ai-interview, behavioral, and domain modes)
- Job description (tailors answers to the target role)
- TTS toggle
