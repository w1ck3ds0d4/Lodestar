//! SmartRecruiters public postings (keyless). Config: { "company": "<companyId>" }.
//! `GET https://api.smartrecruiters.com/v1/companies/<company>/postings` (paginated).
//! The list endpoint is summary-only - it carries no full description.

use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::model::RawJob;
use crate::util::{cfg_str, normalize_level, vstr};

const PAGE: usize = 100;
const MAX_PAGES: usize = 5;

pub async fn fetch(http: &Client, cfg: &Value) -> Result<Vec<RawJob>> {
    let company = cfg_str(cfg, "company")?;
    let mut out = Vec::new();
    for page in 0..MAX_PAGES {
        let offset = page * PAGE;
        let url = format!(
            "https://api.smartrecruiters.com/v1/companies/{company}/postings?limit={PAGE}&offset={offset}"
        );
        let v: Value = http
            .get(&url)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;
        let got = v
            .get("content")
            .and_then(Value::as_array)
            .map(|a| a.len())
            .unwrap_or(0);
        out.extend(parse(&v, &company));
        if got < PAGE {
            break;
        }
    }
    Ok(out)
}

/// Map one page of SmartRecruiters postings into RawJobs (pure). The list is
/// summary-only, so `description` is left empty; the candidate URL is built from the
/// posting's company identifier and id.
fn parse(v: &Value, company: &str) -> Vec<RawJob> {
    let mut out = Vec::new();
    let Some(arr) = v.get("content").and_then(|x| x.as_array()) else {
        return out;
    };
    for p in arr {
        let Some(external_id) = vstr(p, "id") else {
            continue;
        };
        let Some(title) = vstr(p, "name") else {
            continue;
        };
        let loc = p.get("location");
        let remote = loc
            .and_then(|l| l.get("remote"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let location = loc
            .map(|l| {
                [vstr(l, "city"), vstr(l, "region")]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(", ")
            })
            .unwrap_or_default();
        let identifier = p
            .get("company")
            .and_then(|c| vstr(c, "identifier"))
            .unwrap_or_else(|| company.to_string());
        out.push(RawJob {
            experience_level: p
                .get("experienceLevel")
                .and_then(|e| vstr(e, "label"))
                .and_then(|s| normalize_level(&s)),
            url: format!("https://jobs.smartrecruiters.com/{identifier}/{external_id}"),
            external_id,
            title,
            company: p
                .get("company")
                .and_then(|c| vstr(c, "name"))
                .unwrap_or_else(|| company.to_string()),
            remote,
            location,
            employment_type: p.get("typeOfEmployment").and_then(|t| vstr(t, "label")),
            description: String::new(),
            salary: None,
            posted_at: vstr(p, "releasedDate").map(|s| s.chars().take(10).collect()),
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::parse;
    use serde_json::json;

    #[test]
    fn maps_postings_and_builds_url() {
        let v = json!({
            "content": [
                {
                    "id": "744000129815717",
                    "name": "Product Designer",
                    "company": { "identifier": "acme", "name": "Acme Inc" },
                    "releasedDate": "2026-06-02T16:50:18.032Z",
                    "location": { "city": "London", "region": "REMOTE", "country": "gb", "remote": true },
                    "typeOfEmployment": { "label": "Full-time" },
                    "experienceLevel": { "label": "Mid-Senior Level" }
                },
                { "name": "No id" } // skipped
            ],
            "totalFound": 1
        });
        let jobs = parse(&v, "Acme");
        assert_eq!(jobs.len(), 1);
        let j = &jobs[0];
        assert_eq!(j.external_id, "744000129815717");
        assert_eq!(j.title, "Product Designer");
        assert_eq!(j.company, "Acme Inc");
        assert!(j.remote);
        assert_eq!(j.location, "London, REMOTE");
        assert_eq!(
            j.url,
            "https://jobs.smartrecruiters.com/acme/744000129815717"
        );
        assert_eq!(j.employment_type.as_deref(), Some("Full-time"));
        assert_eq!(j.experience_level.as_deref(), Some("senior")); // "Mid-Senior" -> senior
        assert_eq!(j.posted_at.as_deref(), Some("2026-06-02"));
    }

    #[test]
    fn empty_content_yields_nothing() {
        assert!(parse(&json!({ "content": [], "totalFound": 0 }), "Acme").is_empty());
    }
}
