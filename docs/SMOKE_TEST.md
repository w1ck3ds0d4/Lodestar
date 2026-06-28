# JobRadar release smoke test

A ~5-minute manual pass to run on a **packaged build** (not just `cargo`/`vite`)
before shipping. CI only compiles and unit-tests; it cannot catch runtime issues
like window freezes, multi-window regressions, or a broken key round-trip - so do
this on the real app.

## Build

- [ ] `pnpm install` is clean.
- [ ] `pnpm tauri build` succeeds; an installer/exe is produced under
      `src-tauri/target/release/bundle/`.
- [ ] Launch the built app (not `tauri dev`).

## Launch & startup

- [ ] The JobRadar **splash appears immediately** - no blank or frozen window.
- [ ] The splash fades to the populated job list within a few seconds.
- [ ] On the **first launch after updating**, the app stays responsive while the
      one-time derived-column backfill runs in the background (it should not hang).

## Search & filters

- [ ] Typing in search updates results (debounced); no freeze while typing.
- [ ] Each filter group works: Remote/workplace, level, Posted, Min salary,
      Language, Source.
- [ ] "Hide likely scams" hides flagged rows; "Review"/"Likely scam" badges show
      on others with a why-flagged tooltip.
- [ ] With a resume saved in the Assistant: "Best match" sort + "NN% match"
      badges appear, and the Min-match chips filter.
- [ ] Open a job's link (external browser), Track a job, and Score a job (LLM)
      returns a scorecard.

## Sources

- [ ] Sources panel lists the seeded sources with last-run status/counts.
- [ ] Add a keyed source (e.g. Findwork), paste a key, click **Test** -> shows
      "N jobs" on success or a **redacted** error (never the raw key).
- [ ] Add the generic **RSS feed** source with a feed URL -> Test pulls items.
- [ ] The app stays responsive across a background sync (every ~2 min) - no hang.

## Custom title bar (borderless window)

- [ ] Drag the window by the top bar; double-click the top bar maximizes/restores.
- [ ] Minimize / maximize / close buttons work; the middle button shows a restore
      icon while maximized. Resize from each edge and corner.

## Capture mode

- [ ] Open Capture mode -> the control bar + overlay panels (Answers, Transcript,
      Controls, Shortcuts, Log) appear **immediately** (no stuck loading splash).
- [ ] `Alt+Shift+C` toggles panel visibility; closing the main window tears the
      panels down (app exits cleanly).
- [ ] "Hide from taskbar & Alt+Tab" removes the window from both; the tray icon
      and `Alt+Shift+H` bring it back, and the title bar stays consistent.

## Secrets at rest (DPAPI encryption)

- [ ] After setting Claude/Deepgram keys and source API keys, the assistant,
      listening, and source fetches still work (keys decrypt correctly).
- [ ] In `jobradar.db` (the `settings` table and `sources.config`), secret values
      are stored as `dpapi:...` ciphertext, **not** plaintext.
      DB path (Windows): `%APPDATA%\<app identifier>\jobradar.db`.
- [ ] Keys entered **before** this update still work without re-entry (legacy
      plaintext is tolerated and re-encrypted on the next save).

## Assistant / interview (if used)

- [ ] Ask the assistant a question -> streamed answer renders.
- [ ] A Listen session (Deepgram or local STT) transcribes; the capture panels
      show transcript/answers.
- [ ] Read full page attaches the focused page's text (the Activity log shows the
      chars captured); a second click reports it as already attached (deduped).

---

When a step fails, capture what you did + the symptom and file it before
shipping. Keep this list updated as features land.
