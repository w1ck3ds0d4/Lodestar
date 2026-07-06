//! RemoteOK public feed (keyless). `GET https://remoteok.com/api` returns a JSON
//! array whose first element is a legal/disclaimer object; the rest are remote jobs.
//! Needs a User-Agent header. Attribute to RemoteOK.

use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::model::RawJob;
use crate::util::{clean_html, vstr};

pub async fn fetch(http: &Client, _cfg: &Value) -> Result<Vec<RawJob>> {
    let v: Value = http
        .get("https://remoteok.com/api")
        .header(
            reqwest::header::USER_AGENT,
            "Lodestar/0.1 (+https://github.com/w1ck3ds0d4/Lodestar)",
        )
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(parse(&v))
}

/// Map the RemoteOK array into RawJobs (pure; no I/O). The leading legal object and
/// any entry without an id + position are skipped. All roles are remote.
fn parse(v: &Value) -> Vec<RawJob> {
    let mut out = Vec::new();
    let Some(arr) = v.as_array() else {
        return out;
    };
    for j in arr {
        // RemoteOK ids are strings; the first (legal) element has no id and is skipped.
        let external_id = match j.get("id") {
            Some(Value::String(s)) if !s.is_empty() => s.clone(),
            Some(Value::Number(n)) => n.to_string(),
            _ => continue,
        };
        let Some(title) = vstr(j, "position") else {
            continue;
        };
        out.push(RawJob {
            experience_level: None,
            external_id,
            title,
            company: vstr(j, "company").unwrap_or_default(),
            location: vstr(j, "location").unwrap_or_else(|| "Remote".into()),
            remote: true,
            employment_type: None,
            description: clean_html(&vstr(j, "description").unwrap_or_default()),
            url: vstr(j, "url").unwrap_or_default(),
            salary: salary(j),
            posted_at: vstr(j, "date").map(|s| s.chars().take(10).collect()),
        });
    }
    out
}

/// Build a salary string from RemoteOK's numeric `salary_min`/`salary_max` (USD/yr),
/// ignoring the common 0 placeholders.
fn salary(j: &Value) -> Option<String> {
    let n = |k: &str| j.get(k).and_then(|x| x.as_i64()).filter(|v| *v > 0);
    match (n("salary_min"), n("salary_max")) {
        (Some(a), Some(b)) => Some(format!("${a}-${b}")),
        (Some(a), None) => Some(format!("${a}+")),
        (None, Some(b)) => Some(format!("up to ${b}")),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::parse;
    use serde_json::json;

    #[test]
    fn skips_legal_object_and_maps_jobs() {
        let v = json!([
            { "legal": "RemoteOK API legal notice" },
            {
                "id": "1099999",
                "company": "Globex",
                "position": "Senior Rust Engineer",
                "location": "Worldwide",
                "description": "<p>Build &amp; ship</p>",
                "url": "https://remoteok.com/remote-jobs/1099999",
                "date": "2024-05-01T12:00:00+00:00",
                "salary_min": 120000,
                "salary_max": 160000
            },
            { "id": "5", "company": "NoTitle Inc" } // skipped: no position
        ]);
        let jobs = parse(&v);
        assert_eq!(jobs.len(), 1);
        let j = &jobs[0];
        assert_eq!(j.external_id, "1099999");
        assert_eq!(j.title, "Senior Rust Engineer");
        assert_eq!(j.company, "Globex");
        assert!(j.remote);
        assert_eq!(j.location, "Worldwide");
        assert_eq!(j.description, "Build & ship");
        assert_eq!(j.salary.as_deref(), Some("$120000-$160000"));
        assert_eq!(j.posted_at.as_deref(), Some("2024-05-01"));
    }

    #[test]
    fn salary_handles_zero_placeholders() {
        let v = json!([{ "id": "1", "position": "Dev", "salary_min": 0, "salary_max": 0 }]);
        assert_eq!(parse(&v)[0].salary, None);
    }

    #[test]
    fn non_array_yields_nothing() {
        assert!(parse(&json!({ "error": "nope" })).is_empty());
    }
}
