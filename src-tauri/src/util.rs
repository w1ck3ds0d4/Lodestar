//! Small shared helpers: time, hashing, HTML cleanup, date conversion, and
//! lenient JSON field extraction used across the source adapters.

use std::time::{SystemTime, UNIX_EPOCH};

/// Current time in milliseconds since the Unix epoch.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
