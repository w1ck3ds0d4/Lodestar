//! Career tracking: a job-application + training-program pipeline (with
//! per-item milestones) and a study/skills log.

use anyhow::Result;
use rusqlite::{params, Connection};

use crate::model::{Milestone, Session, Skill, Story, Tracked};
use crate::util::epoch_to_date;

const TRACKED_COLS: &str = "id, kind, title, org, url, status, notes, applied_at, deadline, \
    progress, source_job_id, created_at, updated_at, next_action, prep";

fn row_to_tracked(r: &rusqlite::Row) -> rusqlite::Result<Tracked> {
    Ok(Tracked {
        id: r.get(0)?,
        kind: r.get(1)?,
        title: r.get(2)?,
        org: r.get(3)?,
        url: r.get(4)?,
        status: r.get(5)?,
        notes: r.get(6)?,
        applied_at: r.get(7)?,
        deadline: r.get(8)?,
        progress: r.get(9)?,
        source_job_id: r.get(10)?,
        created_at: r.get(11)?,
        updated_at: r.get(12)?,
        next_action: r.get(13)?,
        prep: r.get(14)?,
        milestones: Vec::new(),
    })
}

/// Set (or clear) an application's "next action" reminder.
pub fn set_next_action(conn: &Connection, now: i64, id: i64, text: Option<&str>) -> Result<()> {
    let text = text.filter(|t| !t.trim().is_empty());
    conn.execute(
        "UPDATE tracked SET next_action=?2, updated_at=?3 WHERE id=?1",
        params![id, text, now],
    )?;
    Ok(())
}

/// Store a generated interview-prep brief for an application.
pub fn set_prep(conn: &Connection, id: i64, prep: &str) -> Result<()> {
    conn.execute("UPDATE tracked SET prep=?2 WHERE id=?1", params![id, prep])?;
    Ok(())
}

pub fn list_tracked(conn: &Connection, kind: Option<&str>) -> Result<Vec<Tracked>> {
    let mut items: Vec<Tracked> = match kind {
        Some(k) => {
            let mut stmt = conn.prepare(&format!(
                "SELECT {TRACKED_COLS} FROM tracked WHERE kind = ?1 ORDER BY updated_at DESC"
            ))?;
            let rows = stmt.query_map(params![k], row_to_tracked)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        }
        None => {
            let mut stmt = conn.prepare(&format!(
                "SELECT {TRACKED_COLS} FROM tracked ORDER BY updated_at DESC"
            ))?;
            let rows = stmt.query_map([], row_to_tracked)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        }
    };
    for t in items.iter_mut() {
        t.milestones = list_milestones(conn, t.id)?;
    }
    Ok(items)
}

const APPLIED_STATUSES: &str = "'applied','screen','interview','offer','accepted'";

#[allow(clippy::too_many_arguments)]
pub fn add_tracked(
    conn: &Connection,
    now: i64,
    kind: &str,
    title: &str,
    org: &str,
    url: &str,
    status: &str,
    notes: &str,
    deadline: Option<&str>,
    source_job_id: Option<i64>,
) -> Result<i64> {
    let applied_at = if status != "saved" && status != "interested" {
        Some(epoch_to_date(now / 1000))
    } else {
        None
    };
    conn.execute(
        "INSERT INTO tracked (kind, title, org, url, status, notes, applied_at, deadline, progress, source_job_id, created_at, updated_at) \
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,0,?9,?10,?10)",
        params![kind, title, org, url, status, notes, applied_at, deadline, source_job_id, now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_tracked(
    conn: &Connection,
    now: i64,
    id: i64,
    status: &str,
    notes: &str,
    progress: i64,
    deadline: Option<&str>,
) -> Result<()> {
    let today = epoch_to_date(now / 1000);
    // Stamp applied_at the first time the item reaches an "applied or later" status.
    conn.execute(
        &format!(
            "UPDATE tracked SET status=?2, notes=?3, progress=?4, deadline=?5, updated_at=?6, \
                applied_at = COALESCE(applied_at, CASE WHEN ?2 IN ({APPLIED_STATUSES}) THEN ?7 ELSE NULL END) \
             WHERE id=?1"
        ),
        params![id, status, notes, progress.clamp(0, 100), deadline, now, today],
    )?;
    Ok(())
}

pub fn remove_tracked(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM tracked WHERE id=?1", params![id])?;
    Ok(())
}

fn row_to_milestone(r: &rusqlite::Row) -> rusqlite::Result<Milestone> {
    Ok(Milestone {
        id: r.get(0)?,
        tracked_id: r.get(1)?,
        label: r.get(2)?,
        done: r.get::<_, i64>(3)? != 0,
        position: r.get(4)?,
    })
}

pub fn list_milestones(conn: &Connection, tracked_id: i64) -> Result<Vec<Milestone>> {
    let mut stmt = conn.prepare(
        "SELECT id, tracked_id, label, done, position FROM milestones WHERE tracked_id=?1 ORDER BY position, id",
    )?;
    let rows = stmt.query_map(params![tracked_id], row_to_milestone)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn add_milestone(conn: &Connection, tracked_id: i64, label: &str) -> Result<i64> {
    let pos: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(position), -1) + 1 FROM milestones WHERE tracked_id=?1",
            params![tracked_id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    conn.execute(
        "INSERT INTO milestones (tracked_id, label, done, position) VALUES (?1,?2,0,?3)",
        params![tracked_id, label, pos],
    )?;
    let id = conn.last_insert_rowid();
    recompute_progress(conn, tracked_id)?;
    Ok(id)
}

pub fn set_milestone(conn: &Connection, id: i64, done: bool) -> Result<()> {
    conn.execute(
        "UPDATE milestones SET done=?2 WHERE id=?1",
        params![id, done as i64],
    )?;
    if let Some(tid) = milestone_parent(conn, id) {
        recompute_progress(conn, tid)?;
    }
    Ok(())
}

pub fn remove_milestone(conn: &Connection, id: i64) -> Result<()> {
    let tid = milestone_parent(conn, id);
    conn.execute("DELETE FROM milestones WHERE id=?1", params![id])?;
    if let Some(tid) = tid {
        recompute_progress(conn, tid)?;
    }
    Ok(())
}

fn milestone_parent(conn: &Connection, id: i64) -> Option<i64> {
    conn.query_row(
        "SELECT tracked_id FROM milestones WHERE id=?1",
        params![id],
        |r| r.get(0),
    )
    .ok()
}

/// Recompute a tracked item's progress as the share of completed milestones.
fn recompute_progress(conn: &Connection, tracked_id: i64) -> Result<()> {
    let (total, done): (i64, i64) = conn.query_row(
        "SELECT COUNT(*), COALESCE(SUM(done), 0) FROM milestones WHERE tracked_id=?1",
        params![tracked_id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if total > 0 {
        let pct = (done * 100 / total).clamp(0, 100);
        conn.execute(
            "UPDATE tracked SET progress=?2 WHERE id=?1",
            params![tracked_id, pct],
        )?;
    }
    Ok(())
}

fn row_to_skill(r: &rusqlite::Row) -> rusqlite::Result<Skill> {
    Ok(Skill {
        id: r.get(0)?,
        name: r.get(1)?,
        goal: r.get(2)?,
        level: r.get(3)?,
        hours: r.get(4)?,
        notes: r.get(5)?,
        created_at: r.get(6)?,
        updated_at: r.get(7)?,
    })
}

pub fn list_skills(conn: &Connection) -> Result<Vec<Skill>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, goal, level, hours, notes, created_at, updated_at FROM skills ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([], row_to_skill)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn add_skill(conn: &Connection, now: i64, name: &str, goal: &str) -> Result<i64> {
    conn.execute(
        "INSERT INTO skills (name, goal, level, hours, notes, created_at, updated_at) VALUES (?1,?2,0,0,'',?3,?3)",
        params![name, goal, now],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_skill(
    conn: &Connection,
    now: i64,
    id: i64,
    name: &str,
    goal: &str,
    level: i64,
    notes: &str,
) -> Result<()> {
    conn.execute(
        "UPDATE skills SET name=?2, goal=?3, level=?4, notes=?5, updated_at=?6 WHERE id=?1",
        params![id, name, goal, level.clamp(0, 100), notes, now],
    )?;
    Ok(())
}

pub fn log_study(conn: &Connection, now: i64, id: i64, hours: f64) -> Result<()> {
    conn.execute(
        "UPDATE skills SET hours = hours + ?2, updated_at=?3 WHERE id=?1",
        params![id, hours.max(0.0), now],
    )?;
    Ok(())
}

pub fn remove_skill(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM skills WHERE id=?1", params![id])?;
    Ok(())
}

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

pub fn list_stories(conn: &Connection) -> Result<Vec<Story>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, content, tags, created_at, updated_at FROM stories ORDER BY updated_at DESC",
    )?;
    let rows = stmt.query_map([], row_to_story)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn add_story(
    conn: &Connection,
    now: i64,
    title: &str,
    content: &str,
    tags: &str,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO stories (title, content, tags, created_at, updated_at) VALUES (?1,?2,?3,?4,?4)",
        params![title, content, tags, now],
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
