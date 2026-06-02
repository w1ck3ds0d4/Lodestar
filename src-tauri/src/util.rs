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

/// Parse a `YYYY-MM-DD` date (the form every adapter stores in `posted_at`) into Unix
/// milliseconds at UTC midnight, for freshness filtering/sorting. Inverse of
/// [`epoch_to_date`]; returns None if the string doesn't start with a valid date.
pub fn date_to_epoch_ms(s: &str) -> Option<i64> {
    let date = s.get(..10)?;
    let mut parts = date.split('-');
    let y: i64 = parts.next()?.parse().ok()?;
    let m: i64 = parts.next()?.parse().ok()?;
    let d: i64 = parts.next()?.parse().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    // Howard Hinnant's days-from-civil (the inverse of the algorithm in epoch_to_date).
    let yy = y - (m <= 2) as i64;
    let era = if yy >= 0 { yy } else { yy - 399 } / 400;
    let yoe = yy - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400_000)
}

/// Best-effort parse of a source's salary string into (min, max, currency code) in
/// whole annual units. Deliberately conservative: only parses when a currency
/// symbol/code is present, so years, counts, or "401k" are never mistaken for pay.
/// Numbers are normalized (commas stripped, a trailing `k` = x1000) and bounded to a
/// sane range. FX conversion is out of scope - amounts stay in the stated currency.
pub fn parse_salary(s: &str) -> (Option<i64>, Option<i64>, Option<String>) {
    let lower = s.to_lowercase();
    let currency = if s.contains('$') || lower.contains("usd") {
        "USD"
    } else if s.contains('€') || lower.contains("eur") {
        "EUR"
    } else if s.contains('£') || lower.contains("gbp") {
        "GBP"
    } else {
        return (None, None, None);
    };

    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"(?i)(\d[\d,.]*)\s*(k)?").unwrap());
    let mut nums: Vec<i64> = Vec::new();
    for c in re.captures_iter(s) {
        let raw = c.get(1).unwrap().as_str().replace(',', "");
        let Ok(mut n) = raw.trim_end_matches('.').parse::<f64>() else {
            continue;
        };
        if c.get(2).is_some() {
            n *= 1000.0;
        }
        let n = n.round() as i64;
        if (1000..=10_000_000).contains(&n) {
            nums.push(n);
        }
    }
    if nums.is_empty() {
        return (None, None, Some(currency.to_string()));
    }
    let (lo, hi) = (*nums.iter().min().unwrap(), *nums.iter().max().unwrap());
    let (min, max) = if lower.contains("up to") || lower.contains("max") {
        (None, Some(hi))
    } else if s.contains('+') || lower.contains("from ") || lower.contains("starting") {
        (Some(lo), None)
    } else if nums.len() >= 2 {
        (Some(lo), Some(hi))
    } else {
        (Some(lo), Some(lo))
    };
    (min, max, Some(currency.to_string()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn redact_secrets_strips_query_strings() {
        assert_eq!(
            redact_secrets("failed to GET https://api.example.com/jobs?app_key=SECRET&x=1"),
            "failed to GET https://api.example.com/jobs?[redacted]"
        );
        // No query string is left untouched.
        assert_eq!(
            redact_secrets("error at https://example.com/path"),
            "error at https://example.com/path"
        );
    }

    #[test]
    fn host_str_blocked_rejects_local_and_internal() {
        let blocked = [
            "http://localhost/",
            "http://service.local/",
            "http://127.0.0.1/",
            "http://10.0.0.5/",
            "http://192.168.1.1/",
            "http://169.254.169.254/", // cloud metadata
            "ftp://example.com/",      // non-http scheme
        ];
        for u in blocked {
            assert!(
                host_str_blocked(&reqwest::Url::parse(u).unwrap()),
                "should block {u}"
            );
        }
        assert!(!host_str_blocked(
            &reqwest::Url::parse("https://example.com/jobs").unwrap()
        ));
    }

    #[test]
    fn validate_public_url_rejects_without_dns() {
        assert!(validate_public_url("not a url").is_err());
        assert!(validate_public_url("http://localhost").is_err());
        assert!(validate_public_url("http://127.0.0.1:8080").is_err());
        assert!(validate_public_url("ftp://example.com").is_err());
    }

    #[test]
    fn stable_hash_is_deterministic_hex() {
        let a = stable_hash("hello");
        assert_eq!(a, stable_hash("hello"));
        assert_ne!(a, stable_hash("world"));
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn vstr_trims_and_drops_empty() {
        let v = json!({ "a": "  hi  ", "blank": "   ", "n": 5 });
        assert_eq!(vstr(&v, "a"), Some("hi".to_string()));
        assert_eq!(vstr(&v, "blank"), None);
        assert_eq!(vstr(&v, "n"), None);
        assert_eq!(vstr(&v, "missing"), None);
    }

    #[test]
    fn clean_html_strips_tags_decodes_entities_collapses_ws() {
        assert_eq!(
            clean_html("<p>Hello&nbsp;&amp;   <b>welcome</b></p>"),
            "Hello & welcome"
        );
    }

    #[test]
    fn epoch_to_date_known_dates() {
        assert_eq!(epoch_to_date(0), "1970-01-01");
        assert_eq!(epoch_to_date(1_700_000_000), "2023-11-14");
    }

    #[test]
    fn parse_salary_handles_ranges_bounds_and_currency() {
        assert_eq!(
            parse_salary("$120000-$160000"),
            (Some(120000), Some(160000), Some("USD".into()))
        );
        assert_eq!(
            parse_salary("$120000+"),
            (Some(120000), None, Some("USD".into()))
        );
        assert_eq!(
            parse_salary("up to $160000"),
            (None, Some(160000), Some("USD".into()))
        );
        assert_eq!(
            parse_salary("$70k - $90k"),
            (Some(70000), Some(90000), Some("USD".into()))
        );
        assert_eq!(
            parse_salary("€50,000"),
            (Some(50000), Some(50000), Some("EUR".into()))
        );
        // No currency -> not parsed (avoids years/counts/401k as "pay").
        assert_eq!(parse_salary("120000"), (None, None, None));
        assert_eq!(parse_salary(""), (None, None, None));
    }

    #[test]
    fn date_to_epoch_ms_parses_and_round_trips() {
        assert_eq!(date_to_epoch_ms("1970-01-01"), Some(0));
        assert_eq!(date_to_epoch_ms("2023-11-14"), Some(1_699_920_000_000));
        // Accepts a full timestamp prefix and round-trips through epoch_to_date.
        let ms = date_to_epoch_ms("2024-02-29T08:00:00Z").unwrap();
        assert_eq!(epoch_to_date(ms / 1000), "2024-02-29");
        assert_eq!(date_to_epoch_ms("not-a-date"), None);
        assert_eq!(date_to_epoch_ms("2023-13-01"), None);
    }

    #[test]
    fn infer_workplace_classifies() {
        assert_eq!(infer_workplace(false, "Remote", "Engineer"), "remote");
        assert_eq!(infer_workplace(true, "New York", ""), "remote");
        assert_eq!(infer_workplace(true, "Hybrid - Berlin", ""), "hybrid"); // hybrid wins
        assert_eq!(infer_workplace(false, "Berlin", "Engineer"), "onsite");
    }

    #[test]
    fn classify_experience_by_title_then_description() {
        assert_eq!(classify_experience("Senior Engineer", "", None), "senior");
        assert_eq!(classify_experience("Junior Developer", "", None), "entry");
        assert_eq!(
            classify_experience("Software Engineer", "", Some("Internship")),
            "entry"
        );
        assert_eq!(
            classify_experience("Software Engineer", "No experience required.", None),
            "entry"
        );
        assert_eq!(
            classify_experience("Software Engineer", "We build things.", None),
            "mid"
        );
    }

    #[test]
    fn normalize_level_buckets() {
        assert_eq!(normalize_level("Senior"), Some("senior".to_string()));
        assert_eq!(normalize_level("Junior"), Some("entry".to_string()));
        assert_eq!(normalize_level("Mid Level"), Some("mid".to_string()));
        assert_eq!(normalize_level("Associate"), Some("mid".to_string()));
        assert_eq!(normalize_level("Any"), None);
        assert_eq!(normalize_level(""), None);
    }

    #[test]
    fn detect_relocation_honors_negatives() {
        assert!(detect_relocation("We offer relocation assistance."));
        assert!(!detect_relocation("No relocation provided."));
        assert!(!detect_relocation("Relocation is not available."));
        assert!(!detect_relocation("Great team, great benefits."));
    }

    #[test]
    fn remote_region_detects_explicit_signals() {
        assert_eq!(remote_region(false, "Remote worldwide", "", ""), ""); // not flagged remote
        assert_eq!(remote_region(true, "Remote worldwide", "", ""), "worldwide");
        assert_eq!(
            remote_region(true, "", "", "US only, must be authorized"),
            "us"
        );
        assert_eq!(remote_region(true, "Remote - EU", "", ""), "eu");
        assert_eq!(remote_region(true, "UK only", "", ""), "uk");
        assert_eq!(remote_region(true, "Canada only", "", ""), "other");
        assert_eq!(remote_region(true, "Remote", "Engineer", "Join us"), "");
    }

    #[test]
    fn excluded_remote_regions_by_country() {
        assert_eq!(excluded_remote_regions("DE"), vec!["us", "uk", "other"]);
        assert_eq!(excluded_remote_regions("US"), vec!["eu", "uk", "other"]);
        assert_eq!(excluded_remote_regions("GB"), vec!["eu", "us", "other"]);
        assert_eq!(
            excluded_remote_regions("BR"),
            vec!["eu", "us", "uk", "other"]
        );
        assert_eq!(excluded_remote_regions(""), vec!["eu", "us", "uk", "other"]);
    }
}
