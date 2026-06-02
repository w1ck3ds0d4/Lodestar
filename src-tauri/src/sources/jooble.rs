//! Jooble aggregator API (free key by request). `POST https://jooble.org/api/<key>`
//! with a JSON body `{ keywords, location }` returns `{ totalCount, jobs: [...] }`.
//! The key is part of the URL path, so any error text is scrubbed of it before it
//! can reach logs or the source's last-status. Config:
//! `{ "api_key": "...", "keywords": "...", "location": "..." }`.

use anyhow::Result;
use reqwest::Client;
use serde_json::{json, Value};

use crate::model::RawJob;
use crate::util::{cfg_opt, cfg_str, clean_html, looks_remote, stable_hash, vstr};

pub async fn fetch(http: &Client, cfg: &Value) -> Result<Vec<RawJob>> {
    let key = cfg_str(cfg, "api_key")?;
    let body = json!({
        "keywords": cfg_opt(cfg, "keywords").unwrap_or_default(),
        "location": cfg_opt(cfg, "location").unwrap_or_default(),
    });
    // The key lives in the path, so scrub it from every error this call can produce.
    let scrub = |e: reqwest::Error| anyhow::anyhow!(e.to_string().replace(&key, "***"));

    let v: Value = http
        .post(format!("https://jooble.org/api/{key}"))
        .json(&body)
        .send()
        .await
        .map_err(scrub)?
        .error_for_status()
        .map_err(scrub)?
        .json()
        .await
        .map_err(scrub)?;
    Ok(parse(&v))
}

/// Map the Jooble response into RawJobs (pure). `title`/`company`/`location` are the
/// basics, `snippet` is an HTML excerpt (Jooble does not return the full body), `type`
/// is the employment type, `link` the apply URL, `salary` a pre-formatted string, and
/// `updated` an ISO timestamp.
fn parse(v: &Value) -> Vec<RawJob> {
    let mut out = Vec::new();
    let Some(arr) = v.get("jobs").and_then(Value::as_array) else {
        return out;
    };
    for j in arr {
        let Some(title) = vstr(j, "title") else {
            continue;
        };
        let location = vstr(j, "location").unwrap_or_default();
        out.push(RawJob {
            experience_level: None,
            external_id: j
                .get("id")
                .and_then(Value::as_i64)
                .map(|n| n.to_string())
                .or_else(|| vstr(j, "link"))
                .unwrap_or_else(|| stable_hash(&title)),
            remote: looks_remote(&title) || looks_remote(&location),
            title,
            company: vstr(j, "company").unwrap_or_default(),
            location,
            employment_type: vstr(j, "type"),
            description: clean_html(&vstr(j, "snippet").unwrap_or_default()),
            url: vstr(j, "link").unwrap_or_default(),
            salary: vstr(j, "salary"),
            // "2026-04-23T00:00:00.0000000" -> "2026-04-23"
            posted_at: vstr(j, "updated").map(|s| s.chars().take(10).collect()),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::parse;
    use serde_json::json;

    #[test]
    fn maps_jobs_with_snippet_and_salary() {
        let v = json!({
            "totalCount": 1,
            "jobs": [
                {
                    "title": "Remote Python Developer",
                    "location": "Anywhere",
                    "snippet": "<b>Django</b> &amp; FastAPI",
                    "salary": "$120,000 per year",
                    "source": "stackoverflow.com",
                    "type": "Full-time",
                    "link": "https://jooble.org/desc/123",
                    "company": "Globex",
                    "updated": "2026-04-23T00:00:00.0000000",
                    "id": 987654
                },
                { "company": "NoTitle" } // skipped
            ]
        });
        let jobs = parse(&v);
        assert_eq!(jobs.len(), 1);
        let j = &jobs[0];
        assert_eq!(j.external_id, "987654");
        assert_eq!(j.title, "Remote Python Developer");
        assert_eq!(j.company, "Globex");
        assert!(j.remote);
        assert_eq!(j.employment_type.as_deref(), Some("Full-time"));
        assert_eq!(j.description, "Django & FastAPI");
        assert_eq!(j.salary.as_deref(), Some("$120,000 per year"));
        assert_eq!(j.url, "https://jooble.org/desc/123");
        assert_eq!(j.posted_at.as_deref(), Some("2026-04-23"));
    }

    #[test]
    fn id_falls_back_to_link() {
        let v = json!({ "jobs": [
            { "title": "Dev", "link": "https://jooble.org/desc/abc" }
        ]});
        let jobs = parse(&v);
        assert_eq!(jobs[0].external_id, "https://jooble.org/desc/abc");
    }

    #[test]
    fn missing_jobs_yields_nothing() {
        assert!(parse(&json!({})).is_empty());
    }
}
