//! Core data types shared between the storage layer, the adapters, and the UI.

use serde::Serialize;

/// A reusable interview story (usually STAR) drawn from the user's real experience,
/// woven into behavioral answers so they are grounded and specific.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Story {
    pub id: i64,
    pub title: String,
    pub content: String,
    pub tags: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A switchable persona: interview context (role, company, resume, notes) plus its own
/// story bank, used to tailor live answers. The active profile drives the assistant.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: i64,
    pub name: String,
    pub role: String,
    pub company: String,
    pub resume: String,
    pub notes: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// A saved interview session: the conversation transcript and the answers given, kept
/// for later review and to surface weak areas to study.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Session {
    pub id: i64,
    pub title: String,
    pub transcript: String,
    pub answers: String,
    pub created_at: i64,
}
