//! Recruitee public careers offers (keyless). Config: { "company": "<subdomain>" } -
//! the `<company>` in <company>.recruitee.com.
//! `GET https://<company>.recruitee.com/api/offers/`

use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::model::RawJob;
use crate::util::{cfg_str, clean_html, vstr};

pub async fn fetch(http: &Client, cfg: &Value) -> Result<Vec<RawJob>> {
    let company = cfg_str(cfg, "company")?;
    let url = format!("https://{company}.recruitee.com/api/offers/");
    let v: Value = http
        .get(&url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    Ok(parse(&v))
}

/// Map the Recruitee `offers` payload into RawJobs (pure). Only published offers are
/// kept; entries missing an id or title are skipped.
fn parse(v: &Value) -> Vec<RawJob> {
    let mut out = Vec::new();
    let Some(arr) = v.get("offers").and_then(|x| x.as_array()) else {
        return out;
    };
    for o in arr {
        if vstr(o, "status").is_some_and(|s| s != "published") {
            continue;
        }
        let external_id = match o.get("id") {
            Some(Value::Number(n)) => n.to_string(),
            Some(Value::String(s)) if !s.is_empty() => s.clone(),
            _ => continue,
        };
        let Some(title) = vstr(o, "title") else {
            continue;
        };
        let location = vstr(o, "location").unwrap_or_else(|| {
            [vstr(o, "city"), vstr(o, "country")]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(", ")
        });
        out.push(RawJob {
            experience_level: None,
            external_id,
            title,
            company: vstr(o, "company_name").unwrap_or_default(),
            remote: o.get("remote").and_then(Value::as_bool).unwrap_or(false),
            location,
            employment_type: vstr(o, "employment_type_code"),
            description: clean_html(&vstr(o, "description").unwrap_or_default()),
            url: vstr(o, "careers_url")
                .or_else(|| vstr(o, "careers_apply_url"))
                .unwrap_or_default(),
            salary: None,
            posted_at: vstr(o, "published_at").map(|s| s.chars().take(10).collect()),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::parse;
    use serde_json::json;

    #[test]
    fn maps_published_offers_and_skips_drafts() {
        let v = json!({
            "offers": [
                {
                    "id": 2625572,
                    "title": "Affiliate Partnerships Manager",
                    "description": "<p>Grow &amp; partner</p>",
                    "location": "Remote job",
                    "city": "Sao Paulo",
                    "country": "Brazil",
                    "country_code": "BR",
                    "careers_url": "https://acme.recruitee.com/o/affiliate",
                    "employment_type_code": "fulltime_permanent",
                    "remote": true,
                    "published_at": "2026-06-02 15:57:23 UTC",
                    "status": "published",
                    "company_name": "Acme A/S"
                },
                { "id": 9, "title": "Draft role", "status": "draft" } // skipped
            ]
        });
        let jobs = parse(&v);
        assert_eq!(jobs.len(), 1);
        let j = &jobs[0];
        assert_eq!(j.external_id, "2625572");
        assert_eq!(j.company, "Acme A/S");
        assert!(j.remote);
        assert_eq!(j.location, "Remote job");
        assert_eq!(j.description, "Grow & partner");
        assert_eq!(j.url, "https://acme.recruitee.com/o/affiliate");
        assert_eq!(j.posted_at.as_deref(), Some("2026-06-02"));
    }

    #[test]
    fn empty_or_missing_offers_yields_nothing() {
        assert!(parse(&json!({ "offers": [] })).is_empty());
        assert!(parse(&json!({})).is_empty());
    }
}
