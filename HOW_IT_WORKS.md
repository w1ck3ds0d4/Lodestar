# How It Works

Lodestar sits over a call as a discreet overlay. It listens, transcribes both sides, and
answers the other party's questions on the spot as short spoken lines you can read aloud.

## What it does

- Captures the other side of a call via WASAPI loopback and/or your own microphone, with
  selectable input/output devices.
- Transcribes each side separately and labels it (You vs Them); on Deepgram, the call side
  can be diarized into multiple speakers.
- Detects when the other party asks a question and answers it automatically, streaming the
  answer token by token as it is generated.
- Writes answers as a few complete first-person sentences meant to be spoken verbatim, not
  study-note bullets, with a dim non-spoken "Next" hint for multi-part questions.
- Renders streamed answers as Markdown with fenced code blocks and LaTeX (KaTeX).
- Can also read the screen: capture a monitor and ask Claude vision to find and answer
  whatever problem or question is shown.
- Mirrors the live session to a phone on the same LAN as a second screen, with start/stop
  and ask controls relayed back.
- Keeps all data (profiles, stories, saved sessions) on the machine. The cloud paths (Claude
  answers, Deepgram transcription) are opt-in; a fully local mode (Ollama + on-device
  Whisper) keeps everything on-device.

## Key features

- Live transcription with a live input-level meter for diagnostics.
- Answer engine: Claude (cloud, default) or a local Ollama model, selectable per session.
- Local token usage tally and cost estimate, since the Claude API reports usage per response
  but exposes no balance endpoint.
- Always-on-top, translucent overlay windows that can sit discreetly over a video call.
- Global mouse-button gestures (peek behind, drag, toggle always-on-top) so the overlay never
  needs its own title bar interaction during a live call.
- Focused-window text capture via Windows UI Automation, as a fallback when the clipboard
  route is unavailable (proctored sites that disable copy/select).
- A GPU-temperature readout baked into the system-tray icon.

## The main path

1. Start a session. Audio capture begins (loopback and/or mic) and streams to Deepgram or
   the local Whisper model.
2. Transcribed lines appear labeled You/Them in the live transcript.
3. When a line from Them looks like a question, it is sent with recent context to the
   configured answer engine (Claude or local Ollama).
4. The answer streams back token by token and renders in the overlay as spoken-style
   Markdown/LaTeX.
5. Optionally, the user triggers a screen capture; the captured image goes to Claude vision
   through the same answer path.
6. Everything typed or answered can be mirrored to the phone companion, which can also send
   start/stop/ask actions back into the same session.
7. Profiles, STAR stories and saved sessions persist locally; nothing is uploaded except the
   opt-in calls to Claude or Deepgram themselves.
