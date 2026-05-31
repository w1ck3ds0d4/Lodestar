//! The ingestion loop: fetch due sources, upsert, sweep stale jobs, and fire
//! desktop notifications for saved-search alerts.

use std::sync::{Arc, Mutex};

use reqwest::Client;
use rusqlite::Connection;
use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::db;
use crate::util::now_ms;

/// Sync every source that is due (or all enabled sources when `force`), then run
/// alerts. Network I/O happens outside the db lock; the lock is held only for the
/// quick write after each fetch.
pub async fn sync_all(db: Arc<Mutex<Connection>>, http: Client, app: AppHandle, force: bool) {
    let sources = {
        let conn = db.lock().expect("db poisoned");
        db::due_sources(&conn, force, now_ms()).unwrap_or_default()
    };

    for src in sources {
        let started = now_ms();
        let result = crate::sources::fetch(&http, &src.kind, &src.config).await;
        let finished = now_ms();

        let mut conn = db.lock().expect("db poisoned");
        match result {
            Ok(jobs) => {
                let n = jobs.len();
                if let Err(e) = db::upsert_jobs(&mut conn, src.id, &src.kind, &jobs, finished) {
                    let msg = crate::util::redact_secrets(&e.to_string());
                    let _ =
                        db::set_source_status(&conn, src.id, now_ms(), &format!("error: {msg}"), 0);
                    tracing::warn!(source = %src.label, error = %msg, "upsert failed");
                    continue;
                }
                // Only sweep when we got a usable response, so a transient empty
                // or partial fetch doesn't wipe the source's jobs.
                if n > 0 {
                    let _ = db::mark_stale(&conn, src.id, started, finished);
                }
                let _ = db::set_source_status(&conn, src.id, now_ms(), "ok", n as i64);
                tracing::info!(source = %src.label, count = n, "synced");
            }
            Err(e) => {
                let msg = crate::util::redact_secrets(&e.to_string());
                let _ = db::set_source_status(&conn, src.id, now_ms(), &format!("error: {msg}"), 0);
                tracing::warn!(source = %src.label, error = %msg, "fetch failed");
            }
        }
    }

    run_alerts(&db, &app);
}

fn run_alerts(db: &Arc<Mutex<Connection>>, app: &AppHandle) {
    let conn = db.lock().expect("db poisoned");
    let alerts = db::list_alerts(&conn).unwrap_or_default();
    for a in alerts {
        let since = a.last_notified_at.unwrap_or(0);
        let count = db::count_new_matches(&conn, &a, since).unwrap_or(0);
        if count == 0 {
            continue;
        }
        let label = if a.query.trim().is_empty() {
            "your saved search".to_string()
        } else {
            format!("\"{}\"", a.query)
        };
        let _ = app
            .notification()
            .builder()
            .title("JobRadar")
            .body(format!("{count} new job(s) for {label}"))
            .show();
        let _ = db::set_alert_notified(&conn, a.id, now_ms());
    }
}
