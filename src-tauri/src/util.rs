//! Small shared helpers: time, hashing, HTML cleanup, date conversion, and
//! lenient JSON field extraction used across the source adapters.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;
use serde_json::Value;

/// Current time in milliseconds since the Unix epoch.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Deterministic short hex hash, stable across runs of the same binary. Used for
/// content de-duplication and for synthesizing ids when a source provides none.
pub fn stable_hash(s: &str) -> String {
    let mut h = DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// Pull a non-empty, trimmed string field out of a JSON object.
pub fn vstr(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(|x| x.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Required string config field.
pub fn cfg_str(cfg: &Value, key: &str) -> anyhow::Result<String> {
    vstr(cfg, key).ok_or_else(|| anyhow::anyhow!("missing config field: {key}"))
}

/// Optional string config field.
pub fn cfg_opt(cfg: &Value, key: &str) -> Option<String> {
    vstr(cfg, key)
}

/// Heuristic: does this location string describe a remote role?
pub fn looks_remote(location: &str) -> bool {
    let l = location.to_lowercase();
    l.contains("remote") || l.contains("anywhere") || l.contains("telecommute")
}

fn tag_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"<[^>]*>").unwrap())
}

fn ws_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| Regex::new(r"\s+").unwrap())
}

/// Strip HTML tags, decode the common entities, and collapse whitespace into a
/// clean single-spaced plain-text string suitable for indexing and display.
pub fn clean_html(s: &str) -> String {
    let no_tags = tag_re().replace_all(s, " ");
    let decoded = no_tags
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&rsquo;", "'")
        .replace("&lsquo;", "'")
        .replace("&ldquo;", "\"")
        .replace("&rdquo;", "\"")
        .replace("&mdash;", "-")
        .replace("&ndash;", "-");
    ws_re().replace_all(decoded.trim(), " ").to_string()
}

/// Convert a Unix timestamp (seconds) to a `YYYY-MM-DD` date string, using
/// Howard Hinnant's days-from-civil algorithm so we avoid a date-library dep.
pub fn epoch_to_date(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// Classify the work arrangement as one of "remote" | "hybrid" | "onsite".
/// Uses the source's remote flag plus keywords in the location/title.
pub fn infer_workplace(remote: bool, location: &str, title: &str) -> String {
    let text = format!("{} {}", location.to_lowercase(), title.to_lowercase());
    if text.contains("hybrid") {
        "hybrid".to_string()
    } else if remote
        || text.contains("remote")
        || text.contains("anywhere")
        || text.contains("telecommute")
        || text.contains("work from home")
        || text.contains("wfh")
    {
        "remote".to_string()
    } else {
        "onsite".to_string()
    }
}

/// Classify the seniority as one of "entry" | "mid" | "senior", inferred from the
/// title first, then the employment type and description. Biased toward
/// recognizing entry-level / no-experience roles.
pub fn classify_experience(
    title: &str,
    description: &str,
    employment_type: Option<&str>,
) -> String {
    let t = title.to_lowercase();
    let emp = employment_type.unwrap_or("").to_lowercase();

    const ENTRY_TITLE: &[&str] = &[
        "intern",
        "internship",
        "junior",
        "jr.",
        "jr ",
        "entry level",
        "entry-level",
        "graduate",
        "new grad",
        "newgrad",
        "trainee",
        "apprentice",
        "working student",
        "werkstudent",
        "early career",
        "no experience",
    ];
    const SENIOR_TITLE: &[&str] = &[
        "senior",
        "sr.",
        "sr ",
        "staff ",
        "principal",
        "lead ",
        "lead,",
        "head of",
        "director",
        "vp ",
        "vice president",
        "distinguished",
    ];

    if emp.contains("intern") || ENTRY_TITLE.iter().any(|k| t.contains(k)) {
        return "entry".to_string();
    }
    if SENIOR_TITLE.iter().any(|k| t.contains(k)) {
        return "senior".to_string();
    }

    let d = description.to_lowercase();
    const ENTRY_DESC: &[&str] = &[
        "no experience required",
        "no prior experience",
        "no experience necessary",
        "entry-level",
        "entry level",
        "0-1 year",
        "0-2 year",
        "0 to 1 year",
    ];
    if ENTRY_DESC.iter().any(|k| d.contains(k)) {
        return "entry".to_string();
    }

    "mid".to_string()
}
