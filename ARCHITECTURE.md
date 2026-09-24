# Architecture

Lodestar is a Tauri v2 desktop app. A Rust backend owns audio capture, screen capture, model
calls, and local storage; a React + TypeScript + Vite frontend renders the overlay UI and the
phone companion page.

## Tech stack

- Backend: Rust (edition 2021), Tauri 2, `rusqlite` (bundled SQLite), `reqwest`
- Speech: WASAPI loopback + microphone capture, Deepgram (cloud, diarized) or local Whisper
  via Candle (pure Rust, no external tools)
- Answers: Claude (Anthropic Messages API over `reqwest`, no official Rust SDK exists) or a
  local Ollama server
- Screen: `xcap` for monitor capture, sent to Claude vision for on-screen question answering
- Secrets: Windows DPAPI (`secret.rs`), scoped to the current user account
- Frontend: React, TypeScript, Vite, `react-markdown` + KaTeX for streamed answers,
  `lucide-react` icons

## Component breakdown

Rust backend (`src-tauri/src/`):

- `lib.rs`: app entry, shared `AppState` (db handle, HTTP client, listening flag, phone
  server handle), window-state persistence, Tauri command registration.
- `assistant.rs`: the answer engine. One interface, two backends (Claude, local Ollama).
- `listen.rs`: system-audio (loopback) and/or microphone capture and transcription for the
  live assistant; the largest module, since it drives the main transcribe-then-answer loop.
- `local_stt.rs`: on-device Whisper via Candle, the fully-local alternative to Deepgram.
- `capture.rs`: the Capture overlay: a control bar plus frameless answer/transcript/controls
  panels the user drags over a video call.
- `screen.rs`: monitor enumeration and PNG capture for the visual assistant.
- `career.rs`: per-profile data, interview profiles, STAR story bank, saved sessions.
- `commands.rs`: the Tauri command surface exposed to the frontend via `invoke`.
- `db.rs`: SQLite schema and the key-value settings table.
- `secret.rs`: DPAPI-protected at-rest storage for API keys, with tolerant fallback for
  legacy plaintext values (re-encrypted on next write).
- `usage.rs`: local token tally from Claude's per-response usage reporting, since the API
  exposes no balance endpoint.
- `phone.rs` / `netcheck.rs`: the LAN phone-companion server and its Windows Firewall
  pre-flight check.
- `window_fx.rs`, `winauto.rs`, `mouse_hook.rs`, `gpu_tray.rs`: overlay behavior (always-on-top,
  translucency), focused-window text capture via UI Automation, global mouse-button gestures,
  and a GPU-temperature tray readout.
- `model.rs`: shared data types (stories, sessions) between storage, adapters and UI.
- `netcheck.rs`, `util.rs`: small supporting helpers.

Frontend (`src/`):

- `main.tsx`, `App.tsx`, `styles.css`: shell and view routing.
- `api.ts`: typed wrappers around the Tauri `invoke` commands.
- `types.ts`: shared frontend types mirroring `model.rs`.
- `components/`, `lib/`: UI components and frontend-only helpers.

## Data flow

1. The user starts a session. `listen.rs` opens WASAPI loopback and/or the microphone and
   streams audio to Deepgram or the local Whisper model in `local_stt.rs`.
2. Transcribed lines are attributed (You vs Them) and pushed to the frontend.
3. When the other party's line looks like a question, `assistant.rs` sends it (with recent
   context) to Claude or a local Ollama model and streams the answer back token by token.
4. The frontend renders the streaming answer as Markdown with fenced code and LaTeX, and
   groups it with any extracted code/equation snippets.
5. If the user triggers screen capture, `screen.rs` grabs the active monitor and sends it to
   Claude vision through `assistant.rs` to answer whatever is shown.
6. All settings, profiles, stories and saved sessions persist through `db.rs`; API keys are
   read and written through `secret.rs`.
7. The optional phone companion (`phone.rs`) mirrors the live transcript/answers to a phone
   on the same LAN and can drive start/stop and ask actions back into the same command layer.
