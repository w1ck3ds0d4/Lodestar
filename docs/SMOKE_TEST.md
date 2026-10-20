# Lodestar release smoke test

A ~5-minute manual pass to run on a **packaged build** (not just `cargo`/`vite`)
before shipping. CI only compiles and unit-tests; it cannot catch runtime issues
like window freezes, multi-window regressions, or a broken key round-trip, so do
this on the real app.

## Build

- [ ] `pnpm install` is clean.
- [ ] `pnpm tauri build` succeeds; an installer/exe is produced under
      `src-tauri/target/release/bundle/`.
- [ ] Launch the built app (not `tauri dev`).

## Launch & startup

- [ ] The Lodestar **splash appears immediately**, no blank or frozen window.
- [ ] The splash fades to the **Assistant** view within a second or two.
- [ ] The top bar shows the **Assistant** and **Settings** tabs; switching between
      them does not drop a live session.

## Answering

- [ ] Type a question into **Ask** and send it -> a streamed answer renders as
      Markdown (code blocks / math where relevant).
- [ ] Switch the model in the top-bar picker -> the menu lists Opus 5, Sonnet 5,
      Haiku 4.5, and Fable 5.1, and the selection sticks.
- [ ] Change the answer style (Auto / Concise / STAR / Code) -> the next answer's
      shape follows it.
- [ ] **Settings -> Answers -> Answer speed**: toggling **Fast mode** and
      **Predicted follow-ups** persists across a relaunch.

## Live listen (if audio is set up)

- [ ] A **Listen** session (Deepgram or local STT) transcribes; the dialog shows
      You/Them attribution and per-line timestamps.
- [ ] The **delivery coach** in the dialog header shows talk time / pace / filler
      count while you speak, and clears when you stop.
- [ ] When the other party asks something, an **auto-answer** streams in.

## Context capture

- [ ] **Read full page** (button or `Alt+Shift+R`) attaches the focused page's
      text; the Activity log shows the chars captured, and a second click reports
      it as already attached (deduped).
- [ ] On a **Microsoft Edge** tab specifically, Read full page returns the page
      text (not empty) - this was the v0.3.0 reader fix.
- [ ] **Answer screen** / **Snip** answer what is visible on the display.

## Accuracy & review

- [ ] **Settings -> Accuracy**: with the overclaim guard on and a saved profile,
      an answer that claims experience the profile lacks shows a flag under it;
      a well-supported answer shows none.
- [ ] **Settings -> Review -> Debrief**: "Refresh debrief" analyses saved sessions
      and lists topics worst-first (or the empty state if no sessions are saved).
- [ ] Save a session, then run its review pass -> a coach summary renders.
- [ ] **Usage & cost** shows a Claude token/cost estimate and the Deepgram balance.

## Overlay & stealth

- [ ] **Always on top** (Overlay): with it on, raise another always-on-top window
      (e.g. Task Manager); Lodestar returns to the front within ~1s and does not
      steal your keyboard focus.
- [ ] `Alt+Shift+H` hides / shows the window; the tray icon brings it back.
- [ ] "Hide from screen capture" removes the window from a Zoom/Meet/Teams share.
- [ ] The window stays out of the taskbar and Alt+Tab (a hidden owner keeps it
      under Task Manager's Background processes).

## Custom title bar (borderless window)

- [ ] Drag the window by the top bar; double-click the top bar maximizes/restores.
- [ ] Minimize / maximize / close buttons work; the middle button shows a restore
      icon while maximized. Resize from each edge and corner.

## Secrets at rest (DPAPI encryption)

- [ ] After setting Claude / Deepgram keys, the assistant and listening still work
      (keys decrypt correctly).
- [ ] In `jobradar.db` (the `settings` table), secret values are stored as
      `dpapi:...` ciphertext, **not** plaintext.
      DB path (Windows): `%APPDATA%\<app identifier>\jobradar.db` (the legacy file name is
      kept so existing installs keep their data).
- [ ] Keys saved as plaintext by older builds still work without re-entry (legacy
      plaintext is tolerated and re-encrypted on the next save).

## Phone remote (if used)

- [ ] **Settings -> Phone control**: start the LAN server, scan the QR / enter the
      PIN on a phone; the transcript and answers mirror and the remote tiles drive
      Listen / Screen / Read & Answer.

---

When a step fails, capture what you did + the symptom and file it before
shipping. Keep this list updated as features land.
