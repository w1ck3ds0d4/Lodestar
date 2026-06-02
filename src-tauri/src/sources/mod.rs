//! Source adapters. Each adapter is a module exposing
//! `async fn fetch(&Client, &Value) -> Result<Vec<RawJob>>`. To add a source,
//! add a module and a match arm in [`fetch`].

pub mod adzuna;
pub mod arbeitnow;
pub mod ashby;
pub mod findwork;
pub mod greenhouse;
pub mod himalayas;
pub mod jobicy;
pub mod jooble;
pub mod jsonld;
pub mod lever;
pub mod recruitee;
pub mod reed;
pub mod remoteok;
pub mod remotive;
pub mod smartrecruiters;
pub mod themuse;
pub mod usajobs;
pub mod workable;

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
        "remoteok" => remoteok::fetch(http, config).await,
        "remotive" => remotive::fetch(http, config).await,
        "jobicy" => jobicy::fetch(http, config).await,
        "himalayas" => himalayas::fetch(http, config).await,
        "workable" => workable::fetch(http, config).await,
        "recruitee" => recruitee::fetch(http, config).await,
        "smartrecruiters" => smartrecruiters::fetch(http, config).await,
        "jsonld" => jsonld::fetch(http, config).await,
        "adzuna" => adzuna::fetch(http, config).await,
        "themuse" => themuse::fetch(http, config).await,
        "findwork" => findwork::fetch(http, config).await,
        "reed" => reed::fetch(http, config).await,
        "jooble" => jooble::fetch(http, config).await,
        "usajobs" => usajobs::fetch(http, config).await,
        other => anyhow::bail!("unknown source kind: {other}"),
    }
}
