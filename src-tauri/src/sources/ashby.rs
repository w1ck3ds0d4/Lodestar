//! Ashby public job board API.
//! `GET https://api.ashbyhq.com/posting-api/job-board/{name}?includeCompensation=true`

use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::model::RawJob;
use crate::util::{cfg_opt, cfg_str, clean_html, looks_remote, vstr};

pub async fn fetch(http: &Client, cfg: &Value) -> Result<Vec<RawJob>> {
    let name = cfg_str(cfg, "name")?;
    let company_label = cfg_opt(cfg, "company").unwrap_or_else(|| name.clone());
    let url =
        format!("https://api.ashbyhq.com/posting-api/job-board/{name}?includeCompensation=true");
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
            // Respect Ashby's listing flag: skip direct-link-only postings.
            if j.get("isListed").and_then(|x| x.as_bool()) == Some(false) {
                continue;
            }
            let Some(external_id) = vstr(j, "id") else {
                continue;
            };
            let Some(title) = vstr(j, "title") else {
                continue;
            };
            let location = vstr(j, "location").unwrap_or_default();
            let remote = j.get("isRemote").and_then(|x| x.as_bool()).unwrap_or(false)
                || looks_remote(&location);
            let description = vstr(j, "descriptionPlain")
                .or_else(|| {
                    j.get("descriptionHtml")
                        .and_then(|x| x.as_str())
                        .map(clean_html)
                })
                .unwrap_or_default();
            out.push(RawJob {
                external_id,
                title,
                company: company_label.clone(),
                location,
                remote,
                employment_type: vstr(j, "employmentType"),
                description,
                url: vstr(j, "jobUrl")
                    .or_else(|| vstr(j, "applyUrl"))
                    .unwrap_or_default(),
                salary: j
                    .get("compensation")
                    .and_then(|c| vstr(c, "compensationTierSummary")),
                posted_at: vstr(j, "publishedAt").map(|s| s.chars().take(10).collect()),
            });
        }
    }
    Ok(out)
}
