//! Tauri commands exposed to the frontend via `invoke`.

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, State};

use crate::db;
use crate::model::{Alert, Card, Job, Session, Skill, Source, Story, Tracked};
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
    posted_within_days: Option<i64>,
    min_salary: Option<i64>,
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
        posted_within_days,
        min_salary,
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

/// Set (or clear, when blank) the "next action" reminder on an application.
#[tauri::command]
pub fn set_tracked_next_action(
    state: State<'_, AppState>,
    id: i64,
    text: String,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::career::set_next_action(&conn, now_ms(), id, Some(&text)).map_err(|e| e.to_string())
}

/// Generate interview prep (questions + talking points + research) for a tracked
/// application and persist it on the item.
#[tauri::command]
pub async fn prep_application(
    state: State<'_, AppState>,
    id: i64,
    title: String,
    org: String,
) -> Result<String, String> {
    let (engine, profile, jd) = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        // Ground prep in the linked posting's description when this app came from one.
        let jd = db::job_jd(&conn, id).map_err(|e| e.to_string())?;
        (build_engine(&conn)?, interview_profile(&conn), jd)
    };
    let http = state.http.clone();
    let prep = crate::assistant::interview_prep(
        &http,
        &engine,
        &title,
        &org,
        jd.as_deref(),
        profile.as_deref(),
    )
    .await
    .map_err(|e| e.to_string())?;
    {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        crate::career::set_prep(&conn, id, &prep).map_err(|e| e.to_string())?;
    }
    Ok(prep)
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

// --- Interview story bank ---

#[tauri::command]
pub fn list_stories(state: State<'_, AppState>) -> Result<Vec<Story>, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::career::list_stories(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_story(
    state: State<'_, AppState>,
    title: String,
    content: String,
    tags: String,
) -> Result<i64, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::career::add_story(&conn, now_ms(), &title, &content, &tags).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn update_story(
    state: State<'_, AppState>,
    id: i64,
    title: String,
    content: String,
    tags: String,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::career::update_story(&conn, now_ms(), id, &title, &content, &tags)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_story(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::career::remove_story(&conn, id).map_err(|e| e.to_string())
}

/// Draft a STAR story from the user's rough notes (uses the configured answer engine).
#[tauri::command]
pub async fn draft_story(state: State<'_, AppState>, notes: String) -> Result<String, String> {
    if notes.trim().is_empty() {
        return Err("Jot down a few notes about the experience first.".into());
    }
    let (engine, profile) = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        (build_engine(&conn)?, interview_profile(&conn))
    };
    let http = state.http.clone();
    crate::assistant::story_draft(&http, &engine, &notes, profile.as_deref())
        .await
        .map_err(|e| e.to_string())
}

/// Generate an application document tailored to a job. `kind` is "bullets", "cover",
/// "ats", or "thanks"; `input` is the job description (or, for "thanks", the interview
/// transcript). Uses the stored resume/profile.
#[tauri::command]
pub async fn application_doc(
    state: State<'_, AppState>,
    kind: String,
    input: String,
) -> Result<String, String> {
    if input.trim().is_empty() {
        return Err(
            "Paste the job description first (or the interview transcript for a thank-you).".into(),
        );
    }
    let (engine, profile) = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        (build_engine(&conn)?, interview_profile(&conn))
    };
    let http = state.http.clone();
    crate::assistant::application_doc(&http, &engine, &kind, &input, profile.as_deref())
        .await
        .map_err(|e| e.to_string())
}

/// Score the stored resume/profile against a job description (structured ATS score).
#[tauri::command]
pub async fn score_ats(
    state: State<'_, AppState>,
    jd: String,
) -> Result<crate::assistant::AtsScore, String> {
    if jd.trim().is_empty() {
        return Err("Paste the job description first.".into());
    }
    let (engine, profile) = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        (build_engine(&conn)?, interview_profile(&conn))
    };
    let http = state.http.clone();
    crate::assistant::score_ats(&http, &engine, &jd, profile.as_deref())
        .await
        .map_err(|e| e.to_string())
}

/// Mock interview: ask the next question (given the ones already asked this session).
#[tauri::command]
pub async fn practice_question(
    state: State<'_, AppState>,
    asked: Vec<String>,
) -> Result<String, String> {
    let (engine, profile) = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        (build_engine(&conn)?, interview_profile(&conn))
    };
    let http = state.http.clone();
    crate::assistant::practice_question(&http, &engine, &asked, profile.as_deref())
        .await
        .map_err(|e| e.to_string())
}

/// Mock interview: score the candidate's answer and return coaching feedback.
#[tauri::command]
pub async fn practice_feedback(
    state: State<'_, AppState>,
    question: String,
    answer: String,
) -> Result<String, String> {
    if answer.trim().is_empty() {
        return Err("Type or paste your answer first.".into());
    }
    let (engine, profile) = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        (build_engine(&conn)?, interview_profile(&conn))
    };
    let http = state.http.clone();
    crate::assistant::practice_feedback(&http, &engine, &question, &answer, profile.as_deref())
        .await
        .map_err(|e| e.to_string())
}

// --- Interview session history ---

#[tauri::command]
pub fn list_sessions(state: State<'_, AppState>) -> Result<Vec<Session>, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::career::list_sessions(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_session(
    state: State<'_, AppState>,
    title: String,
    transcript: String,
    answers: String,
) -> Result<i64, String> {
    if transcript.trim().is_empty() && answers.trim().is_empty() {
        return Err("Nothing to save yet.".into());
    }
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::career::add_session(&conn, now_ms(), &title, &transcript, &answers)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_session(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::career::remove_session(&conn, id).map_err(|e| e.to_string())
}

/// Review a saved session: strengths, weak areas, and a prioritized study list.
#[tauri::command]
pub async fn session_review(
    state: State<'_, AppState>,
    transcript: String,
    answers: String,
) -> Result<String, String> {
    if transcript.trim().is_empty() && answers.trim().is_empty() {
        return Err("This session has no content to review.".into());
    }
    let (engine, profile) = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        (build_engine(&conn)?, interview_profile(&conn))
    };
    let http = state.http.clone();
    crate::assistant::session_review(&http, &engine, &transcript, &answers, profile.as_deref())
        .await
        .map_err(|e| e.to_string())
}

// --- Spaced-repetition flashcards ---

#[tauri::command]
pub fn list_cards(state: State<'_, AppState>) -> Result<Vec<Card>, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::career::list_cards(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn review_card(state: State<'_, AppState>, id: i64, good: bool) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::career::review_card(&conn, now_ms(), id, good).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_card(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::career::remove_card(&conn, id).map_err(|e| e.to_string())
}

/// Generate flashcards from study material (a session review, weak areas, notes, etc.)
/// and save them. Returns how many were added.
#[tauri::command]
pub async fn generate_cards(state: State<'_, AppState>, source: String) -> Result<usize, String> {
    if source.trim().is_empty() {
        return Err("Paste some material to make cards from first.".into());
    }
    let (engine, profile) = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        (build_engine(&conn)?, interview_profile(&conn))
    };
    let http = state.http.clone();
    let cards = crate::assistant::generate_cards(&http, &engine, &source, profile.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    let now = now_ms();
    for (front, back) in &cards {
        let _ = crate::career::add_card(&conn, now, front, back);
    }
    Ok(cards.len())
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

/// The user's chosen answer format ("auto" | "concise" | "star" | "code").
fn answer_style(conn: &rusqlite::Connection) -> String {
    db::get_setting(conn, "answer_style")
        .ok()
        .flatten()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "auto".to_string())
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
    if let Some(company) = get("context_company") {
        parts.push(format!("Company they are interviewing at:\n{company}"));
    }
    if let Some(resume) = get("context_resume") {
        parts.push(format!("Resume / background:\n{resume}"));
    }
    if let Some(notes) = get("context_notes") {
        parts.push(format!("Notes / materials:\n{notes}"));
    }
    // The story bank: real experiences to ground behavioral answers in. The model is
    // told to pick the most relevant and weave in concrete details (not invent any).
    if let Ok(stories) = crate::career::list_stories(conn) {
        if !stories.is_empty() {
            let mut block = String::from(
                "Candidate's real experience stories - for behavioral/experience questions, draw on the \
                 most relevant one and weave in its concrete details; do not invent facts beyond these:",
            );
            for s in &stories {
                let tags = if s.tags.trim().is_empty() {
                    String::new()
                } else {
                    format!(" [{}]", s.tags.trim())
                };
                block.push_str(&format!(
                    "\n\n# {}{}\n{}",
                    s.title.trim(),
                    tags,
                    s.content.trim()
                ));
            }
            parts.push(block);
        }
    }
    (!parts.is_empty()).then(|| parts.join("\n\n"))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantSettings {
    pub has_key: bool,
    pub has_stt_key: bool,
    pub stt_engine: String,
    pub stt_language: String,
    pub stt_diarize: bool,
    pub capture_source: String,
    pub model: String,
    pub input_device: String,
    pub output_device: String,
    pub answer_engine: String,
    pub answer_style: String,
    pub ollama_url: String,
    pub ollama_model: String,
    pub context_role: String,
    pub context_company: String,
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
    let stt_language = db::get_setting(&conn, "stt_language")
        .map_err(|e| e.to_string())?
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "multi".to_string());
    let stt_diarize = db::get_setting(&conn, "stt_diarize")
        .map_err(|e| e.to_string())?
        .map(|v| v == "true")
        .unwrap_or(false);
    let capture_source = db::get_setting(&conn, "capture_source")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "system".to_string());
    let model = db::get_setting(&conn, "assistant_model")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| DEFAULT_MODEL.to_string());
    let answer_engine = db::get_setting(&conn, "answer_engine")
        .map_err(|e| e.to_string())?
        .unwrap_or_else(|| "claude".to_string());
    let answer_style = answer_style(&conn);
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
        stt_language,
        stt_diarize,
        capture_source,
        model,
        input_device: get("input_device")?,
        output_device: get("output_device")?,
        answer_engine,
        answer_style,
        ollama_url,
        ollama_model,
        context_role: get("context_role")?,
        context_company: get("context_company")?,
        context_resume: get("context_resume")?,
        context_notes: get("context_notes")?,
    })
}

/// Save the interview/study context (role, company, resume, notes) used to tailor answers.
#[tauri::command]
pub fn set_interview_context(
    state: State<'_, AppState>,
    role: String,
    company: String,
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
    save("context_company", &company)?;
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

/// Set the answer format: "auto" (default), "concise", "star", or "code".
#[tauri::command]
pub fn set_answer_style(state: State<'_, AppState>, style: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::set_setting(&conn, "answer_style", &style).map_err(|e| e.to_string())
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
    let (engine, profile, style) = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        (
            build_engine(&conn),
            interview_profile(&conn),
            answer_style(&conn),
        )
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
        &style,
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
/// language). Streams the answer over `on_event` (delta/done/error); the answer is the
/// literal "NONE" when nothing is being asked, which the UI suppresses.
#[tauri::command]
pub async fn auto_answer(
    state: State<'_, AppState>,
    transcript: String,
    latest: String,
    on_event: tauri::ipc::Channel<AnswerChunk>,
) -> Result<(), String> {
    if latest.trim().is_empty() {
        let _ = on_event.send(AnswerChunk::Done {
            text: "NONE".into(),
        });
        return Ok(());
    }
    let (engine, profile, style) = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        (
            build_engine(&conn)?,
            interview_profile(&conn),
            answer_style(&conn),
        )
    };
    let http = state.http.clone();
    let emit = |t: &str| {
        let _ = on_event.send(AnswerChunk::Delta {
            text: t.to_string(),
        });
    };
    match crate::assistant::auto_answer(
        &http,
        &engine,
        &transcript,
        &latest,
        profile.as_deref(),
        &style,
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
    context: Option<String>,
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
    let answer = crate::assistant::answer_screen(
        &http,
        &engine,
        &b64,
        profile.as_deref(),
        context.as_deref(),
    )
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

/// Read plain text from the system clipboard. Powers the assistant's "Read page"
/// button: the user selects-all + copies a web page (or any document) in their
/// browser, and this pulls the full text in as context - including anything scrolled
/// off-screen, which a viewport screenshot can't capture.
#[tauri::command]
pub fn read_clipboard() -> Result<String, String> {
    let mut clipboard = arboard::Clipboard::new().map_err(|e| e.to_string())?;
    match clipboard.get_text() {
        Ok(t) => Ok(t),
        // No text on the clipboard (empty, or an image/file) is not an error here.
        Err(arboard::Error::ContentNotAvailable) => Ok(String::new()),
        Err(e) => Err(e.to_string()),
    }
}

// --- Phone companion server ---

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PhoneInfo {
    pub url: String,
    pub pin: String,
    /// QR code of the URL as an SVG data: URL.
    pub qr: String,
}

/// Start (or return the already-running) phone companion server.
#[tauri::command]
pub fn start_phone_server(state: State<'_, AppState>, app: AppHandle) -> Result<PhoneInfo, String> {
    let mut guard = state.phone.lock().map_err(|_| "phone lock poisoned")?;
    if let Some(server) = guard.as_ref() {
        return Ok(PhoneInfo {
            url: server.url.clone(),
            pin: server.pin.clone(),
            qr: crate::phone::qr_for(&server.url, &server.pin),
        });
    }
    let server = crate::phone::start(app)?;
    let info = PhoneInfo {
        url: server.url.clone(),
        pin: server.pin.clone(),
        qr: crate::phone::qr_for(&server.url, &server.pin),
    };
    *guard = Some(server);
    Ok(info)
}

#[tauri::command]
pub fn stop_phone_server(state: State<'_, AppState>) -> Result<(), String> {
    let mut guard = state.phone.lock().map_err(|_| "phone lock poisoned")?;
    if let Some(server) = guard.take() {
        server.stop();
    }
    Ok(())
}

/// Push the current UI state (JSON) to all connected phones.
#[tauri::command]
pub fn phone_broadcast(state: State<'_, AppState>, payload: String) -> Result<(), String> {
    let guard = state.phone.lock().map_err(|_| "phone lock poisoned")?;
    if let Some(server) = guard.as_ref() {
        server.broadcast(&payload);
    }
    Ok(())
}

// --- Usage / cost ---

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeUsageItem {
    pub model: String,
    pub input: u64,
    pub output: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageReport {
    /// Unix seconds when the tally started, or null if nothing recorded yet.
    pub since: Option<i64>,
    pub claude_items: Vec<ClaudeUsageItem>,
    /// Seconds of audio streamed to Deepgram (summed across sources).
    pub deepgram_seconds: f64,
    /// Credit the user says they topped up, in USD (0 if unset), so the UI can show a
    /// remaining figure for keys whose balance the API won't return.
    pub claude_credit: f64,
    pub deepgram_credit: f64,
}

fn read_f64(conn: &rusqlite::Connection, key: &str) -> f64 {
    db::get_setting(conn, key)
        .ok()
        .flatten()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// Locally-tracked usage: Claude tokens per model + Deepgram audio seconds, plus the
/// user-entered top-up credits. Anthropic has no balance endpoint for a normal key (and
/// Deepgram's needs an owner key), so the frontend turns these into an estimated spend
/// and a remaining figure against the entered credit.
#[tauri::command]
pub fn get_usage(state: State<'_, AppState>) -> Result<UsageReport, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    let since = db::get_setting(&conn, "usage_since")
        .map_err(|e| e.to_string())?
        .and_then(|s| s.parse::<i64>().ok());
    let claude_items = db::get_setting(&conn, "claude_usage")
        .map_err(|e| e.to_string())?
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .and_then(|v| v.as_object().cloned())
        .map(|map| {
            map.into_iter()
                .map(|(model, v)| ClaudeUsageItem {
                    model,
                    input: v["input"].as_u64().unwrap_or(0),
                    output: v["output"].as_u64().unwrap_or(0),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(UsageReport {
        since,
        claude_items,
        deepgram_seconds: read_f64(&conn, "deepgram_seconds"),
        claude_credit: read_f64(&conn, "claude_credit"),
        deepgram_credit: read_f64(&conn, "deepgram_credit"),
    })
}

/// Set (or, when <= 0, clear) the top-up credit the user has on a provider, in USD.
/// `provider` is "claude" or "deepgram".
#[tauri::command]
pub fn set_usage_credit(
    state: State<'_, AppState>,
    provider: String,
    amount: f64,
) -> Result<(), String> {
    let key = match provider.as_str() {
        "claude" => "claude_credit",
        "deepgram" => "deepgram_credit",
        _ => return Err("Unknown provider.".into()),
    };
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    if amount > 0.0 {
        db::set_setting(&conn, key, &amount.to_string()).map_err(|e| e.to_string())
    } else {
        db::delete_setting(&conn, key).map_err(|e| e.to_string())
    }
}

/// Reset the usage tally (Claude tokens + Deepgram seconds + start time). The entered
/// top-up credits are kept.
#[tauri::command]
pub fn reset_usage(state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::delete_setting(&conn, "claude_usage").map_err(|e| e.to_string())?;
    db::delete_setting(&conn, "deepgram_seconds").map_err(|e| e.to_string())?;
    db::delete_setting(&conn, "usage_since").map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeepgramBalance {
    pub amount: f64,
    pub units: String,
    pub project: String,
}

/// Query Deepgram for the remaining credit on the project tied to the stored key.
/// Uses Deepgram's Management API (`/v1/projects` then `/v1/projects/{id}/balances`).
#[tauri::command]
pub async fn deepgram_balance(state: State<'_, AppState>) -> Result<DeepgramBalance, String> {
    // Read the key under the lock, then drop it before any await.
    let key = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        db::get_setting(&conn, "deepgram_api_key")
            .map_err(|e| e.to_string())?
            .filter(|k| !k.trim().is_empty())
            .ok_or("No Deepgram API key set.")?
    };
    let http = state.http.clone();
    let auth = format!("Token {}", key.trim());

    let projects = http
        .get("https://api.deepgram.com/v1/projects")
        .header("Authorization", &auth)
        .send()
        .await
        .map_err(|e| format!("Could not reach Deepgram: {e}"))?;
    if !projects.status().is_success() {
        return Err(match projects.status().as_u16() {
            401 | 403 => "This Deepgram key can't read usage (needs a member/owner key).".into(),
            c => format!("Deepgram returned {c} listing projects."),
        });
    }
    let projects: Value = projects
        .json()
        .await
        .map_err(|e| format!("Unexpected Deepgram response: {e}"))?;
    let project = &projects["projects"][0];
    let pid = project["project_id"]
        .as_str()
        .ok_or("No Deepgram project found for this key.")?;
    let pname = project["name"].as_str().unwrap_or("").to_string();

    let balances = http
        .get(format!(
            "https://api.deepgram.com/v1/projects/{pid}/balances"
        ))
        .header("Authorization", &auth)
        .send()
        .await
        .map_err(|e| format!("Could not reach Deepgram: {e}"))?;
    if !balances.status().is_success() {
        return Err(format!(
            "Deepgram returned {} reading the balance.",
            balances.status().as_u16()
        ));
    }
    let balances: Value = balances
        .json()
        .await
        .map_err(|e| format!("Unexpected Deepgram response: {e}"))?;
    let arr = balances["balances"].as_array();
    let amount = arr
        .map(|b| b.iter().filter_map(|x| x["amount"].as_f64()).sum())
        .unwrap_or(0.0);
    let units = arr
        .and_then(|b| b.first())
        .and_then(|b| b["units"].as_str())
        .unwrap_or("usd")
        .to_string();
    Ok(DeepgramBalance {
        amount,
        units,
        project: pname,
    })
}

/// Start capturing computer audio on a background thread. Transcripts arrive on
/// the frontend as `transcript` events; failures as `listen-error`.
#[tauri::command]
pub fn set_stt_engine(state: State<'_, AppState>, engine: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::set_setting(&conn, "stt_engine", &engine).map_err(|e| e.to_string())
}

/// Deepgram transcription language: a language code ("en", "pt", "es", ...) or "multi"
/// for the multilingual model. Streaming has no auto-detect, so this is chosen up front.
#[tauri::command]
pub fn set_stt_language(state: State<'_, AppState>, language: String) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::set_setting(&conn, "stt_language", &language).map_err(|e| e.to_string())
}

/// Whether to separate the system side into "Speaker 1/2/..." (Deepgram diarization).
/// Off by default - streaming diarization is unstable and mislabels one person on a
/// 1-on-1 call; only worth turning on for genuine multi-person audio.
#[tauri::command]
pub fn set_stt_diarize(state: State<'_, AppState>, diarize: bool) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::set_setting(&conn, "stt_diarize", if diarize { "true" } else { "false" })
        .map_err(|e| e.to_string())
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
        let (language, diarize) = {
            let conn = state.db.lock().map_err(|_| "db poisoned")?;
            let language = db::get_setting(&conn, "stt_language")
                .map_err(|e| e.to_string())?
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "multi".to_string());
            let diarize = db::get_setting(&conn, "stt_diarize")
                .map_err(|e| e.to_string())?
                .map(|v| v == "true")
                .unwrap_or(false);
            (language, diarize)
        };
        crate::listen::EngineConfig::Deepgram {
            key,
            language,
            diarize,
        }
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

// --- Capture mode (floating overlay panels) ---

/// Open (or focus) the floating, always-on-top capture panels.
///
/// Async so it runs off the main thread: creating WebView2 windows needs the main
/// event loop to pump during initialization, so building them from a *synchronous*
/// command (which blocks the main thread) deadlocks - the windows show up blank and
/// the app freezes. Running async lets the runtime dispatch creation to the main
/// loop without blocking it.
#[tauri::command]
pub async fn open_capture_panels(app: AppHandle) -> Result<(), String> {
    crate::capture::open_panels(&app)
}

/// Close all capture panels.
#[tauri::command]
pub fn close_capture_panels(app: AppHandle) {
    crate::capture::close_panels(&app);
}

/// Show/hide the capture panels (the control bar stays). Returns the new visibility.
#[tauri::command]
pub fn toggle_capture_panels(app: AppHandle) -> bool {
    crate::capture::toggle_visibility(&app)
}
