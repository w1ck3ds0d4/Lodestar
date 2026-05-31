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

// --- Assistant (Claude API) ---

const DEFAULT_MODEL: &str = "claude-opus-4-8";

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
    Ok(AssistantSettings {
        has_key,
        has_stt_key,
        stt_engine,
        capture_source,
        model,
        input_device: get("input_device")?,
        output_device: get("output_device")?,
    })
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

#[tauri::command]
pub async fn ask_assistant(
    state: State<'_, AppState>,
    question: String,
    context: Option<String>,
) -> Result<String, String> {
    if question.trim().is_empty() {
        return Err("Ask a question first.".into());
    }
    // Read the key + model under a brief lock, then drop it before the await.
    let (key, model) = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        let key = db::get_setting(&conn, "anthropic_api_key").map_err(|e| e.to_string())?;
        let model = db::get_setting(&conn, "assistant_model")
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());
        (key, model)
    };
    let key = key
        .filter(|k| !k.trim().is_empty())
        .ok_or("No API key set. Add your Anthropic API key in the Assistant tab.")?;
    let http = state.http.clone();
    crate::assistant::ask(&http, &key, &model, &question, context.as_deref())
        .await
        .map_err(|e| e.to_string())
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
    let (key, model) = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        let key = db::get_setting(&conn, "anthropic_api_key").map_err(|e| e.to_string())?;
        let model = db::get_setting(&conn, "assistant_model")
            .map_err(|e| e.to_string())?
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());
        (key, model)
    };
    let key = key
        .filter(|k| !k.trim().is_empty())
        .ok_or("No API key set. Add your Anthropic API key in the Assistant tab.")?;
    let http = state.http.clone();
    crate::assistant::auto_answer(&http, &key, &model, &transcript, &latest)
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
