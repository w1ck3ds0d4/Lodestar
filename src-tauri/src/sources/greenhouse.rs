//! Greenhouse public job board API.
//! `GET https://boards-api.greenhouse.io/v1/boards/{company}/jobs?content=true`

use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::model::RawJob;
use crate::util::{cfg_str, clean_html, looks_remote, vstr};

pub async fn fetch(http: &Client, cfg: &Value) -> Result<Vec<RawJob>> {
    let company = cfg_str(cfg, "company")?;
    let url = format!("https://boards-api.greenhouse.io/v1/boards/{company}/jobs?content=true");
    let v: Value = http
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(parse(&v, &company))
}

/// Map a Greenhouse `jobs` payload into RawJobs (pure; no I/O). Entries missing an
/// id or title are skipped.
fn parse(v: &Value, company: &str) -> Vec<RawJob> {
    let mut out = Vec::new();
    if let Some(arr) = v.get("jobs").and_then(|x| x.as_array()) {
        for j in arr {
            let external_id = match j.get("id") {
                Some(id) if !id.is_null() => id.to_string(),
                _ => continue,
            };
            let Some(title) = vstr(j, "title") else {
                continue;
            };
            let location = j
                .get("location")
                .and_then(|l| vstr(l, "name"))
                .unwrap_or_default();
            out.push(RawJob {
                experience_level: None,
                external_id,
                title,
                company: company.to_string(),
                remote: looks_remote(&location),
                location,
                employment_type: None,
                description: clean_html(&vstr(j, "content").unwrap_or_default()),
                url: vstr(j, "absolute_url").unwrap_or_default(),
                salary: None,
                posted_at: vstr(j, "updated_at").map(|s| s.chars().take(10).collect()),
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::parse;
    use serde_json::json;

    #[test]
    fn maps_jobs_and_skips_invalid_entries() {
        let v = json!({
            "jobs": [
                {
                    "id": 123,
                    "title": "Senior Engineer",
                    "location": { "name": "Remote - US" },
                    "content": "<p>Build &amp; ship</p>",
                    "absolute_url": "https://example.com/123",
                    "updated_at": "2024-05-01T12:00:00Z"
                },
                { "title": "No id" }, // skipped: missing id
                { "id": 9 }           // skipped: missing title
            ]
        });
        let jobs = parse(&v, "Acme");
        assert_eq!(jobs.len(), 1);
        let j = &jobs[0];
        assert_eq!(j.external_id, "123");
        assert_eq!(j.title, "Senior Engineer");
        assert_eq!(j.company, "Acme");
        assert!(j.remote); // "Remote - US" reads as remote
        assert_eq!(j.location, "Remote - US");
        assert_eq!(j.description, "Build & ship"); // tags stripped, entity decoded
        assert_eq!(j.url, "https://example.com/123");
        assert_eq!(j.posted_at.as_deref(), Some("2024-05-01")); // first 10 chars
    }

    #[test]
    fn missing_or_empty_jobs_yields_nothing() {
        assert!(parse(&json!({}), "Acme").is_empty());
        assert!(parse(&json!({ "jobs": [] }), "Acme").is_empty());
    }
}
