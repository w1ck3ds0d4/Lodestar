//! Tauri commands exposed to the frontend via `invoke`.

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Manager, State};

use crate::db;
use crate::model::{Profile, Session, Story};
use crate::util::now_ms;
use crate::AppState;

// --- Interview story bank ---

#[tauri::command]
pub fn list_stories(state: State<'_, AppState>) -> Result<Vec<Story>, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    let pid = active_profile_id(&conn).ok_or("no active profile")?;
    crate::career::list_stories(&conn, pid).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_story(
    state: State<'_, AppState>,
    title: String,
    content: String,
    tags: String,
) -> Result<i64, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    let pid = active_profile_id(&conn).ok_or("no active profile")?;
    crate::career::add_story(&conn, now_ms(), pid, &title, &content, &tags)
        .map_err(|e| e.to_string())
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

// --- Profiles (switchable interview personas) ---

/// The active profile's id, resolved robustly: the stored setting when it still points at a
/// real profile, otherwise the first profile (which is then persisted). None only when no
/// profile exists at all (should not happen after the migration creates "Default").
fn active_profile_id(conn: &rusqlite::Connection) -> Option<i64> {
    let stored = db::get_setting(conn, "active_profile_id")
        .ok()
        .flatten()
        .and_then(|s| s.parse::<i64>().ok());
    if let Some(id) = stored {
        if crate::career::get_profile(conn, id)
            .ok()
            .flatten()
            .is_some()
        {
            return Some(id);
        }
    }
    let first = crate::career::first_profile_id(conn)?;
    let _ = db::set_setting(conn, "active_profile_id", &first.to_string());
    Some(first)
}

/// Mirror the active profile's scalar fields into the legacy `context_*` settings, so the
/// resume-match scorer and ATS keyterm builder (which read those keys) follow the active
/// profile without each having to know about profiles.
fn mirror_active_profile(conn: &rusqlite::Connection) {
    if let Some(p) =
        active_profile_id(conn).and_then(|id| crate::career::get_profile(conn, id).ok().flatten())
    {
        let _ = db::set_setting(conn, "context_role", &p.role);
        let _ = db::set_setting(conn, "context_company", &p.company);
        let _ = db::set_setting(conn, "context_resume", &p.resume);
        let _ = db::set_setting(conn, "context_notes", &p.notes);
    }
}

#[tauri::command]
pub fn list_profiles(state: State<'_, AppState>) -> Result<Vec<Profile>, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    crate::career::list_profiles(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn active_profile(state: State<'_, AppState>) -> Result<i64, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    active_profile_id(&conn).ok_or_else(|| "no profile".to_string())
}

#[tauri::command]
pub fn set_active_profile(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::set_setting(&conn, "active_profile_id", &id.to_string()).map_err(|e| e.to_string())?;
    mirror_active_profile(&conn);
    Ok(())
}

#[tauri::command]
pub fn create_profile(state: State<'_, AppState>, name: String) -> Result<Profile, String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    let trimmed = name.trim();
    let name = if trimmed.is_empty() {
        "New profile"
    } else {
        trimmed
    };
    let id = crate::career::add_profile(&conn, now_ms(), name).map_err(|e| e.to_string())?;
    crate::career::get_profile(&conn, id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "profile not found after create".to_string())
}

#[tauri::command]
pub fn update_profile(
    state: State<'_, AppState>,
    id: i64,
    name: String,
    role: String,
    company: String,
    resume: String,
    notes: String,
) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    let trimmed = name.trim();
    let name = if trimmed.is_empty() {
        "Profile"
    } else {
        trimmed
    };
    crate::career::update_profile(&conn, now_ms(), id, name, &role, &company, &resume, &notes)
        .map_err(|e| e.to_string())?;
    // Keep the context_* mirror fresh when the edited profile is the active one.
    if active_profile_id(&conn) == Some(id) {
        mirror_active_profile(&conn);
    }
    Ok(())
}

#[tauri::command]
pub fn delete_profile(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    if crate::career::count_profiles(&conn).map_err(|e| e.to_string())? <= 1 {
        return Err("Keep at least one profile.".to_string());
    }
    let was_active =
        db::get_setting(&conn, "active_profile_id").ok().flatten() == Some(id.to_string());
    crate::career::remove_profile(&conn, id).map_err(|e| e.to_string())?;
    if was_active {
        if let Some(first) = crate::career::first_profile_id(&conn) {
            let _ = db::set_setting(&conn, "active_profile_id", &first.to_string());
        }
    }
    mirror_active_profile(&conn);
    Ok(())
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub dir: String,
    pub count: usize,
}

/// Export one saved session's transcript + answers as a Markdown file in the user's
/// Downloads/Lodestar folder. Returns the full path written, so the UI can show it and
/// offer to reveal it. (Sessions otherwise live only in the local database.)
#[tauri::command]
pub fn export_session(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> Result<String, String> {
    let (title, transcript, answers) = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        conn.query_row(
            "SELECT title, transcript, answers FROM sessions WHERE id = ?1",
            [id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            },
        )
        .map_err(|e| format!("Could not read the session: {e}"))?
    };
    write_session_md(&app, id, &title, &transcript, &answers)
}

/// Export every saved session to its own Markdown file in Downloads/Lodestar. Returns
/// the folder and how many were written.
#[tauri::command]
pub fn export_all_sessions(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<ExportResult, String> {
    let sessions = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        crate::career::list_sessions(&conn).map_err(|e| e.to_string())?
    };
    for s in &sessions {
        write_session_md(&app, s.id, &s.title, &s.transcript, &s.answers)?;
    }
    Ok(ExportResult {
        dir: export_dir(&app)?.to_string_lossy().to_string(),
        count: sessions.len(),
    })
}

/// Export destination: <Downloads>/Lodestar (falling back to Documents, then app data),
/// created if missing.
fn export_dir(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    let base = app
        .path()
        .download_dir()
        .or_else(|_| app.path().document_dir())
        .or_else(|_| app.path().app_data_dir())
        .map_err(|e| e.to_string())?;
    let dir = base.join("Lodestar");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir)
}

/// Write one session as a Markdown file in the export dir; returns the full path.
fn write_session_md(
    app: &AppHandle,
    id: i64,
    title: &str,
    transcript: &str,
    answers: &str,
) -> Result<String, String> {
    let mut md = format!("# {}\n\n", title.trim());
    if !transcript.trim().is_empty() {
        md.push_str("## Transcript\n\n");
        md.push_str(transcript.trim());
        md.push_str("\n\n");
    }
    if !answers.trim().is_empty() {
        md.push_str("## Answers\n\n");
        md.push_str(answers.trim());
        md.push('\n');
    }
    // Suffix the id so same-titled (e.g. two auto-saved) sessions never collide.
    let path = export_dir(app)?.join(format!("{} ({}).md", sanitize_filename(title), id));
    std::fs::write(&path, md).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().to_string())
}

/// Make a session title safe to use as a filename: keep alphanumerics plus a few safe
/// punctuation chars, replace the rest with '_', strip trailing dots, and cap length.
fn sanitize_filename(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '.' | ',') {
                c
            } else {
                '_'
            }
        })
        .collect();
    let cleaned: String = cleaned
        .trim()
        .trim_end_matches('.')
        .trim()
        .chars()
        .take(80)
        .collect();
    if cleaned.is_empty() {
        "session".to_string()
    } else {
        cleaned
    }
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

// Sonnet is the live-assistant default: much lower time-to-first-token than Opus
// and strong enough for mid-call Q&A. Opus stays selectable in Settings.
const DEFAULT_MODEL: &str = "claude-opus-5";
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
            .map(|k| crate::secret::unprotect(&k))
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
/// Mine short keyterm-prompting terms (proper nouns, company/role, ALLCAPS acronyms)
/// from the interview context so nova-3 recognises them. The pure core
/// ([`keyterms_from_sources`]) is unit-tested without a live key. Empty vec = no keyterms
/// attached. The count + token budget stay well under Deepgram's ~500-token/request cap.
fn interview_keyterms(conn: &rusqlite::Connection) -> Vec<String> {
    let get = |k: &str| db::get_setting(conn, k).ok().flatten().unwrap_or_default();
    // (text, allow_single_titlecase): long free-text fields are scanned only for
    // acronyms and multi-word proper phrases - a lone capitalised word there is usually
    // a sentence start (noise). Short, high-signal fields also yield single TitleCase words.
    let mut sources: Vec<(String, bool)> = vec![
        (get("context_role"), true),
        (get("context_company"), true),
        (get("context_resume"), false),
        (get("context_notes"), false),
    ];
    // Verbatim high-signal phrases: the role line and the deliberately-chosen story tags.
    let mut verbatim: Vec<String> = Vec::new();
    let role = get("context_role");
    let role = role.trim();
    if !role.is_empty() && role.split_whitespace().count() <= 6 {
        verbatim.push(role.to_string());
    }
    if let Ok(stories) = crate::career::list_stories(conn, active_profile_id(conn).unwrap_or(0)) {
        for s in &stories {
            sources.push((s.title.clone(), true));
            for tag in s.tags.split(',') {
                let tag = tag.trim();
                if !tag.is_empty() && tag.split_whitespace().count() <= 4 {
                    verbatim.push(tag.to_string());
                }
            }
        }
    }
    keyterms_from_sources(&sources, &verbatim)
}

/// Pure core of [`interview_keyterms`]: dedupe (case-insensitive) + length/stopword
/// filter + budget-cap the mined terms so a long resume cannot blow Deepgram's cap.
fn keyterms_from_sources(sources: &[(String, bool)], verbatim: &[String]) -> Vec<String> {
    let mut terms: Vec<String> = Vec::new();
    {
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut emit = |raw: &str| {
            let t = raw.trim().trim_matches(|c: char| !c.is_alphanumeric());
            if t.chars().count() < 2 || t.len() > 40 || is_keyterm_stopword(t) {
                return;
            }
            if seen.insert(t.to_lowercase()) {
                terms.push(t.to_string());
            }
        };
        for v in verbatim {
            emit(v);
        }
        for (text, allow_single) in sources {
            scan_keyterms(text, *allow_single, &mut emit);
        }
    }
    // Cap by term count AND an estimated token budget (whitespace words) so a long
    // resume + many stories cannot exceed Deepgram's ~500-token/request keyterm limit.
    let mut out: Vec<String> = Vec::new();
    let mut budget = 0usize;
    for t in terms {
        let words = t.split_whitespace().count().max(1);
        if out.len() >= 48 || budget + words > 80 {
            break;
        }
        budget += words;
        out.push(t);
    }
    out
}

/// Scan text for acronyms (ALLCAPS) and proper-noun phrases (runs of TitleCase words),
/// emitting each candidate. Single TitleCase words are emitted only when `allow_single`
/// (short, high-signal fields), since in free text they are mostly sentence starts.
fn scan_keyterms(text: &str, allow_single: bool, emit: &mut dyn FnMut(&str)) {
    let is_word = |c: char| c.is_alphanumeric() || matches!(c, '+' | '#' | '.');
    let mut words: Vec<&str> = Vec::new();
    let mut start: Option<usize> = None;
    for (i, c) in text.char_indices() {
        if is_word(c) {
            if start.is_none() {
                start = Some(i);
            }
        } else if let Some(s) = start.take() {
            words.push(&text[s..i]);
        }
    }
    if let Some(s) = start {
        words.push(&text[s..]);
    }

    let is_titlecase = |w: &str| {
        let mut cs = w.chars();
        matches!(cs.next(), Some(c) if c.is_uppercase()) && w.chars().any(|c| c.is_lowercase())
    };
    let is_acronym = |w: &str| {
        let n = w.chars().count();
        (2..=8).contains(&n)
            && w.chars().all(|c| c.is_uppercase() || c.is_ascii_digit())
            && w.chars().any(|c| c.is_alphabetic())
    };

    let mut i = 0;
    while i < words.len() {
        let w = words[i];
        if is_acronym(w) {
            emit(w);
            i += 1;
        } else if is_titlecase(w) {
            let mut j = i + 1;
            while j < words.len() && is_titlecase(words[j]) {
                j += 1;
            }
            if j - i >= 2 {
                // Multi-word proper phrase (cap to the first 4 words).
                let end = (i + 4).min(j);
                let phrase = words[i..end].join(" ");
                emit(&phrase);
            } else if allow_single {
                emit(w);
            }
            i = j;
        } else {
            i += 1;
        }
    }
}

fn is_keyterm_stopword(t: &str) -> bool {
    const STOP: &[&str] = &[
        "the",
        "and",
        "for",
        "with",
        "you",
        "your",
        "our",
        "their",
        "this",
        "that",
        "from",
        "into",
        "have",
        "has",
        "was",
        "were",
        "will",
        "would",
        "should",
        "could",
        "about",
        "over",
        "under",
        "team",
        "role",
        "company",
        "resume",
        "notes",
        "experience",
        "interview",
        "candidate",
        "monday",
        "tuesday",
        "wednesday",
        "thursday",
        "friday",
        "saturday",
        "sunday",
        "january",
        "february",
        "march",
        "april",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    STOP.contains(&t.to_lowercase().as_str())
}

#[cfg(test)]
mod keyterm_tests {
    use super::keyterms_from_sources;

    #[test]
    fn mines_acronyms_and_phrases_and_dedupes() {
        let sources = vec![
            ("Senior Platform Engineer at Acme Corp".to_string(), true),
            (
                "We use AWS and Kubernetes. The team ships fast. AWS again.".to_string(),
                false,
            ),
        ];
        let verbatim = vec!["Senior Platform Engineer".to_string(), "rust".to_string()];
        let terms = keyterms_from_sources(&sources, &verbatim);
        let lower: Vec<String> = terms.iter().map(|t| t.to_lowercase()).collect();
        // Verbatim role phrase kept; acronym mined and deduped despite appearing twice.
        assert!(lower.iter().any(|t| t == "senior platform engineer"));
        assert_eq!(lower.iter().filter(|t| *t == "aws").count(), 1);
        // Multi-word proper phrase from a high-signal field.
        assert!(lower.iter().any(|t| t == "acme corp"));
        // A lone TitleCase word in a long free-text field is treated as noise (skipped).
        assert!(!lower.iter().any(|t| t == "kubernetes"));
        assert!(!lower.iter().any(|t| t == "the"));
    }

    #[test]
    fn respects_budget_cap() {
        let big = (0..200)
            .map(|i| format!("AB{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let terms = keyterms_from_sources(&[(big, false)], &[]);
        assert!(!terms.is_empty());
        assert!(terms.len() <= 48, "expected <=48, got {}", terms.len());
    }
}

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
    if let Ok(stories) = crate::career::list_stories(conn, active_profile_id(conn).unwrap_or(0)) {
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
    /// Whether the capture overlay panels are hidden from screen capture (default true).
    pub capture_protect: bool,
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
        capture_protect: capture_protect_setting(&conn),
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
    // The interview context now belongs to the active profile: update it (keeping its name)
    // and refresh the context_* mirror that the matcher and keyterm builder read.
    let pid = active_profile_id(&conn).ok_or("no active profile")?;
    let name = crate::career::get_profile(&conn, pid)
        .ok()
        .flatten()
        .map(|p| p.name)
        .unwrap_or_else(|| "Profile".to_string());
    crate::career::update_profile(
        &conn,
        now_ms(),
        pid,
        &name,
        role.trim(),
        company.trim(),
        resume.trim(),
        notes.trim(),
    )
    .map_err(|e| e.to_string())?;
    mirror_active_profile(&conn);
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
///
/// Async + `spawn_blocking` because CPAL enumerates devices through WASAPI, which makes
/// blocking COM calls that can stall for seconds when a device is in a bad state (a
/// Bluetooth headset reconnecting, a sleeping USB interface, a flaky virtual cable). As a
/// plain synchronous command this ran on the main thread, so the stall froze the whole
/// window - Windows would mark it "Not Responding" and ghost its taskbar icon. The timeout
/// guards against a wedged audio driver hanging enumeration forever: on timeout we return
/// no devices and the UI falls back to the system default. The detached blocking task is
/// harmless - it finishes on the blocking pool without ever touching the UI thread.
#[tauri::command]
pub async fn list_audio_devices() -> Result<AudioDevices, String> {
    let enumerate = tokio::task::spawn_blocking(|| {
        let (inputs, outputs) = crate::listen::list_devices();
        AudioDevices { inputs, outputs }
    });
    match tokio::time::timeout(std::time::Duration::from_secs(5), enumerate).await {
        Ok(Ok(devices)) => Ok(devices),
        // Join error (the blocking task panicked) or timeout: degrade to an empty list
        // rather than surfacing an error, so the picker just shows "system default".
        _ => Ok(AudioDevices {
            inputs: Vec::new(),
            outputs: Vec::new(),
        }),
    }
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
        db::set_setting(&conn, "anthropic_api_key", &crate::secret::protect(trimmed))
            .map_err(|e| e.to_string())
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

/// Ask about an attached image (a pasted/dropped screenshot) with an optional text question.
/// `image` is base64 PNG (no data: prefix). Streams the answer like `ask_assistant`.
#[tauri::command]
pub async fn ask_image(
    state: State<'_, AppState>,
    question: String,
    image: String,
    context: Option<String>,
    on_event: tauri::ipc::Channel<AnswerChunk>,
) -> Result<(), String> {
    if image.trim().is_empty() {
        let _ = on_event.send(AnswerChunk::Error {
            message: "No image attached.".into(),
        });
        return Ok(());
    }
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
    match crate::assistant::ask_image(
        &http,
        &engine,
        &question,
        &image,
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
///
/// Async + `spawn_blocking` so the OS monitor query runs off the main thread, matching
/// `list_audio_devices` - both fire together when the Assistant tab mounts, and neither
/// should be able to stall the window's event loop.
#[tauri::command]
pub async fn list_monitors() -> Result<Vec<MonitorInfo>, String> {
    tokio::task::spawn_blocking(|| {
        (0..crate::screen::count())
            .map(|id| MonitorInfo {
                id,
                name: format!("Display {}", id + 1),
            })
            .collect()
    })
    .await
    .map_err(|e| e.to_string())
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
        // Skip when the screen is perceptually the same as the last ANSWERED one. The
        // hash is a dHash, compared by Hamming distance (differing bits): a moved cursor
        // or a ticking clock barely shifts it, while a new exercise flips many bits. This
        // is read-only - the baseline is advanced later, only after a real answer, so a
        // transient/NONE frame can never become the baseline and swallow the next real
        // question. The lock is dropped before the vision call below.
        const SCREEN_HASH_THRESHOLD: u32 = 4;
        if !force {
            let last = state
                .screen_hash
                .lock()
                .map_err(|_| "screen hash poisoned")?;
            if let Some(prev) = *last {
                if (prev ^ cap.hash).count_ones() <= SCREEN_HASH_THRESHOLD {
                    return Ok(ScreenAnswer {
                        changed: false,
                        answer: String::new(),
                    });
                }
            }
        }
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
        None,
    )
    .await
    .map_err(|e| e.to_string())?;

    // Advance the dedup baseline only for a real, answerable screen on the watcher path:
    // never for a forced one-off (so a manual answer can't perturb the watcher) and never
    // for a NONE / empty result (so a between-questions frame can't poison the baseline).
    let real = {
        let a = answer.trim();
        !a.is_empty() && !a.eq_ignore_ascii_case("NONE")
    };
    if real && !force {
        if let Ok(mut last) = state.screen_hash.lock() {
            *last = Some(cap.hash);
        }
    }
    Ok(ScreenAnswer {
        changed: true,
        answer,
    })
}

/// Like `answer_screen`, but also reads the focused window's full text via UI Automation
/// (including content scrolled off-screen) and sends it alongside the screenshot, so a coding
/// exercise that spills past the visible viewport can still be solved. Meant for the phone
/// remote: the PC keeps the browser focused while you tap, so the read targets the page, not
/// Lodestar. A failed or empty text read just falls back to answering from the image alone.
#[tauri::command]
pub async fn answer_screen_hybrid(
    state: State<'_, AppState>,
    monitor: usize,
    force: bool,
    context: Option<String>,
) -> Result<ScreenAnswer, String> {
    let cap = crate::screen::capture(monitor).map_err(|e| e.to_string())?;

    {
        // Same image-hash dedup as answer_screen for the watch path; a forced call bypasses it.
        const SCREEN_HASH_THRESHOLD: u32 = 4;
        if !force {
            let last = state
                .screen_hash
                .lock()
                .map_err(|_| "screen hash poisoned")?;
            if let Some(prev) = *last {
                if (prev ^ cap.hash).count_ones() <= SCREEN_HASH_THRESHOLD {
                    return Ok(ScreenAnswer {
                        changed: false,
                        answer: String::new(),
                    });
                }
            }
        }
    }

    // Best-effort full text of the focused window. UI Automation is blocking COM, so run it
    // off the async executor. Any failure (no accessible text, canvas, locked page) just means
    // we answer from the screenshot alone.
    let page_text = tokio::task::spawn_blocking(crate::winauto::read_focused_window_text)
        .await
        .ok()
        .and_then(|r| r.ok());

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
        page_text.as_deref(),
    )
    .await
    .map_err(|e| e.to_string())?;

    let real = {
        let a = answer.trim();
        !a.is_empty() && !a.eq_ignore_ascii_case("NONE")
    };
    if real && !force {
        if let Ok(mut last) = state.screen_hash.lock() {
            *last = Some(cap.hash);
        }
    }
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
        db::set_setting(&conn, "deepgram_api_key", &crate::secret::protect(trimmed))
            .map_err(|e| e.to_string())
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

/// Check whether Windows networking/firewall is likely to block phone connections, so the
/// UI can warn up front. Runs the inspection off the main thread (it shells out to
/// PowerShell on Windows); never errors - an unreadable environment reports "ok".
#[tauri::command]
pub async fn phone_network_status() -> Result<crate::netcheck::PhoneNetStatus, String> {
    tokio::task::spawn_blocking(crate::netcheck::status)
        .await
        .map_err(|e| e.to_string())
}

/// Mark the current network Private and add a LAN-only inbound rule for the companion port.
/// Prompts the user for elevation (UAC); returns the refreshed status. Off the main thread
/// because it blocks on the elevated child process.
#[tauri::command]
pub async fn phone_allow_firewall() -> Result<crate::netcheck::PhoneNetStatus, String> {
    tokio::task::spawn_blocking(crate::netcheck::allow)
        .await
        .map_err(|e| e.to_string())?
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
    /// Manual "actual spent" corrections, in USD. The local figure is only an estimate
    /// (tokens x list price / minutes x rate); when set, the UI shows this instead so
    /// the user can sync it to what their provider console actually reports. `None` =
    /// no override, use the estimate.
    pub claude_spent_override: Option<f64>,
    pub deepgram_spent_override: Option<f64>,
}

/// Read a setting as an optional f64 (absent / unparseable -> None).
fn read_opt_f64(conn: &rusqlite::Connection, key: &str) -> Option<f64> {
    db::get_setting(conn, key)
        .ok()
        .flatten()
        .and_then(|s| s.parse::<f64>().ok())
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
        claude_spent_override: read_opt_f64(&conn, "claude_spent_override"),
        deepgram_spent_override: read_opt_f64(&conn, "deepgram_spent_override"),
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

/// Override the "actual spent" figure for a provider, in USD, so the user can correct
/// the local estimate to match what their console reports. `provider` is "claude" or
/// "deepgram"; a negative `amount` clears the override (back to the estimate). Zero is
/// a valid override (the user really has spent nothing).
#[tauri::command]
pub fn set_usage_spent(
    state: State<'_, AppState>,
    provider: String,
    amount: f64,
) -> Result<(), String> {
    let key = match provider.as_str() {
        "claude" => "claude_spent_override",
        "deepgram" => "deepgram_spent_override",
        _ => return Err("Unknown provider.".into()),
    };
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    if amount >= 0.0 {
        db::set_setting(&conn, key, &amount.to_string()).map_err(|e| e.to_string())
    } else {
        db::delete_setting(&conn, key).map_err(|e| e.to_string())
    }
}

/// Reset the usage tally (Claude tokens + Deepgram seconds + start time + any manual
/// spent overrides). The entered top-up credits are kept.
#[tauri::command]
pub fn reset_usage(state: State<'_, AppState>) -> Result<(), String> {
    let conn = state.db.lock().map_err(|_| "db poisoned")?;
    db::delete_setting(&conn, "claude_usage").map_err(|e| e.to_string())?;
    db::delete_setting(&conn, "deepgram_seconds").map_err(|e| e.to_string())?;
    db::delete_setting(&conn, "usage_since").map_err(|e| e.to_string())?;
    db::delete_setting(&conn, "claude_spent_override").map_err(|e| e.to_string())?;
    db::delete_setting(&conn, "deepgram_spent_override").map_err(|e| e.to_string())?;
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
            .map(|k| crate::secret::unprotect(&k))
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
        let key = key
            .map(|k| crate::secret::unprotect(&k))
            .filter(|k| !k.trim().is_empty())
            .ok_or(
            "No Deepgram API key set. Add it in the Assistant tab, or switch the engine to Local.",
        )?;
        let (language, diarize, keyterms) = {
            let conn = state.db.lock().map_err(|_| "db poisoned")?;
            let language = db::get_setting(&conn, "stt_language")
                .map_err(|e| e.to_string())?
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| "multi".to_string());
            let diarize = db::get_setting(&conn, "stt_diarize")
                .map_err(|e| e.to_string())?
                .map(|v| v == "true")
                .unwrap_or(false);
            // Mine proper nouns / company / role / acronyms from the same interview
            // context the prompt uses, to keyterm-prompt nova-3 (attached only when a
            // specific language is selected; see EngineConfig::Deepgram::keyterms).
            let keyterms = interview_keyterms(&conn);
            (language, diarize, keyterms)
        };
        crate::listen::EngineConfig::Deepgram {
            key,
            language,
            diarize,
            keyterms,
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
pub async fn open_capture_panels(
    app: AppHandle,
    state: State<'_, AppState>,
    monitor: usize,
) -> Result<(), String> {
    // Honor the saved "hide panels from screen capture" preference (default: on), and
    // remember the chosen monitor so a later protection toggle reopens on the same one.
    let protected = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        let _ = db::set_setting(&conn, "capture_monitor", &monitor.to_string());
        capture_protect_setting(&conn)
    };
    // Resolve the chosen monitor to Tauri's coordinate space and open the overlay there.
    let bounds = crate::capture::monitor_bounds(&app, monitor);
    crate::capture::open_panels(&app, protected, bounds)
}

/// Close all capture panels.
#[tauri::command]
pub fn close_capture_panels(app: AppHandle) {
    crate::capture::close_panels(&app);
}

/// Set the translucency (0.0..=1.0) of the capture overlay (bar + panels) together.
#[tauri::command]
pub fn set_capture_opacity(app: AppHandle, opacity: f64) -> Result<(), String> {
    crate::capture::set_panels_opacity(&app, opacity);
    Ok(())
}

/// Read the "capture panels are hidden from screen capture" preference (default true).
fn capture_protect_setting(conn: &rusqlite::Connection) -> bool {
    db::get_setting(conn, "capture_protect")
        .ok()
        .flatten()
        .map(|v| v != "false")
        .unwrap_or(true)
}

/// Toggle whether the capture overlay panels are hidden from screen capture/sharing.
/// `protected` true (default) keeps them invisible to a shared/recorded screen; false
/// makes them appear in screenshots and shares so the user can capture the panels
/// themselves. Persisted and applied live to any open panels.
#[tauri::command]
pub async fn set_capture_protection(
    app: AppHandle,
    state: State<'_, AppState>,
    protected: bool,
) -> Result<(), String> {
    let monitor = {
        let conn = state.db.lock().map_err(|_| "db poisoned")?;
        db::set_setting(
            &conn,
            "capture_protect",
            if protected { "true" } else { "false" },
        )
        .map_err(|e| e.to_string())?;
        db::get_setting(&conn, "capture_monitor")
            .ok()
            .flatten()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0)
    };
    // Apply the new protection live by recreating the open panels (toggling it on a live
    // window does not lift the Windows capture exclusion). Destroy them, let the event
    // loop process the close, then reopen on the same monitor and restore the layout.
    // Async so window creation can pump the main loop (a sync command would deadlock).
    let geo = crate::capture::snapshot_and_destroy(&app);
    if geo.is_empty() {
        return Ok(()); // panels not open; the saved setting applies on the next open
    }
    tokio::time::sleep(std::time::Duration::from_millis(140)).await;
    let bounds = crate::capture::monitor_bounds(&app, monitor);
    crate::capture::open_panels(&app, protected, bounds)?;
    crate::capture::restore_geometry(&app, geo);
    Ok(())
}

/// Show/hide the capture panels (the control bar stays). Returns the new visibility.
#[tauri::command]
pub fn toggle_capture_panels(app: AppHandle) -> bool {
    crate::capture::toggle_visibility(&app)
}

/// Move or resize a capture panel (or "all") from the phone remote: snap it to a corner/center
/// (tl/tr/bl/br/center), nudge it a step (up/down/left/right), or resize it (grow/shrink).
#[tauri::command]
pub fn move_capture_panel(
    app: AppHandle,
    panel: String,
    snap: Option<String>,
    nudge: Option<String>,
    resize: Option<String>,
) -> Result<(), String> {
    crate::capture::move_panel(
        &app,
        &panel,
        snap.as_deref(),
        nudge.as_deref(),
        resize.as_deref(),
    )
}

/// Read the full text of the page the user is on (including content scrolled off-screen) and
/// return it, so the caller can attach it to context. Works from an in-app button or the
/// phone - which would otherwise focus Lodestar - by targeting the last external window the
/// user was on. UI Automation can be slow, so it runs on a blocking thread.
#[tauri::command]
pub async fn read_active_page() -> Result<String, String> {
    tokio::task::spawn_blocking(crate::winauto::read_active_page)
        .await
        .map_err(|e| format!("read task failed: {e}"))?
        .map_err(|e| e.to_string())
}
