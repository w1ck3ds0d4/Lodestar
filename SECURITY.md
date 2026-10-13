# Security

## Posture

Lodestar is a personal, local-first desktop app. It is not distributed to anyone else: CI
attaches the installers to this private repository's Releases, and it runs only on the
maintainer's own machine.

What it touches:

- **Audio**: system-audio loopback (WASAPI) and/or the microphone, while a session is
  running. Audio is transcribed and discarded; it is not written to disk as raw audio.
- **Screen**: on-demand monitor capture (`screen.rs`), triggered explicitly by the user, sent
  once to Claude vision to answer whatever is shown, not continuously recorded.
- **Network**: outbound calls only, and only when the corresponding feature is enabled:
  Deepgram (transcription), Anthropic's Claude API (answers, vision), a local Ollama server,
  and a local Candle/Whisper model download (one-time, cached).
- **Files**: local SQLite (`db.rs`) for settings, profiles, STAR stories and saved sessions;
  no other files are read or written outside the app-data directory.
- **LAN**: the phone-companion server (`phone.rs`) binds a local port and serves a PIN-gated
  page to a phone on the same Wi-Fi; `netcheck.rs` checks Windows Firewall reachability
  before advertising it as available.

## Secrets

API keys (Claude, Deepgram) are stored through `secret.rs`, which encrypts them at rest with
Windows DPAPI scoped to the current user account: the ciphertext cannot be read by another
user or on another machine. A value without the `dpapi:` prefix (legacy plaintext) is read
as-is and re-encrypted on its next write, so there is no plaintext-only fallback going
forward. Keys are never logged and never leave the machine except as the Authorization header
of the request they belong to.

## Known risks

- The app has 22 open Dependabot alerts as of this review, including a currently-failing
  `glib` bump; an app that captures audio/screen and calls external model APIs is a real
  surface to keep current, and clearing these is the top item on the roadmap.
- The Capture overlay and phone companion widen the window during a live interview; the PIN
  on the phone companion is the only gate on that LAN surface today.
- Cloud paths (Claude, Deepgram) send transcript and screen content to third-party APIs when
  enabled; Anthropic's and Deepgram's own data-handling terms apply to that traffic.

## Reporting a problem

This is a private, single-user repository. Report a problem through GitHub's private
vulnerability reporting on this repository (Security tab) or by emailing
support@wickitlab.com. There is no public bug tracker for security issues here.
