//! We Work Remotely RSS (keyless, all roles remote). Item titles are "Company: Role",
//! and WWR adds `<region>` and `<type>` elements. Config:
//! `{ "category": "(optional, e.g. remote-programming-jobs)" }` selects a category feed;
//! empty uses the all-jobs feed.

use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::model::RawJob;
use crate::sources::feed;
use crate::util::{cfg_opt, clean_html, stable_hash};

const ALL: &str = "https://weworkremotely.com/remote-jobs.rss";

pub async fn fetch(http: &Client, cfg: &Value) -> Result<Vec<RawJob>> {
    let url = match cfg_opt(cfg, "category") {
        Some(c) if !c.is_empty() => format!("https://weworkremotely.com/categories/{c}.rss"),
        _ => ALL.to_string(),
    };
    let xml = http
        .get(&url)
        .header("Accept", "application/rss+xml, application/xml;q=0.9")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;
    Ok(map(feed::parse(&xml)))
}

/// Map WWR feed items into RawJobs (pure). The title is split on the first ": " into
/// company + role; every WWR posting is remote, and `<region>` becomes the location so
/// remote-eligibility detection downstream can tell "Anywhere" from "USA Only".
fn map(items: Vec<feed::FeedItem>) -> Vec<RawJob> {
    items
        .into_iter()
        .filter_map(|it| {
            let raw = it.title.trim();
            if raw.is_empty() {
                return None;
            }
            let (company, title) = match raw.split_once(": ") {
                Some((c, t)) if !t.trim().is_empty() => {
                    (c.trim().to_string(), t.trim().to_string())
                }
                _ => (String::new(), raw.to_string()),
            };
            let url = if !it.guid.is_empty() {
                it.guid.clone()
            } else {
                it.link.clone()
            };
            let region = it
                .extra
                .get("region")
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            Some(RawJob {
                experience_level: None,
                external_id: if url.is_empty() {
                    stable_hash(raw)
                } else {
                    url.clone()
                },
                title,
                company,
                location: region.unwrap_or_else(|| "Remote".into()),
                remote: true,
                employment_type: it.extra.get("type").map(|s| s.trim().to_string()),
                description: clean_html(&it.description),
                url,
                salary: None,
                posted_at: feed::to_iso_date(&it.pub_date),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::map;
    use crate::sources::feed::FeedItem;
    use std::collections::HashMap;

    fn item(title: &str) -> FeedItem {
        let mut extra = HashMap::new();
        extra.insert("region".into(), "Anywhere in the World".into());
        extra.insert("type".into(), "Full-Time".into());
        FeedItem {
            title: title.into(),
            link: "https://weworkremotely.com/remote-jobs/x".into(),
            guid: "https://weworkremotely.com/remote-jobs/x".into(),
            description: "<p>Headquarters: remote</p>".into(),
            pub_date: "Tue, 02 Jun 2026 20:31:22 +0000".into(),
            extra,
        }
    }

    #[test]
    fn splits_company_from_role_and_is_remote() {
        let jobs = map(vec![item("Express Vitamin: Product Marketing Tester")]);
        assert_eq!(jobs.len(), 1);
        let j = &jobs[0];
        assert_eq!(j.company, "Express Vitamin");
        assert_eq!(j.title, "Product Marketing Tester");
        assert!(j.remote);
        assert_eq!(j.location, "Anywhere in the World");
        assert_eq!(j.employment_type.as_deref(), Some("Full-Time"));
        assert_eq!(j.external_id, "https://weworkremotely.com/remote-jobs/x");
        assert_eq!(j.description, "Headquarters: remote");
        assert_eq!(j.posted_at.as_deref(), Some("2026-06-02"));
    }

    #[test]
    fn role_keeps_later_colons() {
        // Only the first ": " separates company from role.
        let jobs = map(vec![item("Acme: Senior Engineer: Platform")]);
        assert_eq!(jobs[0].company, "Acme");
        assert_eq!(jobs[0].title, "Senior Engineer: Platform");
    }

    #[test]
    fn title_without_company_prefix_is_kept_whole() {
        let jobs = map(vec![item("Just A Title")]);
        assert_eq!(jobs[0].company, "");
        assert_eq!(jobs[0].title, "Just A Title");
    }
}
