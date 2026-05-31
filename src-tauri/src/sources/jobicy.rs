//! Jobicy remote-jobs API (keyless). Attribute to Jobicy.com.
//! `GET https://jobicy.com/api/v2/remote-jobs?count=100`

use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::model::RawJob;
use crate::util::{clean_html, normalize_level, vstr};

pub async fn fetch(http: &Client, _cfg: &Value) -> Result<Vec<RawJob>> {
    let v: Value = http
        .get("https://jobicy.com/api/v2/remote-jobs?count=100")
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
            let Some(title) = vstr(j, "jobTitle") else {
                continue;
            };
            let employment_type = j
                .get("jobType")
                .and_then(|x| x.as_array())
                .and_then(|a| a.first())
                .and_then(|x| x.as_str())
                .map(|s| s.to_string())
                .or_else(|| vstr(j, "jobType"));
            // Jobicy reports seniority in `jobLevel` (string or array). Prefer it
            // over the title/description heuristic so e.g. a "Senior" role is not
            // mislabeled "Entry level" by boilerplate in the description.
            let experience_level = j
                .get("jobLevel")
                .and_then(|x| x.as_array())
                .and_then(|a| a.first())
                .and_then(|x| x.as_str())
                .map(|s| s.to_string())
                .or_else(|| vstr(j, "jobLevel"))
                .and_then(|s| normalize_level(&s));
            let description = vstr(j, "jobDescription")
                .map(|s| clean_html(&s))
                .or_else(|| vstr(j, "jobExcerpt").map(|s| clean_html(&s)))
                .unwrap_or_default();
            out.push(RawJob {
                external_id,
                title,
                company: vstr(j, "companyName").unwrap_or_default(),
                location: vstr(j, "jobGeo").unwrap_or_else(|| "Remote".into()),
                remote: true,
                employment_type,
                experience_level,
                description,
                url: vstr(j, "url").unwrap_or_default(),
                salary: salary(j),
                posted_at: vstr(j, "pubDate").map(|s| s.chars().take(10).collect()),
            });
        }
    }
    Ok(out)
}

fn salary(j: &Value) -> Option<String> {
    let min = j.get("annualSalaryMin").and_then(|x| x.as_i64());
    let max = j.get("annualSalaryMax").and_then(|x| x.as_i64());
    let cur = vstr(j, "salaryCurrency").unwrap_or_default();
    match (min, max) {
        (Some(a), Some(b)) => Some(format!("{cur} {a}-{b}").trim().to_string()),
        (Some(a), None) => Some(format!("{cur} {a}+").trim().to_string()),
        _ => None,
    }
}
