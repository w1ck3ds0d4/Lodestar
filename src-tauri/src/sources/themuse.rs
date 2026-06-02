//! The Muse public jobs API. `GET https://www.themuse.com/api/public/jobs`
//! returns `{ page, page_count, results: [...] }`. Works without a key (rate
//! limited); an optional `api_key` raises the limit. Config:
//! `{ "api_key": "(optional)", "category": "...", "location": "...", "level": "...", "company": "..." }`.

use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::model::RawJob;
use crate::util::{
    cfg_opt, clean_html, looks_remote, normalize_level, redact_secrets, stable_hash, vstr,
};

const ENDPOINT: &str = "https://www.themuse.com/api/public/jobs";
/// 20 results per page; fetch the newest few pages.
const MAX_PAGES: u64 = 3;

/// Wrap a reqwest error so the optional api_key in the URL query never leaks into
/// logs or the source's last-status message.
fn redact_err(e: reqwest::Error) -> anyhow::Error {
    anyhow::anyhow!(redact_secrets(&e.to_string()))
}

pub async fn fetch(http: &Client, cfg: &Value) -> Result<Vec<RawJob>> {
    let api_key = cfg_opt(cfg, "api_key");
    // Optional server-side narrowing; all are plain filter params on the endpoint.
    let filters: Vec<(&str, String)> = [
        ("category", cfg_opt(cfg, "category")),
        ("location", cfg_opt(cfg, "location")),
        ("level", cfg_opt(cfg, "level")),
        ("company", cfg_opt(cfg, "company")),
    ]
    .into_iter()
    .filter_map(|(k, v)| v.map(|v| (k, v)))
    .collect();

    let mut out = Vec::new();
    for page in 1..=MAX_PAGES {
        let mut query: Vec<(&str, String)> = vec![("page", page.to_string())];
        if let Some(k) = &api_key {
            query.push(("api_key", k.clone()));
        }
        query.extend(filters.iter().cloned());

        let resp = http
            .get(ENDPOINT)
            .query(&query)
            .send()
            .await
            .map_err(redact_err)?
            .error_for_status()
            .map_err(redact_err)?;
        let v: Value = resp.json().await?;

        let before = out.len();
        out.extend(parse(&v));
        let page_count = v.get("page_count").and_then(Value::as_u64).unwrap_or(0);
        // Stop when a page adds nothing or we have walked every page.
        if out.len() == before || page >= page_count {
            break;
        }
    }
    Ok(out)
}

/// Map one page of The Muse feed into RawJobs (pure). `name` is the title, `company.name`
/// the employer, `refs.landing_page` the apply URL, `levels[0].short_name` the seniority,
/// `locations[].name` the places ("Flexible / Remote" => remote), and `contents` the HTML body.
fn parse(v: &Value) -> Vec<RawJob> {
    let mut out = Vec::new();
    let Some(arr) = v.get("results").and_then(Value::as_array) else {
        return out;
    };
    for j in arr {
        let Some(title) = vstr(j, "name") else {
            continue;
        };
        let external_id = j
            .get("id")
            .and_then(Value::as_i64)
            .map(|n| n.to_string())
            .or_else(|| vstr(j, "short_name"))
            .unwrap_or_else(|| stable_hash(&title));
        let location = j
            .get("locations")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|l| vstr(l, "name"))
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        out.push(RawJob {
            experience_level: j
                .get("levels")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
                .and_then(|l| l.get("short_name").and_then(Value::as_str))
                .and_then(normalize_level),
            external_id,
            title,
            company: j
                .get("company")
                .and_then(|c| vstr(c, "name"))
                .unwrap_or_default(),
            remote: looks_remote(&location),
            location,
            employment_type: None,
            description: clean_html(&vstr(j, "contents").unwrap_or_default()),
            url: j
                .get("refs")
                .and_then(|r| vstr(r, "landing_page"))
                .unwrap_or_default(),
            salary: None,
            // "2026-04-23T00:10:34Z" -> "2026-04-23"
            posted_at: vstr(j, "publication_date").map(|s| s.chars().take(10).collect()),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::parse;
    use serde_json::json;

    #[test]
    fn maps_jobs_with_company_level_and_remote() {
        let v = json!({
            "page": 1,
            "page_count": 5,
            "results": [
                {
                    "id": 21191260,
                    "name": "Senior Backend Engineer",
                    "type": "external",
                    "publication_date": "2026-04-23T00:10:34Z",
                    "short_name": "senior-backend-engineer",
                    "company": { "id": 61, "short_name": "uber", "name": "Uber" },
                    "locations": [{ "name": "Flexible / Remote" }, { "name": "Lisbon, Portugal" }],
                    "levels": [{ "name": "Senior Level", "short_name": "senior" }],
                    "refs": { "landing_page": "https://www.themuse.com/jobs/uber/senior-backend-engineer" },
                    "contents": "<p>Build &amp; scale services</p>"
                },
                { "type": "external" } // no name -> skipped
            ]
        });
        let jobs = parse(&v);
        assert_eq!(jobs.len(), 1);
        let j = &jobs[0];
        assert_eq!(j.external_id, "21191260");
        assert_eq!(j.title, "Senior Backend Engineer");
        assert_eq!(j.company, "Uber");
        assert!(j.remote, "Flexible / Remote should flag remote");
        assert_eq!(j.location, "Flexible / Remote, Lisbon, Portugal");
        assert_eq!(j.experience_level.as_deref(), Some("senior"));
        assert_eq!(j.description, "Build & scale services");
        assert_eq!(
            j.url,
            "https://www.themuse.com/jobs/uber/senior-backend-engineer"
        );
        assert_eq!(j.posted_at.as_deref(), Some("2026-04-23"));
    }

    #[test]
    fn id_falls_back_to_short_name_then_hash() {
        let v = json!({ "results": [
            { "name": "Designer", "short_name": "designer-123" }
        ]});
        let jobs = parse(&v);
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].external_id, "designer-123");
        assert!(!jobs[0].remote);
    }

    #[test]
    fn missing_results_yields_nothing() {
        assert!(parse(&json!({})).is_empty());
    }
}
