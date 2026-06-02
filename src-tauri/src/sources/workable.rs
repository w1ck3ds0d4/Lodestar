//! Workable public job-board widget (keyless). Config: { "account": "<slug>" } - the
//! `<account>` in apply.workable.com/<account>.
//! `GET https://apply.workable.com/api/v1/widget/accounts/<account>?details=true`

use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::model::RawJob;
use crate::util::{cfg_str, clean_html, vstr};

pub async fn fetch(http: &Client, cfg: &Value) -> Result<Vec<RawJob>> {
    let account = cfg_str(cfg, "account")?;
    let url = format!("https://apply.workable.com/api/v1/widget/accounts/{account}?details=true");
    let v: Value = http
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(parse(&v))
}

/// Map the Workable widget payload into RawJobs (pure). The account's display `name`
/// is the company; each job's `shortcode` is the external id (there is no `id`).
fn parse(v: &Value) -> Vec<RawJob> {
    let company = vstr(v, "name").unwrap_or_default();
    let mut out = Vec::new();
    let Some(arr) = v.get("jobs").and_then(|x| x.as_array()) else {
        return out;
    };
    for j in arr {
        let Some(external_id) = vstr(j, "shortcode") else {
            continue;
        };
        let Some(title) = vstr(j, "title") else {
            continue;
        };
        let telecommuting = j
            .get("telecommuting")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let location = [vstr(j, "city"), vstr(j, "country")]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(", ");
        out.push(RawJob {
            experience_level: None,
            external_id,
            title,
            company: company.clone(),
            remote: telecommuting,
            location: if location.is_empty() && telecommuting {
                "Remote".into()
            } else {
                location
            },
            employment_type: vstr(j, "employment_type"),
            description: clean_html(&vstr(j, "description").unwrap_or_default()),
            url: vstr(j, "application_url")
                .or_else(|| vstr(j, "url"))
                .unwrap_or_default(),
            salary: None,
            posted_at: vstr(j, "published_on")
                .or_else(|| vstr(j, "created_at"))
                .map(|s| s.chars().take(10).collect()),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::parse;
    use serde_json::json;

    #[test]
    fn maps_widget_jobs() {
        let v = json!({
            "name": "Cleafy",
            "jobs": [
                {
                    "shortcode": "5A2BC64797",
                    "title": "Full Stack Engineer",
                    "telecommuting": true,
                    "city": "Milan",
                    "country": "Italy",
                    "employment_type": "Full-time",
                    "description": "<p>Build &amp; ship</p>",
                    "url": "https://apply.workable.com/j/5A2BC64797",
                    "application_url": "https://apply.workable.com/j/5A2BC64797/apply",
                    "published_on": "2026-02-12"
                },
                { "title": "No shortcode" } // skipped
            ]
        });
        let jobs = parse(&v);
        assert_eq!(jobs.len(), 1);
        let j = &jobs[0];
        assert_eq!(j.external_id, "5A2BC64797");
        assert_eq!(j.company, "Cleafy");
        assert!(j.remote);
        assert_eq!(j.location, "Milan, Italy");
        assert_eq!(j.description, "Build & ship");
        assert_eq!(j.url, "https://apply.workable.com/j/5A2BC64797/apply");
        assert_eq!(j.posted_at.as_deref(), Some("2026-02-12"));
    }

    #[test]
    fn empty_or_missing_jobs_yields_nothing() {
        assert!(parse(&json!({ "name": "X" })).is_empty());
    }
}
