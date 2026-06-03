//! Generic RSS 2.0 / Atom 1.0 job feed (keyless). Config: `{ "url": "https://.../jobs.rss", "company": "(optional)" }`.
//! The feed URL is user-supplied, so it goes through the same SSRF guard + bounded,
//! re-checked redirect policy as the careers-page (jsonld) source.

use anyhow::{anyhow, Result};
use reqwest::Client;
use serde_json::Value;

use crate::model::RawJob;
use crate::sources::feed;
use crate::util::{
    cfg_opt, cfg_str, clean_html, host_str_blocked, looks_remote, stable_hash, validate_public_url,
};

pub async fn fetch(http: &Client, cfg: &Value) -> Result<Vec<RawJob>> {
    let url = cfg_str(cfg, "url")?;
    validate_public_url(&url).map_err(|e| anyhow!(e))?;
    let company = cfg_opt(cfg, "company");

    let client = reqwest::Client::builder()
        .user_agent("JobRadar/0.1 (+personal job search)")
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 3 || host_str_blocked(attempt.url()) {
                attempt.stop()
            } else {
                attempt.follow()
            }
        }))
        .build()
        .unwrap_or_else(|_| http.clone());

    let xml = client
        .get(&url)
        .header(
            "Accept",
            "application/rss+xml, application/xml;q=0.9, text/xml;q=0.8",
        )
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    Ok(map(feed::parse(&xml), company.as_deref()))
}

/// Map generic feed items into RawJobs (pure). Company comes from the config override,
/// else a `dc:creator`/`author` element if present. Remote is inferred from the text.
fn map(items: Vec<feed::FeedItem>, company: Option<&str>) -> Vec<RawJob> {
    items
        .into_iter()
        .filter_map(|it| {
            let title = it.title.trim().to_string();
            if title.is_empty() {
                return None;
            }
            let url = if !it.link.is_empty() {
                it.link.clone()
            } else {
                it.guid.clone()
            };
            let description = clean_html(&it.description);
            let company = company
                .map(str::to_string)
                .filter(|s| !s.is_empty())
                .or_else(|| it.extra.get("creator").cloned())
                .or_else(|| it.extra.get("author").cloned())
                // Atom nests the author as <author><name>..</name></author>.
                .or_else(|| it.extra.get("name").cloned())
                .unwrap_or_default();
            let external_id = if !it.guid.is_empty() {
                it.guid.clone()
            } else if !url.is_empty() {
                url.clone()
            } else {
                stable_hash(&title)
            };
            Some(RawJob {
                experience_level: None,
                remote: looks_remote(&title) || looks_remote(&description),
                external_id,
                title,
                company,
                location: String::new(),
                employment_type: None,
                description,
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

    fn item() -> FeedItem {
        FeedItem {
            title: "Backend Engineer".into(),
            link: "https://board.example/jobs/7".into(),
            guid: "guid-7".into(),
            description: "<p>Work remotely on &amp; APIs</p>".into(),
            pub_date: "Tue, 02 Jun 2026 20:31:22 +0000".into(),
            extra: Default::default(),
        }
    }

    #[test]
    fn maps_item_with_company_override() {
        let jobs = map(vec![item()], Some("Globex"));
        assert_eq!(jobs.len(), 1);
        let j = &jobs[0];
        assert_eq!(j.title, "Backend Engineer");
        assert_eq!(j.company, "Globex");
        assert_eq!(j.external_id, "guid-7");
        assert_eq!(j.url, "https://board.example/jobs/7");
        assert_eq!(j.description, "Work remotely on & APIs");
        assert!(j.remote, "description mentions remotely");
        assert_eq!(j.posted_at.as_deref(), Some("2026-06-02"));
    }

    #[test]
    fn company_falls_back_to_creator_then_empty() {
        let mut it = item();
        it.extra.insert("creator".into(), "Acme Inc".into());
        assert_eq!(map(vec![it], None)[0].company, "Acme Inc");
        assert_eq!(map(vec![item()], None)[0].company, "");
    }

    #[test]
    fn empty_title_is_dropped() {
        let mut it = item();
        it.title = "  ".into();
        assert!(map(vec![it], None).is_empty());
    }
}
