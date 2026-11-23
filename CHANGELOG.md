# Changelog

All notable changes to this project are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## Unreleased - 0.4.0

- Phone companion hardened: pairing and the local connection are tightened for public use.
- Key storage fails closed: if the system credential store is unavailable, keys are not written to disk.
- App id renamed, with a one-time copy of existing data to the new location.
- Release workflow builds the installers on a clean runner and publishes SHA256 checksums.
- Docs rewritten for public users.

## [0.3.0] - 2026-09-16

- Assistant-only rebuild: Search, Tracker and Study features removed, scope fixed to the
  live interview copilot (#106, #107).
- Delivery coach, fast mode, predicted follow-ups, overclaim guard, and session debrief.
- README rewritten for the assistant-only interview copilot; smoke test updated.

## [0.2.1] - 2026-07-08

- The MB5 recall shortcut raises the overlay to the front without stealing focus.

## [0.2.0] - 2026-07-06

- App renamed from JobRadar to Lodestar.
- Discreet always-on-top overlay window with a live-answer toolbar and timer.
- Richer assistant answers: approach, commented code and steps.
- Keyboard-shortcuts reference, capture-safe dropdowns, and no native title tooltips.
- Live GPU-temperature tray icon and background launch by default.
- Claude Sonnet 5 added to the model list.

## [0.1.1] - 2026-06-30

- Deep dive answer style and a style picker in capture mode.
- Code-steps answer style.

## [0.1.0] - 2026-06-29

- Initial release, when Lodestar was a local-first job-market search tool.
