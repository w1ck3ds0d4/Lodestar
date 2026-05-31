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
#[allow(clippy::too_many_arguments)]
pub fn search_jobs(
    state: State<'_, AppState>,
    query: String,
    workplaces: Vec<String>,
    levels: Vec<String>,
    kinds: Vec<String>,
    relocation_only: bool,
    remote_eligible: bool,
    limit: i64,
) -> Result<Vec<Job>, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::search_jobs(
        &conn,
        &query,
        &workplaces,
        &levels,
        &kinds,
        relocation_only,
        remote_eligible,
        limit,
    )
    .map_err(|e| e.to_string())
}

/// The ISO 3166-1 alpha-2 country the user can work from, used by the
/// "remote fit" filter. Returns the stored value, or detects it once via IP
/// geolocation and caches it. Empty string if detection fails.
#[tauri::command]
pub async fn work_country(state: State<'_, AppState>) -> Result<String, String> {
    {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        if let Some(c) = db::get_setting(&conn, "work_from_country")
            .map_err(|e| e.to_string())?
            .filter(|s| !s.trim().is_empty())
        {
            return Ok(c);
        }
    }
    let http = state.http.clone();
    let code = http
        .get("https://ipapi.co/country/")
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?
        .trim()
        .to_uppercase();
    if code.len() == 2 && code.chars().all(|c| c.is_ascii_alphabetic()) {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        db::set_setting(&conn, "work_from_country", &code).map_err(|e| e.to_string())?;
        Ok(code)
    } else {
        Ok(String::new())
    }
}

/// Override the work-from country. A blank code clears it so the next
/// `work_country` call re-detects via IP geolocation.
#[tauri::command]
pub fn set_work_country(state: State<'_, AppState>, code: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    let code = code.trim().to_uppercase();
    if code.is_empty() {
        db::delete_setting(&conn, "work_from_country").map_err(|e| e.to_string())
    } else {
        db::set_setting(&conn, "work_from_country", &code).map_err(|e| e.to_string())
    }
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

/// Generate an AI study plan for a skill (uses the configured answer engine + profile).
#[tauri::command]
pub async fn study_plan(
    state: State<'_, AppState>,
    skill: String,
    goal: String,
) -> Result<String, String> {
    if skill.trim().is_empty() {
        return Err("Add a skill name first.".into());
    }
    let (engine, profile) = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        (build_engine(&conn)?, interview_profile(&conn))
    };
    let http = state.http.clone();
    crate::assistant::study_plan(&http, &engine, &skill, &goal, profile.as_deref())
        .await
        .map_err(|e| e.to_string())
}

// --- Screen privacy ---

/// Toggle whether this window is excluded from screen capture and screen sharing.
/// On Windows this maps to `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)`: the
/// window stays fully visible on the local display but is omitted from anything that
/// captures the screen (Zoom/Teams/Meet shares, OBS, recorders). No-op on platforms
/// that do not support it.
#[tauri::command]
pub fn set_screen_privacy(window: tauri::WebviewWindow, enabled: bool) -> Result<(), String> {
    window
        .set_content_protected(enabled)
        .map_err(|e| e.to_string())
}

// --- Assistant (Claude / Ollama) ---

const DEFAULT_MODEL: &str = "claude-opus-4-8";
const DEFAULT_OLLAMA_URL: &str = "http://localhost:11434";
const DEFAULT_OLLAMA_MODEL: &str = "llama3.1";

/// Build the answer engine from settings: Claude (default) or local Ollama.
fn build_engine(conn: &rusqlite::Connection) -> Result<crate::assistant::Engine, String> {
    let engine = db::get_setting(conn, "answer_engine")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "claude".to_string());
    if engine == "ollama" {
        let url = db::get_setting(conn, "ollama_url")
            .map_err(|e| e.to_string())?
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_OLLAMA_URL.to_string());
        let model = db::get_setting(conn, "ollama_model")
            .map_err(|e| e.to_string())?
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_OLLAMA_MODEL.to_string());
        Ok(crate::assistant::Engine::Ollama { url, model })
    } else {
        let key = db::get_setting(conn, "anthropic_api_key")
            .map_err(|e| e.to_string())?
            .filter(|k| !k.trim().is_empty())
            .ok_or("No API key set. Add your Anthropic API key in the Assistant tab.")?;
        let model = db::get_setting(conn, "assistant_model")
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());
        Ok(crate::assistant::Engine::Claude {
            api_key: key,
            model,
        })
    }
}

/// Build the user's interview/study profile (role, resume, notes) from settings, so
/// answers can be tailored. `None` when nothing is set.
fn interview_profile(conn: &rusqlite::Connection) -> Option<String> {
    let get = |k: &str| {
        db::get_setting(conn, k)
            .ok()
            .flatten()
            .filter(|s| !s.trim().is_empty())
    };
    let mut parts = Vec::new();
    if let Some(role) = get("context_role") {
        parts.push(format!("Target role / focus: {role}"));
    }
    if let Some(resume) = get("context_resume") {
        parts.push(format!("Resume / background:\n{resume}"));
    }
    if let Some(notes) = get("context_notes") {
        parts.push(format!("Notes / materials:\n{notes}"));
    }
    (!parts.is_empty()).then(|| parts.join("\n\n"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantSettings {
    pub has_key: bool,
    pub has_stt_key: bool,
    pub stt_engine: String,
    pub capture_source: String,
    pub model: String,
    pub input_device: String,
    pub output_device: String,
    pub answer_engine: String,
    pub ollama_url: String,
    pub ollama_model: String,
    pub context_role: String,
    pub context_resume: String,
    pub context_notes: String,
}

#[tauri::command]
pub fn get_assistant_settings(state: State<'_, AppState>) -> Result<AssistantSettings, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    let present = |k: &str| -> Result<bool, String> {
        Ok(db::get_setting(&conn, k)
            .map_err(|e| e.to_string())?
            .map(|v| !v.trim().is_empty())
            .unwrap_or(false))
    };
    let get = |k: &str| -> Result<String, String> {
        Ok(db::get_setting(&conn, k)
            .map_err(|e| e.to_string())?
            .unwrap_or_default())
    };
    let has_key = present("anthropic_api_key")?;
    let has_stt_key = present("deepgram_api_key")?;
    let stt_engine = db::get_setting(&conn, "stt_engine")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "deepgram".to_string());
    let capture_source = db::get_setting(&conn, "capture_source")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "system".to_string());
    let model = db::get_setting(&conn, "assistant_model")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());
    let answer_engine = db::get_setting(&conn, "answer_engine")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "claude".to_string());
    let ollama_url = db::get_setting(&conn, "ollama_url")
        .map_err(|e| e.to_string())?
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_OLLAMA_URL.to_string());
    let ollama_model = db::get_setting(&conn, "ollama_model")
        .map_err(|e| e.to_string())?
        .unwrap_or_default();
    Ok(AssistantSettings {
        has_key,
        has_stt_key,
        stt_engine,
        capture_source,
        model,
        input_device: get("input_device")?,
        output_device: get("output_device")?,
        answer_engine,
        ollama_url,
        ollama_model,
        context_role: get("context_role")?,
        context_resume: get("context_resume")?,
        context_notes: get("context_notes")?,
    })
}

/// Save the interview/study context (role, resume, notes) used to tailor answers.
#[tauri::command]
pub fn set_interview_context(
    state: State<'_, AppState>,
    role: String,
    resume: String,
    notes: String,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    let save = |k: &str, v: &str| -> Result<(), String> {
        if v.trim().is_empty() {
            db::delete_setting(&conn, k).map_err(|e| e.to_string())
        } else {
            db::set_setting(&conn, k, v.trim()).map_err(|e| e.to_string())
        }
    };
    save("context_role", &role)?;
    save("context_resume", &resume)?;
    save("context_notes", &notes)?;
    Ok(())
}

/// Set the answer engine: "claude" (default) or "ollama".
#[tauri::command]
pub fn set_answer_engine(state: State<'_, AppState>, engine: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::set_setting(&conn, "answer_engine", &engine).map_err(|e| e.to_string())
}

/// Set the Ollama model used when the answer engine is "ollama".
#[tauri::command]
pub fn set_ollama_model(state: State<'_, AppState>, model: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::set_setting(&conn, "ollama_model", model.trim()).map_err(|e| e.to_string())
}

/// Set the Ollama server URL (blank => default localhost:11434).
#[tauri::command]
pub fn set_ollama_url(state: State<'_, AppState>, url: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    let url = url.trim();
    if url.is_empty() {
        db::delete_setting(&conn, "ollama_url").map_err(|e| e.to_string())
    } else {
        db::set_setting(&conn, "ollama_url", url).map_err(|e| e.to_string())
    }
}

/// List locally-installed Ollama models (for the model picker).
#[tauri::command]
pub async fn list_ollama_models(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let (http, url) = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        let url = db::get_setting(&conn, "ollama_url")
            .map_err(|e| e.to_string())?
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_OLLAMA_URL.to_string());
        (state.http.clone(), url)
    };
    crate::assistant::ollama_models(&http, &url)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevices {
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
}

/// List the available microphone (input) and speaker (output) device names.
#[tauri::command]
pub fn list_audio_devices() -> Result<AudioDevices, String> {
    let (inputs, outputs) = crate::listen::list_devices();
    Ok(AudioDevices { inputs, outputs })
}

/// Select the microphone to capture (blank = system default).
#[tauri::command]
pub fn set_input_device(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    if name.trim().is_empty() {
        db::delete_setting(&conn, "input_device").map_err(|e| e.to_string())
    } else {
        db::set_setting(&conn, "input_device", name.trim()).map_err(|e| e.to_string())
    }
}

/// Select the output device to capture via loopback (blank = system default).
#[tauri::command]
pub fn set_output_device(state: State<'_, AppState>, name: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    if name.trim().is_empty() {
        db::delete_setting(&conn, "output_device").map_err(|e| e.to_string())
    } else {
        db::set_setting(&conn, "output_device", name.trim()).map_err(|e| e.to_string())
    }
}

/// Store (or, when blank, clear) the Anthropic API key in local SQLite.
#[tauri::command]
pub fn set_api_key(state: State<'_, AppState>, key: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    let trimmed = key.trim();
    if trimmed.is_empty() {
        db::delete_setting(&conn, "anthropic_api_key").map_err(|e| e.to_string())
    } else {
        db::set_setting(&conn, "anthropic_api_key", trimmed).map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub fn set_assistant_model(state: State<'_, AppState>, model: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::set_setting(&conn, "assistant_model", &model).map_err(|e| e.to_string())
}

/// Streamed answer chunks sent to the frontend over a Tauri channel.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum AnswerChunk {
    Delta { text: String },
    Done { text: String },
    Error { message: String },
}

#[tauri::command]
pub async fn ask_assistant(
    state: State<'_, AppState>,
    question: String,
    context: Option<String>,
    on_event: tauri::ipc::Channel<AnswerChunk>,
) -> Result<(), String> {
    if question.trim().is_empty() {
        let _ = on_event.send(AnswerChunk::Error {
            message: "Ask a question first.".into(),
        });
        return Ok(());
    }
    // Build the engine + profile under a brief lock, then drop it before the await.
    let (engine, profile) = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        (build_engine(&conn), interview_profile(&conn))
    };
    let engine = match engine {
        Ok(e) => e,
        Err(message) => {
            let _ = on_event.send(AnswerChunk::Error { message });
            return Ok(());
        }
    };
    let http = state.http.clone();
    let emit = |t: &str| {
        let _ = on_event.send(AnswerChunk::Delta {
            text: t.to_string(),
        });
    };
    match crate::assistant::ask(
        &http,
        &engine,
        &question,
        context.as_deref(),
        profile.as_deref(),
        Some(&emit),
    )
    .await
    {
        Ok(full) => {
            let _ = on_event.send(AnswerChunk::Done { text: full });
        }
        Err(e) => {
            let _ = on_event.send(AnswerChunk::Error {
                message: e.to_string(),
            });
        }
    }
    Ok(())
}

/// Auto-answer the newest transcript segment if it is a question/problem (any
/// language); returns the answer or the literal "NONE".
#[tauri::command]
pub async fn auto_answer(
    state: State<'_, AppState>,
    transcript: String,
    latest: String,
) -> Result<String, String> {
    if latest.trim().is_empty() {
        return Ok("NONE".into());
    }
    let (engine, profile) = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        (build_engine(&conn)?, interview_profile(&conn))
    };
    let http = state.http.clone();
    crate::assistant::auto_answer(&http, &engine, &transcript, &latest, profile.as_deref())
        .await
        .map_err(|e| e.to_string())
}

// --- Screen watcher: read questions off a monitor and auto-answer them ---

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorInfo {
    pub id: usize,
    pub name: String,
}

/// List connected monitors (addressed by index) for the screen picker.
#[tauri::command]
pub fn list_monitors() -> Result<Vec<MonitorInfo>, String> {
    Ok((0..crate::screen::count())
        .map(|id| MonitorInfo {
            id,
            name: format!("Display {}", id + 1),
        })
        .collect())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenAnswer {
    /// False when the screen was unchanged since the last check (no API call made).
    pub changed: bool,
    /// The model's answer, or "NONE" when nothing on screen needed answering.
    pub answer: String,
}

/// Capture `monitor` and ask Claude's vision to answer any question/problem on it.
/// When `force` is false an unchanged screen is skipped (no tokens spent).
#[tauri::command]
pub async fn answer_screen(
    state: State<'_, AppState>,
    monitor: usize,
    force: bool,
) -> Result<ScreenAnswer, String> {
    let cap = crate::screen::capture(monitor).map_err(|e| e.to_string())?;

    {
        let mut last = state
            .screen_hash
            .lock()
            .map_err(|_| "screen hash poisoned")?;
        if !force && *last == Some(cap.hash) {
            return Ok(ScreenAnswer {
                changed: false,
                answer: String::new(),
            });
        }
        *last = Some(cap.hash);
    }

    let (engine, profile) = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        (build_engine(&conn)?, interview_profile(&conn))
    };

    let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &cap.png);
    let http = state.http.clone();
    let answer = crate::assistant::answer_screen(&http, &engine, &b64, profile.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    Ok(ScreenAnswer {
        changed: true,
        answer,
    })
}

/// Capture the chosen monitor as a base64 PNG, for the in-app snip-to-explain
/// selector (the frontend crops a region and sends it to `explain_region`).
#[tauri::command]
pub fn capture_screen_png(monitor: usize) -> Result<String, String> {
    let cap = crate::screen::capture(monitor).map_err(|e| e.to_string())?;
    Ok(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &cap.png,
    ))
}

/// Explain a user-cropped screen region (base64 PNG) via the answer engine's vision.
#[tauri::command]
pub async fn explain_region(
    state: State<'_, AppState>,
    png_base64: String,
) -> Result<String, String> {
    let (engine, profile) = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        (build_engine(&conn)?, interview_profile(&conn))
    };
    let http = state.http.clone();
    crate::assistant::explain_image(&http, &engine, &png_base64, profile.as_deref())
        .await
        .map_err(|e| e.to_string())
}

// --- Live listening (system-audio loopback capture -> Deepgram) ---

/// Store (or, when blank, clear) the Deepgram API key in local SQLite.
#[tauri::command]
pub fn set_deepgram_key(state: State<'_, AppState>, key: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    let trimmed = key.trim();
    if trimmed.is_empty() {
        db::delete_setting(&conn, "deepgram_api_key").map_err(|e| e.to_string())
    } else {
        db::set_setting(&conn, "deepgram_api_key", trimmed).map_err(|e| e.to_string())
    }
}

/// Start capturing computer audio on a background thread. Transcripts arrive on
/// the frontend as `transcript` events; failures as `listen-error`.
#[tauri::command]
pub fn set_stt_engine(state: State<'_, AppState>, engine: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::set_setting(&conn, "stt_engine", &engine).map_err(|e| e.to_string())
}

/// Which audio to capture: "system" (loopback), "mic", or "both".
#[tauri::command]
pub fn set_capture_source(state: State<'_, AppState>, source: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::set_setting(&conn, "capture_source", &source).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn start_listening(state: State<'_, AppState>, app: AppHandle) -> Result<(), String> {
    use std::sync::atomic::Ordering;
    use tauri::Manager;

    let engine_kind = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        db::get_setting(&conn, "stt_engine")
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| "deepgram".to_string())
    };

    let engine = if engine_kind == "local" {
        let models_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .join("models");
        std::fs::create_dir_all(&models_dir).map_err(|e| e.to_string())?;
        crate::listen::EngineConfig::Local { models_dir }
    } else {
        let key = {
            let conn = state.db.lock().map_err(|_| "db poisoned")?;
            db::get_setting(&conn, "deepgram_api_key").map_err(|e| e.to_string())?
        };
        let key = key.filter(|k| !k.trim().is_empty()).ok_or(
            "No Deepgram API key set. Add it in the Assistant tab, or switch the engine to Local.",
        )?;
        crate::listen::EngineConfig::Deepgram { key }
    };

    let (capture, devices) = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        let capture = crate::listen::CaptureSource::parse(
            &db::get_setting(&conn, "capture_source")
                .map_err(|e| e.to_string())?
                .unwrap_or_else(|| "system".to_string()),
        );
        let devices = crate::listen::DeviceChoice {
            input: db::get_setting(&conn, "input_device").map_err(|e| e.to_string())?,
            output: db::get_setting(&conn, "output_device").map_err(|e| e.to_string())?,
        };
        (capture, devices)
    };

    if state.listening.swap(true, Ordering::SeqCst) {
        return Ok(()); // already running
    }
    let http = state.http.clone();
    let flag = state.listening.clone();
    std::thread::spawn(move || {
        use tauri::Emitter;
        if let Err(e) = crate::listen::run(&app, http, engine, capture, devices, flag.clone()) {
            let _ = app.emit("listen-error", e.to_string());
        }
        flag.store(false, Ordering::SeqCst);
        let _ = app.emit("listen-stopped", ());
    });
    Ok(())
}

#[tauri::command]
pub fn stop_listening(state: State<'_, AppState>) -> Result<(), String> {
    state
        .listening
        .store(false, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}
