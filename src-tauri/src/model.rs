//! Core data types shared between the storage layer, the adapters, and the UI.

use serde::Serialize;
use serde_json::Value;

/// A posting as returned by a source adapter, before normalization/storage.
#[derive(Debug, Clone)]
pub struct RawJob {
    pub external_id: String,
    pub title: String,
    pub company: String,
    pub location: String,
    pub remote: bool,
    pub employment_type: Option<String>,
    pub description: String,
    pub url: String,
    pub salary: Option<String>,
    pub posted_at: Option<String>,
}

/// A stored job row, as returned to the UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: i64,
    pub source_kind: String,
    pub source_label: String,
    pub external_id: String,
    pub title: String,
    pub company: String,
    pub location: String,
    pub remote: bool,
    pub workplace: String,
    pub experience_level: String,
    pub employment_type: Option<String>,
    pub description: String,
    pub url: String,
    pub salary: Option<String>,
    pub posted_at: Option<String>,
    pub relocation: bool,
    pub remote_region: String,
    pub first_seen_at: i64,
    pub last_seen_at: i64,
}

/// A configured source and its last-run status.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub id: i64,
    pub kind: String,
    pub label: String,
    pub config: Value,
    pub enabled: bool,
    pub min_interval_secs: i64,
    pub last_run_at: Option<i64>,
    pub last_status: Option<String>,
    pub last_count: Option<i64>,
}

/// A saved search that fires a desktop notification when new matches arrive.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    pub id: i64,
    pub query: String,
    pub remote_only: bool,
    pub last_notified_at: Option<i64>,
}

/// A tracked job application or training program, with optional milestones.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Tracked {
    pub id: i64,
    pub kind: String, // "application" | "program"
    pub title: String,
    pub org: String,
    pub url: String,
    pub status: String,
    pub notes: String,
    pub applied_at: Option<String>,
    pub deadline: Option<String>,
    pub progress: i64,
    pub source_job_id: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub milestones: Vec<Milestone>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Milestone {
    pub id: i64,
    pub tracked_id: i64,
    pub label: String,
    pub done: bool,
    pub position: i64,
}

/// A skill the user is studying, with a self-rated level and logged hours.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Skill {
    pub id: i64,
    pub name: String,
    pub goal: String,
    pub level: i64,
    pub hours: f64,
    pub notes: String,
    pub created_at: i64,
    pub updated_at: i64,
}
