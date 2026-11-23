# Security

## Supported versions

Only the latest release on the [Releases page](https://github.com/w1ck3ds0d4/Lodestar/releases/latest) receives security fixes. Lodestar has no auto-updater, so update by hand when a new release is published.

## Reporting a vulnerability

Please do not open a public issue for a vulnerability.

- Use GitHub's private vulnerability reporting: the **Security** tab of the repository, then **Report a vulnerability**.
- Or email support@wickitlab.com.

Please include:

- The Lodestar version (from the release or installer file name) and your Windows version
- What you found and the impact you think it has
- Steps to reproduce, or a proof of concept
- Whether the issue needs a particular setup (phone server running, a given engine, a given network)

Never include real API keys, resumes or call transcripts in a report. Redact them.

Lodestar is maintained by a small team, so there is no service-level agreement. We aim to acknowledge a report within about a week and to tell you what we plan to do about it. Fixes ship as a new release, and we are happy to credit you if you want that.

## Scope

In scope: the desktop app and its installers, the phone companion server and page, local storage of keys and data, and the release workflow's artifacts.

Out of scope:

- Vulnerabilities in Anthropic, Deepgram, Ollama, Hugging Face or Windows themselves (report those to their owners)
- Attacks that need an attacker who already controls your Windows account or an administrator account (they can read the unencrypted database and use your keys)
- Screen-share hiding failing in a given capture tool (it is best effort, see below)
- Findings that only apply to an unsigned installer being unsigned

## What the app touches

- **Audio**: system-audio loopback (WASAPI) and/or the microphone, only while a session is running. Audio is transcribed and discarded; it is not written to disk.
- **Screen**: monitor capture and focused-window text reading (UI Automation) only when you trigger them (Answer screen, Watch, Snip, Read page). Captures are sent to the selected answer engine and are not stored as files.
- **Files**: a local SQLite database (`lodestar.db`) for settings, profiles, stories and saved sessions, and the downloaded Whisper model, all in `%APPDATA%\com.wickitlab.lodestar`, plus web view data in `%LOCALAPPDATA%\com.wickitlab.lodestar`. The database is not encrypted. On first launch after an upgrade from a version that used `com.w1ck3ds0d4.jobradar`, that data is copied once and the old folders are left untouched. When you export a session, its transcript and answers are written as plain, unencrypted Markdown files to `Downloads\Lodestar` (falling back to Documents, then the app-data folder); Lodestar never deletes them.
- **Keys**: Anthropic and Deepgram keys are encrypted with Windows DPAPI (current-user scope) before being stored, and are never logged. If encryption fails the key is not saved.

## Data flows

Outbound connections happen only when the matching feature is used:

- `api.anthropic.com`: questions, profile context and, for the vision features, screenshots, when Claude is the answer engine
- `api.deepgram.com`: captured audio over a WebSocket when Deepgram is the transcription engine, and a REST call for the account balance in the usage panel
- `huggingface.co`: a one-time download of the Whisper model, pinned to a fixed revision and checked against built-in SHA-256 checksums; a mismatch is refused
- The Ollama address you configure (default `http://localhost:11434`): the same content as a Claude request
- `console.anthropic.com` and `console.deepgram.com`: opened in your browser only when you click a link

There is no telemetry and no update check. The web view runs under a strict content security policy that allows no remote connections of its own, and the web view is granted only the core window controls plus the Tauri opener plugin (it opens the console links in your browser) and the notification plugin; there is no shell, filesystem or HTTP plugin.

## Phone server threat model

The phone companion is off until you start it. While running it listens on all interfaces on port 8420 and serves a page over plain HTTP and WebSocket. There is no TLS.

- **Protection**: a random 6-digit PIN, sent as the first WebSocket message, and an attempt limit. A PIN gets 5 wrong attempts and is then replaced by a new one, which the desktop shows. After 3 replacements with no successful pairing the server stops itself and has to be started again. The budget is shared by everyone on the network, so any device on the LAN can force that stop by guessing wrong. The QR code carries the PIN. Requests whose Host header is not the server's own address, and connections from a browser page on another origin, are refused.
- **What an authenticated phone can do**: read the live transcript and answers, and send commands (ask, listen, change opacity, read the page or screen). A phone can switch screen-share protection on but not off. Treat the PIN as full control of the session.
- **What it does not defend against**: anyone on the same network who can watch the traffic (the PIN and the transcript are visible in clear text), or who guesses the PIN within the attempt limit. A 6-digit PIN is a small space.
- **Firewall**: the optional fix marks the network Private and opens the port to the local subnet only. Without it Windows may block phone access.
- **Advice**: use it only on a network you trust, stop the server when you finish, and never on public or shared Wi-Fi.

## Screen-capture caveat

Lodestar uses the Windows display-affinity setting to ask Windows to exclude its windows from capture, and hides itself from the taskbar and Alt+Tab. This is a convenience, not a security boundary. It depends on the Windows version, graphics driver and capture software, can be defeated by a capture method that does not honour it, and does not hide the mouse cursor. Do not treat it as a guarantee that a share or recording cannot see the window.

## Dependencies

Dependencies are tracked by Dependabot and a security workflow. Run `pnpm audit` and `cargo audit` yourself if you build from source.
