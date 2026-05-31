//! Adzuna API (free key required). Config:
//! `{ "app_id": "...", "app_key": "...", "country": "gb", "what": "...", "where": "..." }`

use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::model::RawJob;
use crate::util::{cfg_opt, cfg_str, clean_html, looks_remote, vstr};

pub async fn fetch(http: &Client, cfg: &Value) -> Result<Vec<RawJob>> {
    let app_id = cfg_str(cfg, "app_id")?;
    let app_key = cfg_str(cfg, "app_key")?;
    let country = cfg_opt(cfg, "country").unwrap_or_else(|| "gb".into());
    let what = cfg_opt(cfg, "what").unwrap_or_default();
    let where_ = cfg_opt(cfg, "where").unwrap_or_default();

    let url = format!("https://api.adzuna.com/v1/api/jobs/{country}/search/1");
    let mut query: Vec<(&str, String)> = vec![
        ("app_id", app_id),
        ("app_key", app_key),
        ("results_per_page", "50".to_string()),
        ("content-type", "application/json".to_string()),
    ];
    if !what.is_empty() {
        query.push(("what", what));
    }
    if !where_.is_empty() {
        query.push(("where", where_));
    }

    let v: Value = http
        .get(&url)
        .query(&query)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let mut out = Vec::new();
    if let Some(arr) = v.get("results").and_then(|x| x.as_array()) {
        for j in arr {
            let Some(external_id) = vstr(j, "id") else {
                continue;
            };
            let title = clean_html(&vstr(j, "title").unwrap_or_default());
            if title.is_empty() {
                continue;
            }
            let company = j
                .get("company")
                .and_then(|c| vstr(c, "display_name"))
                .unwrap_or_default();
            let location = j
                .get("location")
                .and_then(|l| vstr(l, "display_name"))
                .unwrap_or_default();
            out.push(RawJob {
                experience_level: None,
                external_id,
                title,
                remote: looks_remote(&location),
                company,
                location,
                employment_type: vstr(j, "contract_time"),
                description: clean_html(&vstr(j, "description").unwrap_or_default()),
                url: vstr(j, "redirect_url").unwrap_or_default(),
                salary: salary(j),
                posted_at: vstr(j, "created").map(|s| s.chars().take(10).collect()),
            });
        }
    }
    Ok(out)
}

fn salary(j: &Value) -> Option<String> {
    let min = j.get("salary_min").and_then(|x| x.as_f64());
    let max = j.get("salary_max").and_then(|x| x.as_f64());
    match (min, max) {
        (Some(a), Some(b)) => Some(format!("{}-{}", a as i64, b as i64)),
        (Some(a), None) => Some(format!("{}+", a as i64)),
        _ => None,
    }
}
