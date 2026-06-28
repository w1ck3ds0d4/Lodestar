<p align="center">
  <img src="assets/banner.svg" alt="JobRadar - Local-first job-market search for your desktop" width="100%" />
</p>

**Local-first job search and a real-time career copilot for your desktop**

JobRadar started as a local-first job-market search tool and has grown into a full job-hunt and interview workstation. It polls a set of job sources on a schedule, normalizes and de-duplicates the postings into a local SQLite database, and lets you full-text search, filter, and get desktop notifications about new matches. On top of that it adds an application/program **Tracker**, a **Study** planner for the skills you are building, and an **Assistant** copilot that listens to a call, transcribes it with speaker attribution, and answers questions on the spot.

No account and no backend server. Job-search data lives entirely on your machine. The Assistant's cloud features (Claude answers, Deepgram transcription) are opt-in and clearly flagged, and there is a fully-local mode (Ollama + on-device Whisper) that keeps everything on the machine.

Built with Tauri v2 (Rust) + React + Vite. The Rust core owns ingestion, storage (SQLite + FTS5), search, audio capture, screen capture, and the model clients; the React frontend is the UI.

---

## Features (Built)

### Search & Filtering

- Full-text search over title, company, location, and description (SQLite FTS5, BM25 ranking, prefix matching)
- Debounced search-as-you-type, empty query falls back to most-recently-seen first
- **Workplace filter**: Remote, Hybrid, On-site (classified per posting from source signals plus location/title keywords)
- **Experience-level filter**: Entry / Junior, Mid, Senior. Uses the source's own seniority field when it provides one (e.g. Jobicy's `jobLevel`), otherwise infers it from the title and description
- **Remote-fit filter**: "Workable from <your location>" keeps remote roles that are worldwide / your region / unstated and hides ones locked to another region. Your country is detected once via IP geolocation and is overridable
- **Pays-relocation filter** for postings that mention relocation help
- Region and relocation badges on each card, plus salary / employment type / posted date when the source provides them
- Source filter chips, one-click "Clear all", live job + company counts

### Sources

- Nine adapters behind a single source interface: adding one is a module plus a match arm
- Keyless: Greenhouse, Lever, Ashby (public ATS boards), Arbeitnow, Remotive, Jobicy (remote-job feeds), and a generic `JobPosting` JSON-LD scraper for any careers page
- Credential-gated (free key): Adzuna and USAJOBS
- Per-source config, enable/disable toggle, and editable polling interval in the Sources drawer

### Ingestion

- Background tokio scheduler: syncs each due source on its own polite interval, then polls
- Network I/O runs outside the database lock; writes are quick and transactional
- Idempotent upserts keyed on `(source, external_id)`, content-hash for cross-source de-duplication
- Stale-job sweep: postings that drop out of a source's latest crawl are soft-deleted
- "Sync now" button for an immediate pull of every enabled source

### Alerts

- Saved searches that re-run after every ingest cycle
- Native desktop notification when new matching jobs arrive (counts only jobs first seen after the alert was created)

### Tracker

- A drag-and-drop pipeline board for applications (saved -> applied -> screen -> interview -> offer -> accepted/rejected), with pipeline stats and deadline highlighting (overdue / due soon)
- One-click "Track" from a job card; notes, deadlines, per-item milestones, and a "next action" reminder
- Per-application **AI interview prep**: likely questions, talking points, and a research checklist for the role + company
- A small catalog of remote marketplaces and apprenticeships you can add in one click

### Study

- Track the skills you are building: a goal, a self-rated level, and logged study hours per skill
- AI-generated study plans: from a skill plus your role/context, the Assistant produces a roadmap, milestones, and resources
- **Mock interview**: an AI interviewer asks role/company-tailored questions one at a time and scores your answer on content (score, what worked, what to improve, model answer) and delivery (word and filler-word counts); optional text-to-speech reads questions aloud

### Assistant (interview / study copilot)

- **Live transcription**: captures system audio (the other side of a call, via WASAPI loopback) and/or your microphone, with selectable input/output devices and a live input-level meter for diagnostics
- **Speaker-attributed chat**: your mic and the call audio are transcribed separately and labeled (You vs Them); on Deepgram, the call side is diarized into Speaker 1/2/...
- **Auto-answer**: detected questions/problems from the other party are answered automatically (math, code, definitions, factual lookups), in any language, and **streamed token-by-token** so help appears the moment the model starts
- **Streaming answers** rendered as Markdown with fenced code blocks and LaTeX (KaTeX); answers and their extracted code/equation snippets are grouped per question
- **Transcription engine**: Deepgram realtime streaming (cloud) with a per-language setting and an optional "separate speakers" (diarization) toggle, or fully on-device Whisper (local); **Answer engine**: Claude (cloud, default) or a local Ollama model
- **Answer style**: Auto, Concise (glanceable bullets), STAR, or Code (solution + complexity) - shapes every live and manual answer
- **Context capture**: **Read full page** reads the whole focused page (including off-screen content) via UI Automation and attaches it as context; **Read page (clipboard)** is a manual fallback for apps that expose no accessible text; plus screen "Watch" / "Answer screen" / "Snip" vision
- **Story bank**: save real STAR stories once (or draft them from notes) so behavioral answers ground in your actual experience; an **Application kit** tailors resume bullets, a cover letter, an ATS keyword match, and a thank-you note to a pasted job description
- **Session history**: save a live session's transcript + answers, then run an AI "review & coach" pass that surfaces weak areas and a prioritized study list
- **Usage & cost** panel: live Deepgram balance, a local Claude token/cost estimate, and entered top-up credits with remaining
- **Interview context**: attach your role, resume, notes, and the company, so answers are tailored; generate a company briefing plus questions to ask the interviewer
- **Spoken, teleprompter-style answers**: live answers are written as a few complete first-person sentences you can read aloud verbatim (a one-line spoken roadmap for multi-part questions, trade-offs stated inline, a closing hand-off), with optional depth and a dim non-spoken "Next:" follow-up hint - not study-note bullets
- **Read full page**: one click reads the whole focused page (including content scrolled off-screen) via UI Automation and attaches it as de-duplicated, accumulating context; works from an in-app button or the phone because it targets the last window you were on, not JobRadar
- **Global shortcuts**: `Alt+Shift+` hotkeys (listen, answer screen, watch, read page, show/hide panels, opacity, panic-hide) that fire even while the call app is focused, with a Shortcuts cheat-sheet overlay
- **Activity log**: a timestamped record of what each shortcut/action actually did (answered / screen unchanged / page chars attached / ...), as a main-window panel and a floating overlay panel
- **Switchable profiles**: keep several interview personas (role, resume, notes, per-profile stories and job-search filters) and pick which one drives the answers, the screen reader, the phone, and resume matching
- **Phone remote**: a LAN web remote (no install) reached by QR / PIN that mirrors the transcript and answers and drives listening, screen-answering, opacity, and panel placement from your phone as a discreet second screen
- **Overlay & stealth**: window opacity and always-on-top for a discreet overlay; a hide-from-screen-capture toggle (excluded from Zoom/Meet/Teams shares and recorders on Windows); a custom borderless title bar; an optional "Hide from taskbar & Alt+Tab" mode with a system-tray icon and `Alt+Shift+H` panic-hide to bring the window back
- **Adaptive layout**: a three-column copilot workspace on wide windows that collapses to a Setup / Dialog / Answers tab switcher when narrow

### Local-first and privacy

- SQLite (bundled) with an FTS5 external-content index; the database lives in the OS-standard app-data directory
- The job-search side makes no outbound calls except to the sources you enable
- API keys (Anthropic, Deepgram) are stored only in your local database
- The Assistant's cloud paths send data to their services when used: Claude receives your question/context (and screenshots for the vision features); Deepgram receives captured audio. For a fully on-device setup, use the Ollama answer engine and the local Whisper transcription engine
- "Open" launches a posting in your default browser via the system opener

---

## Tech Stack

| Layer           | Technology                                          |
| --------------- | --------------------------------------------------- |
| Desktop         | Tauri v2 (Rust)                                     |
| Frontend        | React 19 + TypeScript + Vite                        |
| Storage         | SQLite (rusqlite, bundled) with FTS5                |
| Search          | FTS5 full-text, BM25 ranking                        |
| HTTP            | reqwest (rustls, no OpenSSL)                        |
| Async runtime   | tokio                                               |
| Audio capture   | cpal (WASAPI loopback + microphone)                 |
| Local STT       | Candle (pure-Rust Whisper), tokenizers              |
| Cloud STT       | Deepgram (optional)                                 |
| Answer models   | Claude Messages API (streaming) and/or local Ollama |
| Screen capture  | xcap                                                |
| Markdown / math | react-markdown, remark-gfm, remark-math, KaTeX      |
| Notifications   | tauri-plugin-notification                           |
| Icons           | lucide-react                                        |
| Package manager | pnpm                                                |

---

## Prerequisites

- [Rust stable toolchain](https://rustup.rs/)
- [Node.js](https://nodejs.org/) 20+
- [pnpm](https://pnpm.io/installation)
- Windows 10 / 11 with [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) (preinstalled on most modern installs), plus Visual Studio 2022 Build Tools with the "Desktop development with C++" workload. macOS and Linux builds work with the standard Tauri toolchain (the window-opacity overlay and loopback capture are Windows-specific).

Optional, for the Assistant:

- An Anthropic API key for Claude answers, or [Ollama](https://ollama.com/) running locally (`ollama serve` + a pulled model, e.g. `ollama pull llama3.1`; a vision model such as `llama3.2-vision` for the screen features)
- A Deepgram API key for cloud transcription, or use the local Whisper engine (downloads a model on first run, slower on CPU)

---

## Getting Started

```bash
# 1. Clone and install
git clone https://github.com/w1ck3ds0d4/JobRadar.git
cd JobRadar
pnpm install

# 2. Run the app (boots, creates the local DB, syncs enabled sources within a few seconds)
pnpm tauri dev

# 3. Build an installer (output under src-tauri/target/release/bundle/)
pnpm tauri build
```

The database is created at the OS-standard app-data directory on first launch. Enabled keyless sources sync automatically; the scheduler then re-polls each source on its configured interval.

> Note: a development build (`pnpm tauri dev`) runs an unoptimized Rust binary, so on-device Whisper transcription is much slower than the release installer. Cloud transcription (Deepgram) runs at the same speed either way.

---

## Sources

Keyless (work out of the box):

| Source                | Kind         | Notes                                                            |
| --------------------- | ------------ | ---------------------------------------------------------------- |
| Greenhouse            | `greenhouse` | Public ATS board per company. Config: `{ "company": "stripe" }`. |
| Lever                 | `lever`      | Public ATS board per company. Config: `{ "company": "..." }`.    |
| Ashby                 | `ashby`      | Public job board per company. Honors the `isListed` flag.        |
| Arbeitnow             | `arbeitnow`  | Global / EU + remote feed. No config.                            |
| Remotive              | `remotive`   | Remote-jobs feed. Polled politely (6h), attributed to Remotive.  |
| Jobicy                | `jobicy`     | Remote-jobs feed. Polled hourly, attributed to Jobicy.           |
| Career page (JSON-LD) | `jsonld`     | Parses schema.org `JobPosting` markup from any careers URL.      |

Credential-gated (add your own free key in the Sources drawer):

| Source  | Kind      | Config                                                |
| ------- | --------- | ----------------------------------------------------- |
| Adzuna  | `adzuna`  | `{ "app_id", "app_key", "country", "what", "where" }` |
| USAJOBS | `usajobs` | `{ "email", "api_key", "keyword" }`                   |

---

## Using Assistant mode

Assistant mode turns JobRadar into a live interview/study copilot: it listens to a call, transcribes both sides, and answers the other party's questions on the spot. Everything below is optional and lives under the **Assistant** tab.

### 1. Set up audio and a model

In **Setup** (the gear/sliders on the Assistant tab):

- **Capture source**: `System` (the other side of the call, via WASAPI loopback), `Mic` (you), or `Both`. Pick the **output device** the call app plays to and your **input device**. The two-bar level meter (You / Them) confirms both sides are being captured - a flat "Them" bar usually means the call app is playing to a different output device than the one selected.
- **Transcription engine**: Deepgram (cloud, realtime, per-language, optional speaker diarization) or on-device Whisper (local, no key, slower on CPU).
- **Answer engine**: Claude (cloud, default) or a local Ollama model.
- Add your **Anthropic** and/or **Deepgram** API keys in Setup. Keys are stored in the local database only.

### 2. Pick a profile

Open the **person icon** (top bar) to manage **profiles** - switchable interview personas, each with its own role, resume, notes, STAR stories, and job-search filters. The **active profile** drives the live answers (behavioral/fit questions are grounded in that persona; technical questions stay generic), the screen reader, the phone, and resume matching. Select the active profile from the Assistant view.

### 3. Answer live

Press **Listen** (or `Alt+Shift+L`). The dialog transcribes with You/Them attribution and per-line timestamps. When the other party asks something, the **auto-answer** streams a short, spoken answer you can read aloud. Answers are deliberately teleprompter-shaped: a one-line roadmap for multi-part questions, trade-offs inline, a closing hand-off, plus a dim "Next:" hint that is metadata only. You can also type a question into **Ask**.

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

The same actions are listed in a **Shortcuts** overlay panel during capture mode.

### Reading the screen vs. the page

- **Watch screen** / **Answer screen**: screenshot the display and answer what is visible (good for on-screen multiple-choice, diagrams). "Watch" re-reads every few seconds and skips unchanged screens.
- **Read full page** (button / phone / `Alt+Shift+R`): reads the _entire_ focused page's text via UI Automation - including content scrolled off-screen - and attaches it as context with de-duplication and accumulation, so it never duplicates answers or loses context. Use this for long prompts and coding exercises, then Answer/Ask once.
- **Read page (clipboard)**: a manual fallback for apps that expose no accessible text - copy with `Ctrl+A`, `Ctrl+C`, then click.

The **Activity log** (a Setup panel and a floating overlay panel) shows what each of these actually did, so it is clear which fired and how much it captured.

### Capture overlay & stealth

**Capture mode** opens small always-on-top panels (Answers, Transcript, Controls, Shortcuts, Log) you can drag, resize, and dim anywhere over a call; they are excluded from screen capture/sharing by default. For discretion, **Overlay → Setup** also offers opacity, always-on-top, and an optional **Hide from taskbar & Alt+Tab** mode (the window keeps a normal frame via a custom title bar, and you bring it back from the system-tray icon or `Alt+Shift+H`).

### Phone remote (discreet second screen)

In **Setup → Phone control**, start the LAN server and scan the QR (or enter the PIN) on your phone - no app to install. The phone shows the live transcript and answers and gives an 8-tile remote: Listen, Screen Capture, Opacity +/-, Read & Answer (with a monitor chooser when you have more than one display), Watch, Panels, Ask, and Read full page. The **Panels** sheet shows/hides the overlay and moves a chosen panel (or all of them) by snapping it to a corner/center or nudging it, so you can rearrange the overlay from your phone. The phone stays on your own Wi-Fi; the server is PIN-gated and scoped to the local subnet.

---

## Personal Use & Terms

This build is intended for personal, local use: the data stays on your machine and is not redistributed. The ingestion scheduler honors each source's polite polling interval, and the adapters carry the source attribution (Remotive, Jobicy, Adzuna) and listing flags (Ashby) those sources ask for.

If you ever turn this into a public, multi-user website, the redistribution terms of these sources change materially. Several feeds permit personal display with attribution but forbid re-aggregation, and the credentialed APIs gate commercial / public use behind a license. Check each source's Terms of Service before publishing. The Assistant features are study/interview-prep aids; use them in line with the policies of whatever calls or platforms you are on.

---

## What's Not Yet Built

- Real-time streaming transcription (the current pipeline transcribes in ~4s chunks; cross-chunk speaker numbering is therefore not stable)
- Live web lookup for the company briefing (currently uses the model's own knowledge plus what you enter)
- More credential-gated job sources (Reed, The Muse, Jooble, Careerjet)
- Saved/exported searches and CSV/JSON export of results
- Auto-updater and signed release artifacts

---

## License

This project is dual-licensed:

- [AGPL v3](LICENSE) - free for open-source use. Derivatives and SaaS deployments must release their source under AGPL.
- [Commercial license](COMMERCIAL.md) - for proprietary / closed-source use or hosted services that do not want to comply with AGPL source-disclosure requirements. Contact for terms.
