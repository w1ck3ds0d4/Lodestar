//! Generic adapter: parse schema.org `JobPosting` JSON-LD from any careers page.
//! Config: `{ "url": "https://.../careers" }`.

use std::sync::OnceLock;

use anyhow::Result;
use regex::Regex;
use reqwest::Client;
use serde_json::Value;

use crate::model::RawJob;
use crate::util::{cfg_str, clean_html, stable_hash, vstr};

fn script_re() -> &'static Regex {
    static R: OnceLock<Regex> = OnceLock::new();
    R.get_or_init(|| {
        Regex::new(r#"(?is)<script[^>]*type=["']application/ld\+json["'][^>]*>(.*?)</script>"#)
            .unwrap()
    })
}

pub async fn fetch(http: &Client, cfg: &Value) -> Result<Vec<RawJob>> {
    let page = cfg_str(cfg, "url")?;
    let html = http
        .get(&page)
        .header("Accept", "text/html")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    let mut postings: Vec<Value> = Vec::new();
    for cap in script_re().captures_iter(&html) {
        let raw = cap.get(1).map(|m| m.as_str()).unwrap_or("").trim();
        if let Ok(v) = serde_json::from_str::<Value>(raw) {
            collect(&v, &mut postings);
        }
    }

    let mut out = Vec::new();
    for p in &postings {
        if let Some(job) = map_posting(p, &page) {
            out.push(job);
        }
    }
    Ok(out)
}

fn is_jobposting(v: &Value) -> bool {
    match v.get("@type") {
        Some(Value::String(s)) => s.eq_ignore_ascii_case("JobPosting"),
        Some(Value::Array(a)) => a.iter().any(|x| {
            x.as_str()
                .map(|s| s.eq_ignore_ascii_case("JobPosting"))
                .unwrap_or(false)
        }),
        _ => false,
    }
}

fn collect(v: &Value, out: &mut Vec<Value>) {
    match v {
        Value::Object(map) => {
            if is_jobposting(v) {
                out.push(v.clone());
            }
            if let Some(graph) = map.get("@graph") {
                collect(graph, out);
            }
        }
        Value::Array(arr) => arr.iter().for_each(|item| collect(item, out)),
        _ => {}
    }
}

fn org_name(v: &Value) -> Option<String> {
    match v.get("hiringOrganization") {
        Some(Value::String(s)) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
        Some(o @ Value::Object(_)) => vstr(o, "name"),
        _ => None,
    }
}

fn addr_field(addr: &Value, key: &str) -> Option<String> {
    match addr.get(key) {
        Some(Value::String(s)) if !s.trim().is_empty() => Some(s.trim().to_string()),
        Some(o @ Value::Object(_)) => vstr(o, "name"),
        _ => None,
    }
}

fn location_str(v: &Value) -> String {
    let first = match v.get("jobLocation") {
        Some(Value::Array(a)) => a.first(),
        Some(o @ Value::Object(_)) => Some(o),
        _ => None,
    };
    let Some(place) = first else {
        return String::new();
    };
    let Some(addr) = place.get("address") else {
        return String::new();
    };
    if let Some(s) = addr.as_str() {
        return s.trim().to_string();
    }
    ["addressLocality", "addressRegion", "addressCountry"]
        .iter()
        .filter_map(|k| addr_field(addr, k))
        .collect::<Vec<_>>()
        .join(", ")
}

fn employment_type(v: &Value) -> Option<String> {
    match v.get("employmentType") {
        Some(Value::String(s)) => Some(s.trim().to_string()).filter(|s| !s.is_empty()),
        Some(Value::Array(a)) => a.first().and_then(|x| x.as_str()).map(|s| s.to_string()),
        _ => None,
    }
}

fn salary_str(v: &Value) -> Option<String> {
    let bs = v.get("baseSalary")?;
    let cur = vstr(bs, "currency").unwrap_or_default();
    let value = bs.get("value")?;
    let num = |k: &str| value.get(k).and_then(|x| x.as_f64()).map(|n| n as i64);
    let unit = vstr(value, "unitText").unwrap_or_default();
    let amount = if let Some(v0) = num("value") {
        v0.to_string()
    } else {
        match (num("minValue"), num("maxValue")) {
            (Some(a), Some(b)) => format!("{a}-{b}"),
            _ => return None,
        }
    };
    Some(format!("{cur} {amount} {unit}").trim().to_string())
}

fn is_remote(v: &Value) -> bool {
    if let Some(t) = vstr(v, "jobLocationType") {
        if t.eq_ignore_ascii_case("TELECOMMUTE") {
            return true;
        }
    }
    v.get("applicantLocationRequirements").is_some()
}

fn map_posting(p: &Value, page: &str) -> Option<RawJob> {
    let title = vstr(p, "title")?;
    let company = org_name(p).unwrap_or_default();
    let url = vstr(p, "url").unwrap_or_else(|| page.to_string());
    let external_id = vstr(p, "identifier")
        .or_else(|| p.get("identifier").and_then(|i| vstr(i, "value")))
        .unwrap_or_else(|| stable_hash(&format!("{page}|{title}|{company}")));
    Some(RawJob {
        external_id,
        title: title.clone(),
        company,
        location: location_str(p),
        remote: is_remote(p),
        employment_type: employment_type(p),
        description: clean_html(&vstr(p, "description").unwrap_or_default()),
        url,
        salary: salary_str(p),
        posted_at: vstr(p, "datePosted").map(|s| s.chars().take(10).collect()),
    })
}
