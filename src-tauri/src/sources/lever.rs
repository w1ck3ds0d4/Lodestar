//! Lever public postings API.
//! `GET https://api.lever.co/v0/postings/{company}?mode=json`

use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::model::RawJob;
use crate::util::{cfg_str, clean_html, epoch_to_date, looks_remote, vstr};

pub async fn fetch(http: &Client, cfg: &Value) -> Result<Vec<RawJob>> {
    let company = cfg_str(cfg, "company")?;
    let url = format!("https://api.lever.co/v0/postings/{company}?mode=json");
    let v: Value = http
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let mut out = Vec::new();
    if let Some(arr) = v.as_array() {
        for j in arr {
            let Some(external_id) = vstr(j, "id") else {
                continue;
            };
            let Some(title) = vstr(j, "text") else {
                continue;
            };
            let cats = j.get("categories");
            let location = cats.and_then(|c| vstr(c, "location")).unwrap_or_default();
            let employment_type = cats.and_then(|c| vstr(c, "commitment"));
            let workplace = vstr(j, "workplaceType").unwrap_or_default();
            let remote = workplace.eq_ignore_ascii_case("remote") || looks_remote(&location);
            let description = vstr(j, "descriptionPlain")
                .or_else(|| {
                    j.get("description")
                        .and_then(|x| x.as_str())
                        .map(clean_html)
                })
                .unwrap_or_default();
            out.push(RawJob {
                experience_level: None,
                external_id,
                title,
                company: company.clone(),
                location,
                remote,
                employment_type,
                description,
                url: vstr(j, "hostedUrl")
                    .or_else(|| vstr(j, "applyUrl"))
                    .unwrap_or_default(),
                salary: salary(j.get("salaryRange")),
                posted_at: j
                    .get("createdAt")
                    .and_then(|x| x.as_i64())
                    .map(|ms| epoch_to_date(ms / 1000)),
            });
        }
    }
    Ok(out)
}

fn salary(v: Option<&Value>) -> Option<String> {
    let s = v?;
    let cur = vstr(s, "currency").unwrap_or_default();
    let min = s.get("min").and_then(|x| x.as_f64());
    let max = s.get("max").and_then(|x| x.as_f64());
    match (min, max) {
        (Some(a), Some(b)) => Some(
            format!("{cur} {}-{}", a as i64, b as i64)
                .trim()
                .to_string(),
        ),
        _ => None,
    }
}
