//! Small shared helpers: time, hashing, HTML cleanup, date conversion, and
//! lenient JSON field extraction used across the source adapters.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::net::{IpAddr, ToSocketAddrs};
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

/// Strip the query string from any URL inside a message. Network errors (reqwest's
/// `Display`) include the full request URL, so a source that puts a secret in the
/// query string (e.g. Adzuna's `app_key`) would otherwise leak it into the stored
/// source status, the logs, and the Sources panel. Keeps the scheme/host/path.
pub fn redact_secrets(msg: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(https?://[^\s?]+)\?[^\s]*").unwrap());
    re.replace_all(msg, "$1?[redacted]").into_owned()
}

/// True if an IP is loopback / private / link-local / otherwise non-public. Used to
/// keep user-supplied fetch targets from reaching localhost, cloud metadata
/// (169.254.169.254), or RFC-1918 internal hosts (SSRF guard).
fn ip_is_blocked(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.octets()[0] == 0
                // carrier-grade NAT 100.64.0.0/10
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xc0) == 0x40)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6
                    .to_ipv4()
                    .map(|m| ip_is_blocked(IpAddr::V4(m)))
                    .unwrap_or(false)
                // unique-local fc00::/7
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                // link-local fe80::/10
                || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

/// Host-string check (no DNS): blocks localhost-ish names and IP literals that are
/// non-public. Cheap enough to call inside a redirect policy.
pub fn host_str_blocked(url: &reqwest::Url) -> bool {
    if !matches!(url.scheme(), "http" | "https") {
        return true;
    }
    let Some(host) = url.host_str() else {
        return true;
    };
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    if h == "localhost" || h.ends_with(".localhost") || h.ends_with(".local") {
        return true;
    }
    if let Ok(ip) = h.parse::<IpAddr>() {
        return ip_is_blocked(ip);
    }
    false
}

/// Validate a user-supplied fetch URL: require http(s) and reject anything that
/// resolves to a local/internal address. Pair with a bounded, host-re-checking
/// redirect policy on the request itself.
pub fn validate_public_url(url: &str) -> Result<(), String> {
    let parsed = reqwest::Url::parse(url).map_err(|_| "Invalid URL".to_string())?;
    if host_str_blocked(&parsed) {
        return Err("Refusing to fetch a local, internal, or non-http(s) address".into());
    }
    let host = parsed.host_str().ok_or("URL has no host")?;
    let port = parsed.port_or_known_default().unwrap_or(80);
    let mut resolved = false;
    for addr in (host, port)
        .to_socket_addrs()
        .map_err(|e| format!("Could not resolve host: {e}"))?
    {
        resolved = true;
        if ip_is_blocked(addr.ip()) {
            return Err("Refusing to fetch a local or internal address".into());
        }
    }
    if !resolved {
        return Err("Host did not resolve".into());
    }
    Ok(())
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

/// Normalize a source-provided seniority string (e.g. Jobicy's `jobLevel`:
/// "Senior", "Mid Level", "Junior", "Any") into our buckets. Returns `None` when
/// the value is empty or unspecific ("any"/"all"), so the caller can fall back to
/// the title/description heuristic.
pub fn normalize_level(raw: &str) -> Option<String> {
    let s = raw.trim().to_lowercase();
    if s.is_empty() || s == "any" || s == "all" || s == "n/a" {
        return None;
    }
    if s.contains("intern")
        || s.contains("junior")
        || s.contains("entry")
        || s.contains("graduate")
        || s.contains("trainee")
        || s.contains("apprentice")
    {
        Some("entry".to_string())
    } else if s.contains("senior")
        || s.contains("lead")
        || s.contains("principal")
        || s.contains("staff")
        || s.contains("director")
        || s.contains("executive")
        || s.contains("expert")
    {
        Some("senior".to_string())
    } else if s.contains("mid") || s.contains("intermediate") || s.contains("associate") {
        Some("mid".to_string())
    } else {
        None
    }
}

/// Does the posting mention relocation help? Guards against explicit negatives.
pub fn detect_relocation(description: &str) -> bool {
    let d = description.to_lowercase();
    if !d.contains("relocation") && !d.contains("relocate") {
        return false;
    }
    !(d.contains("no relocation")
        || d.contains("relocation is not")
        || d.contains("not offer relocation")
        || d.contains("without relocation"))
}

/// Best-effort remote-eligibility region for a remote role, inferred from
/// location/title/description: "worldwide" | "eu" | "us" | "uk" | "other" | ""
/// (the last meaning not-remote or no stated restriction). Heuristic - sources
/// rarely state geo-eligibility cleanly, so this only catches explicit signals.
pub fn remote_region(remote: bool, location: &str, title: &str, description: &str) -> &'static str {
    if !remote {
        return "";
    }
    let text = format!(
        "{} {} {}",
        location.to_lowercase(),
        title.to_lowercase(),
        description.to_lowercase()
    );
    if text.contains("worldwide")
        || text.contains("work from anywhere")
        || text.contains("anywhere in the world")
        || text.contains("remote anywhere")
        || text.contains("global remote")
    {
        return "worldwide";
    }
    if text.contains("remote (eu")
        || text.contains("remote - eu")
        || text.contains("eu only")
        || text.contains("eu-based")
        || text.contains("europe only")
        || text.contains("within europe")
        || text.contains("eea")
        || text.contains("european union")
    {
        return "eu";
    }
    if text.contains("us only")
        || text.contains("u.s. only")
        || text.contains("usa only")
        || text.contains("united states only")
        || text.contains("us-based")
        || text.contains("us residents")
        || text.contains("authorized to work in the united states")
        || text.contains("us time zone")
        || text.contains("est time zone")
    {
        return "us";
    }
    if text.contains("uk only")
        || text.contains("uk-based")
        || text.contains("united kingdom only")
        || text.contains("must be based in the uk")
    {
        return "uk";
    }
    if text.contains("canada only")
        || text.contains("australia only")
        || text.contains("india only")
    {
        return "other";
    }
    ""
}

/// EU/EEA member country codes treated as "remote-eligible from the EU".
const EU_CODES: &[&str] = &[
    "AT", "BE", "BG", "HR", "CY", "CZ", "DK", "EE", "FI", "FR", "DE", "GR", "HU", "IE", "IS", "IT",
    "LI", "LT", "LU", "LV", "MT", "NL", "NO", "PL", "PT", "RO", "SE", "SI", "SK", "ES",
];

/// Region tags (see [`remote_region`]) to EXCLUDE when filtering remote roles to
/// those workable from `country` (an ISO 3166-1 alpha-2 code). Worldwide and
/// unstated roles are always kept; only postings locked to a different region
/// are dropped. An unknown or blank country keeps only worldwide + unstated.
pub fn excluded_remote_regions(country: &str) -> Vec<&'static str> {
    let country = country.trim().to_uppercase();
    let keep: &[&str] = if EU_CODES.contains(&country.as_str()) {
        &["eu"]
    } else if country == "US" {
        &["us"]
    } else if country == "GB" || country == "UK" {
        &["uk"]
    } else {
        &[]
    };
    ["eu", "us", "uk", "other"]
        .into_iter()
        .filter(|r| !keep.contains(r))
        .collect()
}
