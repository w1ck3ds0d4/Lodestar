//! Remotive remote-jobs API (keyless). Polled politely; attribute to Remotive.
//! `GET https://remotive.com/api/remote-jobs?limit=100`

use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::model::RawJob;
use crate::util::{clean_html, vstr};

pub async fn fetch(http: &Client, _cfg: &Value) -> Result<Vec<RawJob>> {
    let v: Value = http
        .get("https://remotive.com/api/remote-jobs?limit=100")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

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
            out.push(RawJob {
                experience_level: None,
                external_id,
                title,
                company: vstr(j, "company_name").unwrap_or_default(),
                location: vstr(j, "candidate_required_location").unwrap_or_else(|| "Remote".into()),
                remote: true,
                employment_type: vstr(j, "job_type"),
                description: clean_html(&vstr(j, "description").unwrap_or_default()),
                url: vstr(j, "url").unwrap_or_default(),
                salary: vstr(j, "salary"),
                posted_at: vstr(j, "publication_date").map(|s| s.chars().take(10).collect()),
            });
        }
    }
    Ok(out)
}
