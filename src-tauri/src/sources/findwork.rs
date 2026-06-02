//! Findwork.dev tech-jobs API (free key required). `GET https://findwork.dev/api/jobs/`
//! with an `Authorization: Token <key>` header returns `{ count, next, results: [...] }`.
//! The key rides in a request header, never the URL. Config:
//! `{ "api_key": "...", "auth_scheme": "Token|Bearer (optional)", "search": "...", "location": "...", "remote": "true (optional)" }`.

use anyhow::Result;
use reqwest::header::AUTHORIZATION;
use reqwest::Client;
use serde_json::Value;

use crate::model::RawJob;
use crate::util::{cfg_opt, cfg_str, clean_html, looks_remote, normalize_level, stable_hash, vstr};

const ENDPOINT: &str = "https://findwork.dev/api/jobs/";
/// Follow the `next` link this many times at most.
const MAX_PAGES: u32 = 3;

pub async fn fetch(http: &Client, cfg: &Value) -> Result<Vec<RawJob>> {
    let key = cfg_str(cfg, "api_key")?;
    // Findwork's docs use "Token"; some mirrors document "Bearer". Default to Token,
    // but let the user switch without a code change if their key needs Bearer.
    let scheme = cfg_opt(cfg, "auth_scheme").unwrap_or_else(|| "Token".into());
    let auth = format!("{scheme} {key}");

    // First page: endpoint + optional filters. `sort_by=date` surfaces the newest.
    let mut query: Vec<(&str, String)> = vec![("sort_by", "date".into())];
    if let Some(s) = cfg_opt(cfg, "search") {
        query.push(("search", s));
    }
    if let Some(l) = cfg_opt(cfg, "location") {
        query.push(("location", l));
    }
    if cfg_opt(cfg, "remote").is_some_and(|r| r.eq_ignore_ascii_case("true")) {
        query.push(("remote", "true".into()));
    }

    let mut out = Vec::new();
    // Page 1 carries the query params; subsequent pages follow the absolute `next` URL.
    let mut next: Option<String> = None;
    for _ in 0..MAX_PAGES {
        let req = match &next {
            Some(url) => http.get(url),
            None => http.get(ENDPOINT).query(&query),
        };
        let v: Value = req
            .header(AUTHORIZATION, &auth)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        out.extend(parse(&v));
        next = vstr(&v, "next");
        if next.is_none() {
            break;
        }
    }
    Ok(out)
}

/// Map a Findwork page into RawJobs (pure). `role` is the title, `company_name` the
/// employer, `text` the HTML body, `url` the apply link, `remote` a boolean flag, and
/// `date_posted` an ISO timestamp.
fn parse(v: &Value) -> Vec<RawJob> {
    let mut out = Vec::new();
    let Some(arr) = v.get("results").and_then(Value::as_array) else {
        return out;
    };
    for j in arr {
        let Some(title) = vstr(j, "role") else {
            continue;
        };
        let location = vstr(j, "location").unwrap_or_default();
        let remote =
            j.get("remote").and_then(Value::as_bool).unwrap_or(false) || looks_remote(&location);
        out.push(RawJob {
            experience_level: vstr(j, "role").as_deref().and_then(normalize_level),
            // Findwork (a Django REST app) returns a numeric `id`; accept that, a string
            // id, then fall back to a title hash so external_id stays stable for dedup.
            external_id: j
                .get("id")
                .and_then(Value::as_i64)
                .map(|n| n.to_string())
                .or_else(|| vstr(j, "id"))
                .unwrap_or_else(|| stable_hash(&title)),
            title,
            company: vstr(j, "company_name").unwrap_or_default(),
            remote,
            location,
            employment_type: vstr(j, "employment_type"),
            description: clean_html(&vstr(j, "text").unwrap_or_default()),
            url: vstr(j, "url").unwrap_or_default(),
            salary: None,
            // "2026-04-23T00:10:34Z" -> "2026-04-23"
            posted_at: vstr(j, "date_posted").map(|s| s.chars().take(10).collect()),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::parse;
    use serde_json::json;

    #[test]
    fn maps_results_with_remote_and_date() {
        let v = json!({
            "count": 2,
            "next": "https://findwork.dev/api/jobs/?page=2",
            "results": [
                {
                    "id": 1234567,
                    "role": "Senior Backend Engineer",
                    "company_name": "Acme",
                    "employment_type": "Full time",
                    "location": "Lisbon, Portugal",
                    "remote": true,
                    "url": "https://findwork.dev/jobs/acme-senior-backend",
                    "text": "<p>Build &amp; ship</p>",
                    "date_posted": "2026-04-23T00:10:34Z",
                    "keywords": ["rust"]
                },
                { "company_name": "NoRole" } // no role -> skipped
            ]
        });
        let jobs = parse(&v);
        assert_eq!(jobs.len(), 1);
        let j = &jobs[0];
        assert_eq!(
            j.external_id, "1234567",
            "numeric id must be read, not hashed"
        );
        assert_eq!(j.title, "Senior Backend Engineer");
        assert_eq!(j.company, "Acme");
        assert!(j.remote);
        assert_eq!(j.employment_type.as_deref(), Some("Full time"));
        assert_eq!(j.description, "Build & ship");
        assert_eq!(j.url, "https://findwork.dev/jobs/acme-senior-backend");
        assert_eq!(j.experience_level.as_deref(), Some("senior"));
        assert_eq!(j.posted_at.as_deref(), Some("2026-04-23"));
    }

    #[test]
    fn remote_falls_back_to_location_text() {
        let v = json!({ "results": [
            { "id": "1", "role": "Dev", "location": "Remote - EU", "remote": false }
        ]});
        let jobs = parse(&v);
        assert!(
            jobs[0].remote,
            "location 'Remote - EU' should still flag remote"
        );
        // A string id (non-numeric JSON) still resolves via the vstr fallback.
        assert_eq!(jobs[0].external_id, "1");
    }

    #[test]
    fn missing_results_yields_nothing() {
        assert!(parse(&json!({})).is_empty());
    }
}
