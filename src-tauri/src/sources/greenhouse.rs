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
                company: company.clone(),
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
    Ok(out)
}
