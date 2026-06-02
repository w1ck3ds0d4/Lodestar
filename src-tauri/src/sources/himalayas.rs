//! Himalayas remote-jobs feed (keyless). `GET https://himalayas.app/jobs/api`
//! returns `{ jobs: [...], totalCount, offset, limit }`. All roles are remote.

use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::model::RawJob;
use crate::util::{clean_html, epoch_to_date, normalize_level, stable_hash, vstr};

const PAGE: u64 = 100;
const MAX_PAGES: u64 = 2;

pub async fn fetch(http: &Client, _cfg: &Value) -> Result<Vec<RawJob>> {
    let mut out = Vec::new();
    for page in 0..MAX_PAGES {
        let offset = page * PAGE;
        let url = format!("https://himalayas.app/jobs/api?limit={PAGE}&offset={offset}");
        let v: Value = http
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let got = v
            .get("jobs")
            .and_then(Value::as_array)
            .map(|a| a.len())
            .unwrap_or(0);
        out.extend(parse(&v));
        if (got as u64) < PAGE {
            break;
        }
    }
    Ok(out)
}

/// Map one page of the Himalayas feed into RawJobs (pure). `guid` is the canonical id
/// and apply URL; `pubDate` is a Unix epoch (seconds); `locationRestrictions` lists
/// the allowed regions, which feeds remote-eligibility detection downstream.
fn parse(v: &Value) -> Vec<RawJob> {
    let mut out = Vec::new();
    let Some(arr) = v.get("jobs").and_then(|x| x.as_array()) else {
        return out;
    };
    for j in arr {
        let Some(title) = vstr(j, "title") else {
            continue;
        };
        let guid = vstr(j, "guid");
        let locations = j
            .get("locationRestrictions")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        out.push(RawJob {
            experience_level: j
                .get("seniority")
                .and_then(Value::as_array)
                .and_then(|a| a.first())
                .and_then(Value::as_str)
                .and_then(normalize_level),
            external_id: guid.clone().unwrap_or_else(|| stable_hash(&title)),
            title,
            company: vstr(j, "companyName").unwrap_or_default(),
            remote: true,
            location: if locations.is_empty() {
                "Remote".into()
            } else {
                locations
            },
            employment_type: vstr(j, "employmentType"),
            description: clean_html(&vstr(j, "description").unwrap_or_default()),
            url: vstr(j, "applicationLink").or(guid).unwrap_or_default(),
            salary: salary(j),
            posted_at: j.get("pubDate").and_then(Value::as_i64).map(epoch_to_date),
        });
    }
    out
}

/// Build a salary string from Himalayas' numeric min/max + currency (ignoring 0s),
/// in a form the salary parser understands.
fn salary(j: &Value) -> Option<String> {
    let cur = vstr(j, "currency").unwrap_or_default();
    let n = |k: &str| j.get(k).and_then(Value::as_i64).filter(|v| *v > 0);
    match (n("minSalary"), n("maxSalary")) {
        (Some(a), Some(b)) => Some(format!("{cur} {a}-{b}").trim().to_string()),
        (Some(a), None) => Some(format!("{cur} {a}").trim().to_string()),
        (None, Some(b)) => Some(format!("{cur} up to {b}").trim().to_string()),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::parse;
    use serde_json::json;

    #[test]
    fn maps_jobs_with_epoch_date_and_salary() {
        let v = json!({
            "jobs": [
                {
                    "title": "Backend Engineer",
                    "companyName": "Globex",
                    "guid": "https://himalayas.app/jobs/backend-engineer",
                    "locationRestrictions": ["United States", "Canada"],
                    "description": "<p>Ship &amp; scale</p>",
                    "applicationLink": "https://globex.example/apply",
                    "employmentType": "Full Time",
                    "minSalary": 120000,
                    "maxSalary": 160000,
                    "currency": "USD",
                    "seniority": ["Senior"],
                    "pubDate": 1700000000
                },
                { "companyName": "NoTitle" } // skipped
            ],
            "totalCount": 1
        });
        let jobs = parse(&v);
        assert_eq!(jobs.len(), 1);
        let j = &jobs[0];
        assert_eq!(j.external_id, "https://himalayas.app/jobs/backend-engineer");
        assert_eq!(j.company, "Globex");
        assert!(j.remote);
        assert_eq!(j.location, "United States, Canada");
        assert_eq!(j.description, "Ship & scale");
        assert_eq!(j.url, "https://globex.example/apply");
        assert_eq!(j.salary.as_deref(), Some("USD 120000-160000"));
        assert_eq!(j.experience_level.as_deref(), Some("senior"));
        assert_eq!(j.posted_at.as_deref(), Some("2023-11-14"));
    }

    #[test]
    fn missing_jobs_yields_nothing() {
        assert!(parse(&json!({})).is_empty());
    }
}
