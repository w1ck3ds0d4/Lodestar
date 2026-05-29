//! Tauri commands exposed to the frontend via `invoke`.

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, State};

use crate::db;
use crate::model::{Alert, Job, Skill, Source, Tracked};
use crate::util::now_ms;
use crate::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub total_jobs: i64,
    pub companies: i64,
}

#[tauri::command]
pub fn search_jobs(
    state: State<'_, AppState>,
    query: String,
    workplaces: Vec<String>,
    levels: Vec<String>,
    kinds: Vec<String>,
    limit: i64,
) -> Result<Vec<Job>, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::search_jobs(&conn, &query, &workplaces, &levels, &kinds, limit).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_sources(state: State<'_, AppState>) -> Result<Vec<Source>, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::list_sources(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_source(
    state: State<'_, AppState>,
    kind: String,
    label: String,
    config: Value,
    min_interval_secs: i64,
) -> Result<i64, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::add_source(&conn, &kind, &label, &config, min_interval_secs).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_source(
    state: State<'_, AppState>,
    id: i64,
    label: String,
    enabled: bool,
    config: Value,
    min_interval_secs: i64,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::update_source(&conn, id, &label, enabled, &config, min_interval_secs)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_source(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::remove_source(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_stats(state: State<'_, AppState>) -> Result<Stats, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    let (total_jobs, companies) = db::stats(&conn).map_err(|e| e.to_string())?;
    Ok(Stats {
        total_jobs,
        companies,
    })
}

#[tauri::command]
pub fn list_alerts(state: State<'_, AppState>) -> Result<Vec<Alert>, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::list_alerts(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_alert(
    state: State<'_, AppState>,
    query: String,
    remote_only: bool,
) -> Result<i64, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::add_alert(&conn, &query, remote_only).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_alert(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::remove_alert(&conn, id).map_err(|e| e.to_string())
}

/// Kick off an immediate sync of all enabled sources in the background.
#[tauri::command]
pub fn sync_now(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    let db = state.db.clone();
    let http = state.http.clone();
    tauri::async_runtime::spawn(async move {
        crate::ingest::sync_all(db, http, app, true).await;
    });
    Ok(())
}

// --- Career tracker: applications + programs ---

#[tauri::command]
pub fn list_tracked(
    state: State<'_, AppState>,
    kind: Option<String>,
) -> Result<Vec<Tracked>, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::career::list_tracked(&conn, kind.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub fn add_tracked(
    state: State<'_, AppState>,
    kind: String,
    title: String,
    org: String,
    url: String,
    status: String,
    notes: String,
    deadline: Option<String>,
    source_job_id: Option<i64>,
) -> Result<i64, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::career::add_tracked(
        &conn,
        now_ms(),
        &kind,
        &title,
        &org,
        &url,
        &status,
        &notes,
        deadline.as_deref(),
        source_job_id,
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_tracked(
    state: State<'_, AppState>,
    id: i64,
    status: String,
    notes: String,
    progress: i64,
    deadline: Option<String>,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::career::update_tracked(
        &conn,
        now_ms(),
        id,
        &status,
        &notes,
        progress,
        deadline.as_deref(),
    )
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_tracked(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::career::remove_tracked(&conn, id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_milestone(
    state: State<'_, AppState>,
    tracked_id: i64,
    label: String,
) -> Result<i64, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::career::add_milestone(&conn, tracked_id, &label).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn set_milestone(state: State<'_, AppState>, id: i64, done: bool) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::career::set_milestone(&conn, id, done).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_milestone(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::career::remove_milestone(&conn, id).map_err(|e| e.to_string())
}

// --- Study / skills ---

#[tauri::command]
pub fn list_skills(state: State<'_, AppState>) -> Result<Vec<Skill>, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::career::list_skills(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_skill(state: State<'_, AppState>, name: String, goal: String) -> Result<i64, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::career::add_skill(&conn, now_ms(), &name, &goal).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_skill(
    state: State<'_, AppState>,
    id: i64,
    name: String,
    goal: String,
    level: i64,
    notes: String,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::career::update_skill(&conn, now_ms(), id, &name, &goal, level, &notes)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn log_study(state: State<'_, AppState>, id: i64, hours: f64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::career::log_study(&conn, now_ms(), id, hours).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_skill(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::career::remove_skill(&conn, id).map_err(|e| e.to_string())
}
