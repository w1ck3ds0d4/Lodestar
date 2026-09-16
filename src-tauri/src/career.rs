//! Per-profile data: interview profiles, the STAR story bank, and saved sessions.

use anyhow::Result;
use rusqlite::{params, Connection};

use crate::model::{Profile, Session, Story};

// --- Interview story bank (reusable STAR stories) ---

fn row_to_story(r: &rusqlite::Row) -> rusqlite::Result<Story> {
    Ok(Story {
        id: r.get(0)?,
        title: r.get(1)?,
        content: r.get(2)?,
        tags: r.get(3)?,
        created_at: r.get(4)?,
        updated_at: r.get(5)?,
    })
}

pub fn list_stories(conn: &Connection, profile_id: i64) -> Result<Vec<Story>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, content, tags, created_at, updated_at FROM stories \
         WHERE profile_id = ?1 ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map(params![profile_id], row_to_story)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn add_story(
    conn: &Connection,
    now: i64,
    profile_id: i64,
    title: &str,
    content: &str,
    tags: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO stories (profile_id, title, content, tags, created_at, updated_at) \
         VALUES (?1,?2,?3,?4,?5,?5)",
        params![profile_id, title, content, tags, now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_story(
    conn: &Connection,
    now: i64,
    id: i64,
    title: &str,
    content: &str,
    tags: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE stories SET title=?2, content=?3, tags=?4, updated_at=?5 WHERE id=?1",
        params![id, title, content, tags, now],
    )?;
    Ok(())
}

pub fn remove_story(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM stories WHERE id=?1", params![id])?;
    Ok(())
}

// --- Profiles (switchable interview personas) ---

const PROFILE_COLS: &str = "id, name, role, company, resume, notes, created_at, updated_at";

fn row_to_profile(r: &rusqlite::Row) -> rusqlite::Result<Profile> {
    Ok(Profile {
        id: r.get(0)?,
        name: r.get(1)?,
        role: r.get(2)?,
        company: r.get(3)?,
        resume: r.get(4)?,
        notes: r.get(5)?,
        created_at: r.get(6)?,
        updated_at: r.get(7)?,
    })
}

pub fn list_profiles(conn: &Connection) -> Result<Vec<Profile>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {PROFILE_COLS} FROM profiles ORDER BY created_at, id"
    ))?;
    let rows = stmt.query_map([], row_to_profile)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn get_profile(conn: &Connection, id: i64) -> Result<Option<Profile>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {PROFILE_COLS} FROM profiles WHERE id = ?1"
    ))?;
    let mut rows = stmt.query_map(params![id], row_to_profile)?;
    Ok(rows.next().transpose()?)
}

pub fn add_profile(conn: &Connection, now: i64, name: &str) -> Result<i64> {
    conn.execute(
        "INSERT INTO profiles (name, created_at, updated_at) VALUES (?1, ?2, ?2)",
        params![name, now],
    )?;
    Ok(conn.last_insert_rowid())
}

#[allow(clippy::too_many_arguments)]
pub fn update_profile(
    conn: &Connection,
    now: i64,
    id: i64,
    name: &str,
    role: &str,
    company: &str,
    resume: &str,
    notes: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE profiles SET name=?2, role=?3, company=?4, resume=?5, notes=?6, updated_at=?7 \
         WHERE id=?1",
        params![id, name, role, company, resume, notes, now],
    )?;
    Ok(())
}

/// Delete a profile and the stories that belong to it.
pub fn remove_profile(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM stories WHERE profile_id=?1", params![id])?;
    conn.execute("DELETE FROM profiles WHERE id=?1", params![id])?;
    Ok(())
}

pub fn count_profiles(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM profiles", [], |r| r.get(0))?)
}

/// The first profile's id (creation order), used as the fallback active profile.
pub fn first_profile_id(conn: &Connection) -> Option<i64> {
    conn.query_row(
        "SELECT id FROM profiles ORDER BY created_at, id LIMIT 1",
        [],
        |r| r.get(0),
    )
    .ok()
}

// --- Interview session history ---

fn row_to_session(r: &rusqlite::Row) -> rusqlite::Result<Session> {
    Ok(Session {
        id: r.get(0)?,
        title: r.get(1)?,
        transcript: r.get(2)?,
        answers: r.get(3)?,
        created_at: r.get(4)?,
    })
}

pub fn list_sessions(conn: &Connection) -> Result<Vec<Session>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, transcript, answers, created_at FROM sessions ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([], row_to_session)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Sessions not yet analysed for the debrief, oldest first so the picture fills in order.
pub fn sessions_without_analysis(
    conn: &Connection,
    limit: i64,
) -> Result<Vec<(i64, String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, transcript, answers FROM sessions          WHERE analysis IS NULL OR analysis = '' ORDER BY created_at ASC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limit], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Cache a session's topic verdicts so it is never analysed twice.
pub fn set_session_analysis(conn: &Connection, id: i64, analysis: &str) -> Result<()> {
    conn.execute(
        "UPDATE sessions SET analysis=?2 WHERE id=?1",
        params![id, analysis],
    )?;
    Ok(())
}

/// Every cached analysis, newest session first (so the freshest note wins on aggregation).
pub fn session_analyses(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT analysis FROM sessions WHERE analysis IS NOT NULL AND analysis <> ''          ORDER BY created_at DESC",
    )?;
    let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// (total sessions, sessions already analysed).
pub fn session_counts(conn: &Connection) -> Result<(i64, i64)> {
    let total = conn.query_row("SELECT COUNT(*) FROM sessions", [], |r| r.get(0))?;
    let done = conn.query_row(
        "SELECT COUNT(*) FROM sessions WHERE analysis IS NOT NULL AND analysis <> ''",
        [],
        |r| r.get(0),
    )?;
    Ok((total, done))
}

pub fn add_session(
    conn: &Connection,
    now: i64,
    title: &str,
    transcript: &str,
    answers: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO sessions (title, transcript, answers, created_at) VALUES (?1,?2,?3,?4)",
        params![title, transcript, answers, now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn remove_session(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM sessions WHERE id=?1", params![id])?;
    Ok(())
}
