# Roadmap

**Status:** continue. **Last reviewed:** 2026-11-22.

Lodestar is an open-source, local-first interview copilot: it listens to a call, transcribes both
sides, and answers questions on the spot as short spoken lines. It is released under the
AGPL-3.0-or-later, with a commercial license available (see [COMMERCIAL.md](COMMERCIAL.md)).
The aim is a stable app that is dependency-clean and gets better at what it already does.

> How this file is used: the maintainers own the order and the lists. Each work session builds the first unticked item under **Now**, one item per branch and pull request, and ticks it in that same PR as `- [x] ... (#PR)`. Contributors and sessions do not add to Now, Next or Later themselves: propose changes in an issue or under **Ideas**.

## Now

- [ ] **Clear the failing Dependabot alerts**: 22 open alerts, including a currently-failing
      `glib` bump that blocks CI on its own PR. Done when: the `glib` bump PR is green and the
      open alert count is meaningfully down.
- [x] **Add ARCHITECTURE.md and HOW_IT_WORKS.md**: a plain-words account of the Rust core
      (capture, transcription, model calls, storage) and the main call-to-answer path, so a
      the roadmap has the same grounding CRADesk gets. Done when: both files exist and describe
      the code as it stands today. (#118)
- [x] **Add SECURITY.md**: a real posture write-up for an app that captures audio/screen and
      calls external model APIs (Claude, Deepgram). Done when: the file covers what the app
      touches, how keys are stored (`secret.rs`), and how to report a problem. (#118)
- [ ] **Rotate stored cloud API keys**: confirm `secret.rs` storage is still sound after the
      2026-07 credential-rotation incident, and that no key has ever been logged or committed.
      Done when: a manual check is done and, if anything is stale, keys are rotated.

## Next

- [ ] **Extend the delivery-coach / fast-mode line**: this is the feature line that most
      directly moves interview outcomes. Done when: the next concrete improvement (the
      maintainers to specify) ships behind a test.
- [ ] **Phone companion polish**: `phone.rs`/`phone.html` drive the second-screen mode; fix
      the rough edges found in real interview use. Done when: a named rough edge is fixed
      and confirmed in a live or rehearsed session.

## Later

- Local-only mode (Ollama + on-device Whisper) parity checks against the cloud path
- Broader model coverage as new Claude models ship
- Session debrief improvements beyond the current v0.3.0 shape

## Ideas

(empty to start; proposals go here)

## Done

- [x] v0.3.0: delivery coach, fast mode, session debrief (2026-09-16), scope fixed to
      assistant-only (job-search, tracker and study features removed)
- [x] Speaker-attributed live transcription (mic + system audio via WASAPI loopback)
- [x] Streaming, spoken-style auto-answers with Markdown/LaTeX rendering
- [x] Local-only mode (Ollama + on-device Whisper)
- [x] Phone companion as a second screen
