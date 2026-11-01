<p align="center">
  <img src="assets/banner.png" alt="Lodestar banner" />
</p>

**Local-first, real-time interview copilot for your desktop**

Lodestar listens to a call, transcribes both sides with speaker attribution, and answers the other party's questions on the spot, as short spoken lines you can read aloud. It runs on your machine as a discreet overlay: hidden from screen shares, out of the taskbar and Alt+Tab, and driveable from your phone as a second screen.

No account and no backend server. Your data (profiles, stories, saved sessions) lives entirely on your machine. The cloud paths (Claude answers, Deepgram transcription) are opt-in and clearly flagged, and there is a fully-local mode (Ollama + on-device Whisper) that keeps everything on the device.

Built with Tauri v2 (Rust) + React + Vite. The Rust core owns audio capture, screen capture, the model clients, and local storage; the React frontend is the UI.

> Lodestar began as a local-first job-market search tool. As of v0.3.0 it is focused entirely on the live interview copilot; the job-search, tracker, and study features were removed.

## Contents

- [Tech stack](#tech-stack)
- [Install](#install)
- [Prerequisites](#prerequisites)
- [Getting started](#getting-started)
- [Features (built)](#features-built)
- [Using Lodestar](#using-lodestar)
- [Personal use and terms](#personal-use-and-terms)
- [What's not yet built](#whats-not-yet-built)
- [License](#license)

## Tech stack

| Layer           | Technology                                          |
| --------------- | --------------------------------------------------- |
| Desktop         | Tauri v2 (Rust)                                     |
| Frontend        | React 19 + TypeScript + Vite                        |
| Storage         | SQLite (rusqlite, bundled)                          |
| HTTP            | reqwest (rustls, no OpenSSL)                        |
| Async runtime   | tokio                                               |
| Audio capture   | cpal (WASAPI loopback + microphone)                 |
| Local STT       | Candle (pure-Rust Whisper), tokenizers              |
| Cloud STT       | Deepgram (optional)                                 |
| Answer models   | Claude Messages API (streaming) and/or local Ollama |
| Screen capture  | xcap                                                |
| Screen reading  | UI Automation (uiautomation)                        |
| Markdown / math | react-markdown, remark-gfm, remark-math, KaTeX      |
| Icons           | lucide-react                                        |
| Package manager | pnpm                                                |

## Install

Download the latest Windows installer from this private repository's [Releases page](https://github.com/w1ck3ds0d4/Lodestar/releases/latest):

- `Lodestar_<version>_x64-setup.exe` - per-user install, no admin required (recommended)
- `Lodestar_<version>_x64_en-US.msi` - MSI package

The releases are currently unsigned, so on first launch Windows SmartScreen may warn ("Windows protected your PC"); click **More info**, then **Run anyway**. To build from source instead, follow the steps below.

## Prerequisites

- [Rust stable toolchain](https://rustup.rs/)
- [Node.js](https://nodejs.org/) 20+
- [pnpm](https://pnpm.io/installation)
- Windows 10 / 11 with [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) (preinstalled on most modern installs), plus Visual Studio 2022 Build Tools with the "Desktop development with C++" workload. The overlay stealth, loopback capture, and screen reader are Windows-specific.

Optional, for cloud or local answering:

- An Anthropic API key for Claude answers, or [Ollama](https://ollama.com/) running locally (`ollama serve` + a pulled model, e.g. `ollama pull llama3.1`; a vision model such as `llama3.2-vision` for the screen features)
- A Deepgram API key for cloud transcription, or use the local Whisper engine (downloads a model on first run, slower on CPU)

## Getting started

```bash
# 1. Clone and install
git clone https://github.com/w1ck3ds0d4/Lodestar.git
cd Lodestar
pnpm install

# 2. Run the app (boots and creates the local DB on first launch)
pnpm tauri dev

# 3. Build an installer (output under src-tauri/target/release/bundle/)
pnpm tauri build
```

The database is created at the OS-standard app-data directory on first launch.

> Note: a development build (`pnpm tauri dev`) runs an unoptimized Rust binary, so on-device Whisper transcription is much slower than the release installer. Cloud transcription (Deepgram) runs at the same speed either way.

## Features (built)

### The live copilot

- **Live transcription**: captures system audio (the other side of a call, via WASAPI loopback) and/or your microphone, with selectable input/output devices and a live input-level meter for diagnostics
- **Speaker-attributed dialog**: your mic and the call audio are transcribed separately and labeled (You vs Them); on Deepgram, the call side can be diarized into Speaker 1/2/...
- **Auto-answer**: questions from the other party are answered automatically (math, code, definitions, factual lookups), in any language, and **streamed token-by-token** so help appears the moment the model starts
- **Spoken, teleprompter-style answers**: written as a few complete first-person sentences you can read aloud verbatim (a one-line spoken roadmap for multi-part questions, trade-offs stated inline, a closing hand-off), with a dim non-spoken "Next:" follow-up hint, not study-note bullets
- **Streaming answers** rendered as Markdown with fenced code blocks and LaTeX (KaTeX); answers and their extracted code/equation snippets are grouped per question

### Answering

- **Answer engine**: Claude (cloud, default) or a local Ollama model
- **Models**: Claude Opus 5 (default), Sonnet 5, Haiku 4.5, and Fable 5.1, selectable per session; usage and cost estimates use each model's list price
- **Answer style**: Auto, Concise, STAR, or Code (solution + complexity), shapes every live and manual answer
- **Fast mode** (opt-in): runs the same model at up to 2.5x output speed on Opus 5 / Opus 4.8, at premium pricing, for when latency matters
- **Predicted follow-ups**: when an answer predicts the likely next question, it is pre-answered in the background and shown instantly (labeled "Predicted - verifying") until the real answer replaces it. On by default, with a toggle

### Context capture

- **Read full page**: one click reads the whole focused page's text via UI Automation, including content scrolled off-screen, and attaches it as de-duplicated, accumulating context. Works from an in-app button or the phone because it targets the last window you were on, not Lodestar
- **Read page (clipboard)**: a manual fallback for apps that expose no accessible text
- **Screen vision**: Watch / Answer screen / Snip screenshot the display and answer what is visible (on-screen multiple choice, diagrams)

### Grounding and accuracy

- **Interview context**: attach your role, resume, notes, and the company so answers are tailored; generate a company briefing plus questions to ask the interviewer
- **Story bank**: save real STAR stories once (or draft them from notes) so behavioral answers ground in your actual experience
- **Application kit**: tailors resume bullets, a cover letter, an ATS keyword match, and a thank-you note to a pasted job description
- **Overclaim guard** (opt-in): after an answer finishes, it is audited against your saved profile and any claim about your own experience the profile cannot support is flagged under the answer. General technical statements and hypotheticals are never flagged
- **Switchable profiles**: keep several interview personas (role, resume, notes, per-profile stories) and pick which one drives the answers, the screen reader, and the phone

### Review and improvement

- **Delivery coach**: live talk time, speaking pace (wpm), and filler-word count from your own side of the call, with "wrap up" and "slow down" nudges. Entirely local, no API cost
- **Session history**: save a live session's transcript and answers, then run an AI review-and-coach pass
- **Debrief**: ranks the topics that keep going badly across your saved sessions, worst first, so you know what to study. Built from the sessions themselves, cached per session so repeat runs are nearly free
- **Usage and cost** panel: live Deepgram balance, a local Claude token/cost estimate, and entered top-up credits with remaining

### Overlay, stealth, and remote

- **Overlay and stealth**: window opacity and always-on-top for a discreet overlay; a hide-from-screen-capture toggle (excluded from Zoom/Meet/Teams shares and recorders on Windows), with tooltips and dropdowns rendered in-window so they never pop out into a shared frame; the window is kept out of the taskbar, Alt+Tab, and Task Manager's app list via a hidden owner window; a custom borderless title bar; recall it from the system-tray icon, `Alt+Shift+H`, or an `MB5` double-click
- **Global shortcuts**: `Alt+Shift+` hotkeys (listen, answer screen, watch, read page, show/hide panels, opacity, panic-hide) that fire even while the call app is focused, with a Shortcuts cheat-sheet overlay
- **Phone remote**: a LAN web remote (no install) reached by QR / PIN that mirrors the transcript and answers and drives listening, screen-answering, opacity, and panel placement from your phone as a discreet second screen
- **Activity log**: a timestamped record of what each shortcut/action actually did (answered / screen unchanged / page chars attached / ...), as a Settings panel and a floating overlay panel
- **Adaptive layout**: a three-column workspace on wide windows that collapses to a Dialog / Answers tab switcher when narrow, plus a Settings tab that groups set-once configuration (Capture, Answers, Review) out of the live view

### Local-first and privacy

- SQLite (bundled) for profiles, stories, saved sessions, and settings; the database lives in the OS-standard app-data directory
- API keys (Anthropic, Deepgram) are stored only in your local database, encrypted at rest with Windows DPAPI (`secret.rs`)
- The cloud paths send data to their services when used: Claude receives your question/context (and screenshots for the vision features); Deepgram receives captured audio. For a fully on-device setup, use the Ollama answer engine and the local Whisper transcription engine

## Using Lodestar

### 1. Set up audio and a model

In **Settings** (Capture and Answers):

- **Capture source**: `System` (the other side of the call, via WASAPI loopback), `Mic` (you), or `Both`. Pick the **output device** the call app plays to and your **input device**. The two-bar level meter (You / Them) confirms both sides are being captured; a flat "Them" bar usually means the call app is playing to a different output device than the one selected.
- **Transcription engine**: Deepgram (cloud, realtime, per-language, optional speaker diarization) or on-device Whisper (local, no key, slower on CPU).
- **Answer engine**: Claude (cloud, default) or a local Ollama model.
- Add your **Anthropic** and/or **Deepgram** API keys in Settings. Keys are stored in the local database only, encrypted with Windows DPAPI.

### 2. Pick a profile

Open the **person icon** (top bar) to manage **profiles**, switchable interview personas, each with its own role, resume, notes, and STAR stories. The **active profile** grounds behavioral and fit answers in that persona (technical questions stay generic) and drives the screen reader and the phone.

### 3. Answer live

Press **Listen** (or `Alt+Shift+L`). The dialog transcribes with You/Them attribution and per-line timestamps. When the other party asks something, the **auto-answer** streams a short, spoken answer you can read aloud. You can also type a question into **Ask**.

### Keyboard shortcuts

All global (they fire even while your call app / browser is focused), all `Alt+Shift+`:

| Shortcut          | Action                                                           |
| ----------------- | ---------------------------------------------------------------- |
| `Alt+Shift+L`     | Listen on / off                                                  |
| `Alt+Shift+A`     | Answer whatever is on the screen now                             |
| `Alt+Shift+W`     | Watch screen on / off (re-reads and answers it)                  |
| `Alt+Shift+R`     | Read the focused page (full text, incl. off-screen) into context |
| `Alt+Shift+C`     | Show / hide the capture overlay panels                           |
| `Alt+Shift+↑ / ↓` | Opacity up / down                                                |
| `Alt+Shift+H`     | Panic-hide: instantly hide / show the window                     |

The same actions are listed in a **Shortcuts** overlay panel during capture mode, opened from the keyboard icon in the top bar.

### Mouse gestures

Global side-button gestures let you move and recall the overlay without touching the title bar:

| Gesture              | Action                                                       |
| -------------------- | ------------------------------------------------------------ |
| Hold `MB4`           | Peek behind: hide while held, reveal on release              |
| Drag with `MB5` held | Move the window                                              |
| Double-click `MB5`   | Hide / show the window (brings it to the front, keeps focus) |

### Reading the screen vs. the page

- **Watch screen** / **Answer screen**: screenshot the display and answer what is visible (good for on-screen multiple-choice, diagrams). "Watch" re-reads every few seconds and skips unchanged screens.
- **Read full page** (button / phone / `Alt+Shift+R`): reads the _entire_ focused page's text via UI Automation, including content scrolled off-screen, and attaches it as context with de-duplication and accumulation. Use this for long prompts and coding exercises, then Answer/Ask once.
- **Read page (clipboard)**: a manual fallback for apps that expose no accessible text; copy with `Ctrl+A`, `Ctrl+C`, then click.

The **Activity log** shows what each of these actually did, so it is clear which fired and how much it captured.

### Capture overlay & stealth

**Capture mode** opens small always-on-top panels (Answers, Transcript, Controls, Shortcuts, Log) you can drag, resize, and dim anywhere over a call; they are excluded from screen capture/sharing by default. The main window stays out of the taskbar, Alt+Tab, and Task Manager's app list (a hidden owner window keeps it under Background processes). Its tooltips and dropdowns render in-window so nothing pops out of a shared frame, and you bring it back from the system-tray icon, `Alt+Shift+H`, or an `MB5` double-click.

Note that content protection hides the _window_ from capture, but your mouse cursor is drawn by Windows outside any window, so it stays visible in a share. The keyboard shortcuts above drive the main actions without moving the cursor onto the overlay.

### Phone remote (discreet second screen)

In **Settings → Phone control**, start the LAN server and scan the QR (or enter the PIN) on your phone; no app to install. The phone shows the live transcript and answers and gives an 8-tile remote: Listen, Screen Capture, Opacity +/-, Read & Answer (with a monitor chooser for multiple displays), Watch, Panels, Ask, and Read full page. The phone stays on your own Wi-Fi; the server is PIN-gated and scoped to the local subnet.

## Personal use and terms

This build is intended for personal, local use: the data stays on your machine and is not redistributed. The copilot features are study/interview-prep aids; use them in line with the policies of whatever calls or platforms you are on.

## What's not yet built

- Real-time streaming transcription (the current pipeline transcribes in ~4s chunks; cross-chunk speaker numbering is therefore not stable)
- Live web lookup for the company briefing (currently uses the model's own knowledge plus what you enter)
- An auto-updater and code-signed releases (the release installers are currently unsigned)

## License

This project is dual-licensed:

- [AGPL v3](LICENSE) - free for open-source use. Derivatives and SaaS deployments must release their source under AGPL.
- [Commercial license](COMMERCIAL.md) - for proprietary / closed-source use or hosted services that do not want to comply with AGPL source-disclosure requirements. Contact for terms.
