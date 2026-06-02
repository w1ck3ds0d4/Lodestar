//! Reed.co.uk Jobseeker API (free key required). `GET https://www.reed.co.uk/api/1.0/search`
//! with HTTP Basic auth (the API key as the username, blank password) returns
//! `{ totalResults, results: [...] }`. The key rides in the Authorization header,
//! never the URL. Config:
//! `{ "api_key": "...", "keywords": "...", "location": "...", "remote": "true (optional)" }`.

use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::model::RawJob;
use crate::util::{cfg_opt, cfg_str, clean_html, looks_remote, vstr};

const ENDPOINT: &str = "https://www.reed.co.uk/api/1.0/search";

pub async fn fetch(http: &Client, cfg: &Value) -> Result<Vec<RawJob>> {
    let key = cfg_str(cfg, "api_key")?;
    let mut query: Vec<(&str, String)> = vec![("resultsToTake", "100".into())];
    if let Some(k) = cfg_opt(cfg, "keywords") {
        query.push(("keywords", k));
    }
    if let Some(l) = cfg_opt(cfg, "location") {
        query.push(("locationName", l));
    }

    let v: Value = http
        .get(ENDPOINT)
        .basic_auth(&key, Some(""))
        .query(&query)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(parse(&v))
}

/// Map the Reed search response into RawJobs (pure). `jobTitle`/`employerName`/`locationName`
/// are the basics, `minimumSalary`/`maximumSalary`/`currency` build the salary, `date` is the
/// posted date in dd/MM/yyyy, and `jobUrl` is the listing. Reed has no remote flag, so it is
/// inferred from the title/location.
fn parse(v: &Value) -> Vec<RawJob> {
    let mut out = Vec::new();
    let Some(arr) = v.get("results").and_then(Value::as_array) else {
        return out;
    };
    for j in arr {
        let Some(title) = vstr(j, "jobTitle") else {
            continue;
        };
        let id = j
            .get("jobId")
            .and_then(Value::as_i64)
            .map(|n| n.to_string())
            .unwrap_or_default();
        let location = vstr(j, "locationName").unwrap_or_default();
        let url = vstr(j, "jobUrl")
            .or_else(|| vstr(j, "url"))
            .or_else(|| vstr(j, "externalUrl"))
            .unwrap_or_else(|| {
                if id.is_empty() {
                    String::new()
                } else {
                    format!("https://www.reed.co.uk/jobs/{id}")
                }
            });
        out.push(RawJob {
            experience_level: None,
            external_id: if id.is_empty() { url.clone() } else { id },
            remote: looks_remote(&title) || looks_remote(&location),
            title,
            company: vstr(j, "employerName").unwrap_or_default(),
            location,
            employment_type: None,
            description: clean_html(&vstr(j, "jobDescription").unwrap_or_default()),
            url,
            salary: salary(j),
            posted_at: vstr(j, "date").and_then(|d| reformat_date(&d)),
        });
    }
    out
}

/// Build a salary string from Reed's numeric min/max + currency (ignoring missing/0s),
/// in a form the salary parser understands.
fn salary(j: &Value) -> Option<String> {
    let cur = vstr(j, "currency").unwrap_or_default();
    let n = |k: &str| {
        j.get(k)
            .and_then(Value::as_f64)
            .filter(|v| *v > 0.0)
            .map(|v| v as i64)
    };
    match (n("minimumSalary"), n("maximumSalary")) {
        (Some(a), Some(b)) => Some(format!("{cur} {a}-{b}").trim().to_string()),
        (Some(a), None) => Some(format!("{cur} {a}").trim().to_string()),
        (None, Some(b)) => Some(format!("{cur} up to {b}").trim().to_string()),
        (None, None) => None,
    }
}

/// Reed posts dates as dd/MM/yyyy; convert to the ISO yyyy-MM-dd the rest of the app
/// stores. Returns None for anything that does not match that shape.
fn reformat_date(s: &str) -> Option<String> {
    let p: Vec<&str> = s.split('/').collect();
    if let [d, m, y] = p[..] {
        if d.len() == 2
            && m.len() == 2
            && y.len() == 4
            && p.iter().all(|x| x.chars().all(|c| c.is_ascii_digit()))
        {
            return Some(format!("{y}-{m}-{d}"));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{parse, reformat_date};
    use serde_json::json;

    #[test]
    fn maps_results_with_salary_and_date() {
        let v = json!({
            "totalResults": 1,
            "results": [
                {
                    "jobId": 41914309,
                    "employerName": "Acme Ltd",
                    "jobTitle": "Remote .NET Developer",
                    "locationName": "London",
                    "minimumSalary": 50000,
                    "maximumSalary": 65000,
                    "currency": "GBP",
                    "date": "23/04/2026",
                    "jobDescription": "<p>C# &amp; SQL</p>",
                    "jobUrl": "https://www.reed.co.uk/jobs/dotnet/41914309"
                },
                { "employerName": "NoTitle" } // skipped
            ]
        });
        let jobs = parse(&v);
        assert_eq!(jobs.len(), 1);
        let j = &jobs[0];
        assert_eq!(j.external_id, "41914309");
        assert_eq!(j.title, "Remote .NET Developer");
        assert_eq!(j.company, "Acme Ltd");
        assert!(j.remote, "title says Remote");
        assert_eq!(j.description, "C# & SQL");
        assert_eq!(j.salary.as_deref(), Some("GBP 50000-65000"));
        assert_eq!(j.url, "https://www.reed.co.uk/jobs/dotnet/41914309");
        assert_eq!(j.posted_at.as_deref(), Some("2026-04-23"));
    }

    #[test]
    fn url_falls_back_to_constructed_from_id() {
        let v = json!({ "results": [
            { "jobId": 99, "jobTitle": "Dev", "locationName": "Leeds" }
        ]});
        let jobs = parse(&v);
        assert_eq!(jobs[0].url, "https://www.reed.co.uk/jobs/99");
        assert!(!jobs[0].remote);
        assert!(jobs[0].salary.is_none());
    }

    #[test]
    fn reformat_date_only_accepts_ddmmyyyy() {
        assert_eq!(reformat_date("05/01/2026").as_deref(), Some("2026-01-05"));
        assert_eq!(reformat_date("2026-01-05"), None);
        assert_eq!(reformat_date(""), None);
    }
}
