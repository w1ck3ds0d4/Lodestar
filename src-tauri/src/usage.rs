//! Local Claude token tally for the in-app cost estimate.
//!
//! Anthropic exposes no balance endpoint for a normal API key, but every Messages
//! response reports token usage. We accumulate those per model into a single JSON
//! setting (`claude_usage`) plus a "since" timestamp, so the Assistant tab can show an
//! approximate spend. The recorder is reachable from the deep `complete_content` call
//! site without threading app state through every layer, via a process-global DB handle
//! installed once at startup.

use std::sync::{Arc, Mutex, OnceLock};

use rusqlite::Connection;
use serde_json::json;

static DB: OnceLock<Arc<Mutex<Connection>>> = OnceLock::new();

/// Install the shared DB handle once at startup. A second call is a no-op.
pub fn init(db: Arc<Mutex<Connection>>) {
    let _ = DB.set(db);
}

/// Add a Claude response's token usage to the running per-model tally. Best-effort:
/// any failure (no DB yet, lock poisoned, bad JSON) is silently ignored so usage
/// accounting can never break an actual answer.
pub fn record_claude(model: &str, input: u64, output: u64) {
    if input == 0 && output == 0 {
        return;
    }
    let Some(db) = DB.get() else {
        return;
    };
    let Ok(conn) = db.lock() else {
        return;
    };
    let mut map = crate::db::get_setting(&conn, "claude_usage")
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .filter(|v| v.is_object())
        .unwrap_or_else(|| json!({}));
    let prev_in = map[model]["input"].as_u64().unwrap_or(0);
    let prev_out = map[model]["output"].as_u64().unwrap_or(0);
    map[model] = json!({ "input": prev_in + input, "output": prev_out + output });
    let _ = crate::db::set_setting(&conn, "claude_usage", &map.to_string());

    // Stamp the start of the tally window the first time we record anything.
    if crate::db::get_setting(&conn, "claude_usage_since")
        .ok()
        .flatten()
        .is_none()
    {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let _ = crate::db::set_setting(&conn, "claude_usage_since", &now.to_string());
    }
}
