//! Working Nomads remote-jobs feed (keyless). `GET https://www.workingnomads.com/api/exposed_jobs/`
//! returns a JSON array of remote jobs. All roles are remote; `location` (e.g. "Remote,
//! USA" / "Anywhere") feeds remote-eligibility detection downstream. No config.

use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::model::RawJob;
use crate::util::{clean_html, stable_hash, vstr};

const ENDPOINT: &str = "https://www.workingnomads.com/api/exposed_jobs/";

pub async fn fetch(http: &Client, _cfg: &Value) -> Result<Vec<RawJob>> {
    let v: Value = http
        .get(ENDPOINT)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(parse(&v))
}

/// Map the Working Nomads array into RawJobs (pure). There's no id, so the (stable) job
/// `url` is the external id; `pub_date` is an ISO-8601 timestamp; `description` is HTML.
fn parse(v: &Value) -> Vec<RawJob> {
    let mut out = Vec::new();
    let Some(arr) = v.as_array() else {
        return out;
    };
    for j in arr {
        let Some(title) = vstr(j, "title") else {
            continue;
        };
        let url = vstr(j, "url").unwrap_or_default();
        let external_id = if url.is_empty() {
            stable_hash(&title)
        } else {
            url.clone()
        };
        out.push(RawJob {
            experience_level: None,
            external_id,
            title,
            company: vstr(j, "company_name").unwrap_or_default(),
            remote: true,
            location: vstr(j, "location").unwrap_or_else(|| "Remote".into()),
            employment_type: None,
            description: clean_html(&vstr(j, "description").unwrap_or_default()),
            url,
            salary: None,
            // "2026-06-04T16:42:20-04:00" -> "2026-06-04"
            posted_at: vstr(j, "pub_date").map(|s| s.chars().take(10).collect()),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::parse;
    use serde_json::json;

    #[test]
    fn maps_array_jobs() {
        let v = json!([
            {
                "url": "https://www.workingnomads.com/job/go/1645565/",
                "title": "AI Operations Manager",
                "description": "<p>Run ops &amp; automation</p>",
                "company_name": "Storm Ideas",
                "category_name": "Development",
                "tags": "operations,tech",
                "location": "Remote, USA",
                "pub_date": "2026-06-04T16:42:20-04:00"
            },
            { "company_name": "NoTitle" } // skipped
        ]);
        let jobs = parse(&v);
        assert_eq!(jobs.len(), 1);
        let j = &jobs[0];
        assert_eq!(
            j.external_id,
            "https://www.workingnomads.com/job/go/1645565/"
        );
        assert_eq!(j.title, "AI Operations Manager");
        assert_eq!(j.company, "Storm Ideas");
        assert!(j.remote);
        assert_eq!(j.location, "Remote, USA");
        assert_eq!(j.description, "Run ops & automation");
        assert_eq!(j.url, "https://www.workingnomads.com/job/go/1645565/");
        assert_eq!(j.posted_at.as_deref(), Some("2026-06-04"));
    }

    #[test]
    fn id_falls_back_to_hash_without_url() {
        let v = json!([{ "title": "Backend Engineer", "company_name": "Acme" }]);
        let jobs = parse(&v);
        assert_eq!(jobs.len(), 1);
        assert!(!jobs[0].external_id.is_empty());
        assert_eq!(jobs[0].location, "Remote");
    }

    #[test]
    fn non_array_yields_nothing() {
        assert!(parse(&json!({ "jobs": [] })).is_empty());
        assert!(parse(&json!(null)).is_empty());
    }
}
