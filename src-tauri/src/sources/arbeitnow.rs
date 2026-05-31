//! Arbeitnow job board API (keyless).
//! `GET https://www.arbeitnow.com/api/job-board-api`

use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::model::RawJob;
use crate::util::{clean_html, epoch_to_date, vstr};

pub async fn fetch(http: &Client, _cfg: &Value) -> Result<Vec<RawJob>> {
    let v: Value = http
        .get("https://www.arbeitnow.com/api/job-board-api")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let mut out = Vec::new();
    if let Some(arr) = v.get("data").and_then(|x| x.as_array()) {
        for j in arr {
            let Some(external_id) = vstr(j, "slug") else {
                continue;
            };
            let Some(title) = vstr(j, "title") else {
                continue;
            };
            let employment_type = j
                .get("job_types")
                .and_then(|x| x.as_array())
                .and_then(|a| a.first())
                .and_then(|x| x.as_str())
                .map(|s| s.to_string());
            out.push(RawJob {
                experience_level: None,
                external_id,
                title,
                company: vstr(j, "company_name").unwrap_or_default(),
                location: vstr(j, "location").unwrap_or_default(),
                remote: j.get("remote").and_then(|x| x.as_bool()).unwrap_or(false),
                employment_type,
                description: clean_html(&vstr(j, "description").unwrap_or_default()),
                url: vstr(j, "url").unwrap_or_default(),
                salary: None,
                posted_at: j
                    .get("created_at")
                    .and_then(|x| x.as_i64())
                    .map(epoch_to_date),
            });
        }
    }
    Ok(out)
}
