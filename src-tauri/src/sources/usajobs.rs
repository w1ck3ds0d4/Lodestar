//! USAJOBS search API (free key required). Config:
//! `{ "email": "you@example.com", "api_key": "...", "keyword": "..." }`

use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::model::RawJob;
use crate::util::{cfg_opt, cfg_str, clean_html, looks_remote, stable_hash, vstr};

pub async fn fetch(http: &Client, cfg: &Value) -> Result<Vec<RawJob>> {
    let email = cfg_str(cfg, "email")?;
    let api_key = cfg_str(cfg, "api_key")?;
    let keyword = cfg_opt(cfg, "keyword").unwrap_or_default();

    let mut query: Vec<(&str, String)> = vec![("ResultsPerPage", "50".to_string())];
    if !keyword.is_empty() {
        query.push(("Keyword", keyword));
    }

    let v: Value = http
        .get("https://data.usajobs.gov/api/search")
        .query(&query)
        .header("Host", "data.usajobs.gov")
        .header("User-Agent", email)
        .header("Authorization-Key", api_key)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let mut out = Vec::new();
    let items = v
        .get("SearchResult")
        .and_then(|s| s.get("SearchResultItems"))
        .and_then(|x| x.as_array());
    if let Some(arr) = items {
        for item in arr {
            let Some(d) = item.get("MatchedObjectDescriptor") else {
                continue;
            };
            let Some(title) = vstr(d, "PositionTitle") else {
                continue;
            };
            let location = vstr(d, "PositionLocationDisplay").unwrap_or_default();
            let external_id = vstr(item, "MatchedObjectId").unwrap_or_else(|| {
                stable_hash(&format!(
                    "{title}|{}",
                    vstr(d, "PositionURI").unwrap_or_default()
                ))
            });
            let description = d
                .get("UserArea")
                .and_then(|u| u.get("Details"))
                .and_then(|x| vstr(x, "JobSummary"))
                .map(|s| clean_html(&s))
                .unwrap_or_default();
            out.push(RawJob {
                external_id,
                title,
                company: vstr(d, "OrganizationName")
                    .or_else(|| vstr(d, "DepartmentName"))
                    .unwrap_or_default(),
                remote: looks_remote(&location),
                location,
                employment_type: d
                    .get("PositionSchedule")
                    .and_then(|x| x.as_array())
                    .and_then(|a| a.first())
                    .and_then(|x| vstr(x, "Name")),
                description,
                url: vstr(d, "PositionURI").unwrap_or_default(),
                salary: salary(d),
                posted_at: vstr(d, "PublicationStartDate").map(|s| s.chars().take(10).collect()),
            });
        }
    }
    Ok(out)
}

fn salary(d: &Value) -> Option<String> {
    let r = d
        .get("PositionRemuneration")
        .and_then(|x| x.as_array())
        .and_then(|a| a.first())?;
    let min = vstr(r, "MinimumRange").unwrap_or_default();
    let max = vstr(r, "MaximumRange").unwrap_or_default();
    let unit = vstr(r, "RateIntervalCode").unwrap_or_default();
    if min.is_empty() && max.is_empty() {
        return None;
    }
    Some(format!("{min}-{max} {unit}").trim().to_string())
}
