//! Small shared helpers. Currently just the epoch-millisecond clock used
//! across the backend; the source-adapter helpers were removed in v0.3.0.

use std::time::{SystemTime, UNIX_EPOCH};

/// Current time in milliseconds since the Unix epoch.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
