//! Source adapters. Each adapter is a module exposing
//! `async fn fetch(&Client, &Value) -> Result<Vec<RawJob>>`. To add a source,
//! add a module and a match arm in [`fetch`].

pub mod adzuna;
pub mod arbeitnow;
pub mod ashby;
pub mod greenhouse;
pub mod jobicy;
pub mod jsonld;
pub mod lever;
pub mod remotive;
pub mod usajobs;

use anyhow::Result;
use reqwest::Client;
use serde_json::Value;

use crate::model::RawJob;

/// Dispatch a fetch to the adapter for `kind`.
pub async fn fetch(http: &Client, kind: &str, config: &Value) -> Result<Vec<RawJob>> {
    match kind {
        "greenhouse" => greenhouse::fetch(http, config).await,
        "lever" => lever::fetch(http, config).await,
        "ashby" => ashby::fetch(http, config).await,
        "arbeitnow" => arbeitnow::fetch(http, config).await,
        "remotive" => remotive::fetch(http, config).await,
        "jobicy" => jobicy::fetch(http, config).await,
        "jsonld" => jsonld::fetch(http, config).await,
        "adzuna" => adzuna::fetch(http, config).await,
        "usajobs" => usajobs::fetch(http, config).await,
        other => anyhow::bail!("unknown source kind: {other}"),
    }
}
