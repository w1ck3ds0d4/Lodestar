<p align="center">
  <img src="assets/banner.svg" alt="JobRadar - Local-first job-market search for your desktop" width="100%" />
</p>

**Local-first job-market search for your desktop**

JobRadar polls a set of job sources on a schedule, normalizes and de-duplicates the postings into a local SQLite database, then lets you full-text search, filter by workplace and experience level, and get notified about new matches. No account, no server, no tracking: every posting lives in a database on your own machine.

It ships nine source adapters out of the box (public ATS boards, keyless remote-job feeds, a generic structured-data scraper, and two credential-gated APIs), a background ingestion scheduler that honors each source's polite polling interval, and saved-search alerts that fire native desktop notifications.

Built with Tauri v2 (Rust) + React + Vite. The Rust core owns ingestion, storage (SQLite + FTS5), and search; the React frontend is the UI.

---

## Features (Built)

### Search & Filtering

- Full-text search over title, company, location, and description (SQLite FTS5, BM25 ranking, prefix matching)
- Debounced search-as-you-type, empty query falls back to most-recently-seen first
- **Workplace filter**: Remote, Hybrid, On-site (classified per posting from the source signals plus location/title keywords)
- **Experience-level filter**: Entry / Junior, Mid, Senior (inferred from the title, employment type, and description, biased toward recognizing entry-level and no-experience roles)
- Source filter chips, one-click "Clear all", live job + company counts

### Sources

- Nine adapters behind a single `Source` trait: adding one is a module plus a match arm
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

### Local-first

- SQLite (bundled) with an FTS5 external-content index and triggers; the database is created in the OS-standard app-data directory
- No account, no backend server, no telemetry: the app makes no outbound calls except to the job sources you enable
- "Open" launches the posting in your default browser via the system opener

---

## Tech Stack

| Layer            | Technology                                      |
| ---------------- | ----------------------------------------------- |
| Desktop          | Tauri v2 (Rust)                                 |
| Frontend         | React 19 + TypeScript + Vite                    |
| Storage          | SQLite (rusqlite, bundled) with FTS5            |
| Search           | FTS5 full-text, BM25 ranking                    |
| HTTP             | reqwest (rustls, no OpenSSL)                     |
| Async runtime    | tokio                                           |
| Notifications    | tauri-plugin-notification                       |
| Icons            | lucide-react                                    |
| Package manager  | pnpm                                            |

---

## Prerequisites

- [Rust stable toolchain](https://rustup.rs/)
- [Node.js](https://nodejs.org/) 20+
- [pnpm](https://pnpm.io/installation)
- Windows 10 / 11 with [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) (preinstalled on most modern installs), plus Visual Studio 2022 Build Tools with the "Desktop development with C++" workload. macOS and Linux builds work with the standard Tauri toolchain.

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

---

## Sources

Keyless (work out of the box):

| Source                | Kind         | Notes                                                              |
| --------------------- | ------------ | ------------------------------------------------------------------ |
| Greenhouse            | `greenhouse` | Public ATS board per company. Config: `{ "company": "stripe" }`.   |
| Lever                 | `lever`      | Public ATS board per company. Config: `{ "company": "..." }`.      |
| Ashby                 | `ashby`      | Public job board per company. Honors the `isListed` flag.          |
| Arbeitnow             | `arbeitnow`  | Global / EU + remote feed. No config.                              |
| Remotive              | `remotive`   | Remote-jobs feed. Polled politely (6h), attributed to Remotive.    |
| Jobicy                | `jobicy`     | Remote-jobs feed. Polled hourly, attributed to Jobicy.             |
| Career page (JSON-LD) | `jsonld`     | Parses schema.org `JobPosting` markup from any careers URL.        |

Credential-gated (add your own free key in the Sources drawer):

| Source  | Kind      | Config                                                                          |
| ------- | --------- | ------------------------------------------------------------------------------- |
| Adzuna  | `adzuna`  | `{ "app_id", "app_key", "country", "what", "where" }`                           |
| USAJOBS | `usajobs` | `{ "email", "api_key", "keyword" }`                                             |

---

## Personal Use & Terms

This build is intended for personal, local use: the data stays on your machine and is not redistributed. The ingestion scheduler honors each source's polite polling interval, and the adapters carry the source attribution (Remotive, Jobicy, Adzuna) and listing flags (Ashby) those sources ask for.

If you ever turn this into a public, multi-user website, the redistribution terms of these sources change materially. Several feeds permit personal display with attribution but forbid re-aggregation, and the credentialed APIs gate commercial / public use behind a license. Check each source's Terms of Service before publishing.

---

## What's Not Yet Built

- Explicit hybrid/on-site signals from more ATS sources (currently inferred from text where the source does not state it)
- More credential-gated sources (Reed, The Muse, Jooble, Careerjet)
- A configurable per-source schedule UI (intervals are editable; cron-style timing is not)
- Saved/exported searches and CSV/JSON export of results
- Auto-updater and signed release artifacts

---

## License

This project is dual-licensed:

- [AGPL v3](LICENSE) - free for open-source use. Derivatives and SaaS deployments must release their source under AGPL.
- [Commercial license](COMMERCIAL.md) - for proprietary / closed-source use or hosted services that do not want to comply with AGPL source-disclosure requirements. Contact for terms.
