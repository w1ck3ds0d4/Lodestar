//! SQLite storage: schema, FTS5 index, search, idempotent upserts, stale-job
//! sweeping, source management, and saved-search alerts.

use anyhow::Result;
use rusqlite::{params, params_from_iter, types::Value as SqlValue, Connection};
use serde_json::Value;
use std::path::Path;

use crate::model::{Alert, Job, RawJob, Source};
use crate::util::{
    classify_experience, detect_relocation, infer_workplace, now_ms, remote_region, stable_hash,
};

const JOB_COLS: &str = "j.id, j.source_kind, COALESCE(s.label, j.source_kind) AS source_label, \
    j.external_id, j.title, j.company, j.location, j.remote, j.workplace, j.experience_level, \
    j.employment_type, j.description, j.url, j.salary, j.posted_at, j.relocation, j.remote_region, \
    j.first_seen_at, j.last_seen_at";

/// Open the database, apply the schema, and seed default sources on first run.
pub fn init(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;
        PRAGMA busy_timeout = 5000;

        CREATE TABLE IF NOT EXISTS sources (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            kind TEXT NOT NULL,
            label TEXT NOT NULL,
            config TEXT NOT NULL DEFAULT '{}',
            enabled INTEGER NOT NULL DEFAULT 1,
            min_interval_secs INTEGER NOT NULL DEFAULT 3600,
            last_run_at INTEGER,
            last_status TEXT,
            last_count INTEGER
        );

        CREATE TABLE IF NOT EXISTS jobs (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            source_id INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
            source_kind TEXT NOT NULL,
            external_id TEXT NOT NULL,
            title TEXT NOT NULL,
            company TEXT NOT NULL DEFAULT '',
            location TEXT NOT NULL DEFAULT '',
            remote INTEGER NOT NULL DEFAULT 0,
            workplace TEXT NOT NULL DEFAULT '',
            experience_level TEXT NOT NULL DEFAULT '',
            employment_type TEXT,
            description TEXT NOT NULL DEFAULT '',
            url TEXT NOT NULL DEFAULT '',
            salary TEXT,
            posted_at TEXT,
            relocation INTEGER NOT NULL DEFAULT 0,
            remote_region TEXT NOT NULL DEFAULT '',
            content_hash TEXT NOT NULL DEFAULT '',
            first_seen_at INTEGER NOT NULL,
            last_seen_at INTEGER NOT NULL,
            removed_at INTEGER,
            UNIQUE(source_id, external_id)
        );

        CREATE INDEX IF NOT EXISTS idx_jobs_last_seen ON jobs(last_seen_at);
        CREATE INDEX IF NOT EXISTS idx_jobs_removed ON jobs(removed_at);
        CREATE INDEX IF NOT EXISTS idx_jobs_content_hash ON jobs(content_hash);
        CREATE INDEX IF NOT EXISTS idx_jobs_workplace ON jobs(workplace);
        CREATE INDEX IF NOT EXISTS idx_jobs_level ON jobs(experience_level);

        CREATE VIRTUAL TABLE IF NOT EXISTS jobs_fts USING fts5(
            title, company, location, description,
            content='jobs', content_rowid='id'
        );

        CREATE TRIGGER IF NOT EXISTS jobs_ai AFTER INSERT ON jobs BEGIN
            INSERT INTO jobs_fts(rowid, title, company, location, description)
            VALUES (new.id, new.title, new.company, new.location, new.description);
        END;
        CREATE TRIGGER IF NOT EXISTS jobs_ad AFTER DELETE ON jobs BEGIN
            INSERT INTO jobs_fts(jobs_fts, rowid, title, company, location, description)
            VALUES ('delete', old.id, old.title, old.company, old.location, old.description);
        END;
        CREATE TRIGGER IF NOT EXISTS jobs_au AFTER UPDATE ON jobs BEGIN
            INSERT INTO jobs_fts(jobs_fts, rowid, title, company, location, description)
            VALUES ('delete', old.id, old.title, old.company, old.location, old.description);
            INSERT INTO jobs_fts(rowid, title, company, location, description)
            VALUES (new.id, new.title, new.company, new.location, new.description);
        END;

        CREATE TABLE IF NOT EXISTS alerts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            query TEXT NOT NULL DEFAULT '',
            remote_only INTEGER NOT NULL DEFAULT 0,
            last_notified_at INTEGER
        );

        CREATE TABLE IF NOT EXISTS tracked (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            kind TEXT NOT NULL,
            title TEXT NOT NULL,
            org TEXT NOT NULL DEFAULT '',
            url TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT '',
            notes TEXT NOT NULL DEFAULT '',
            applied_at TEXT,
            deadline TEXT,
            progress INTEGER NOT NULL DEFAULT 0,
            source_job_id INTEGER,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS milestones (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            tracked_id INTEGER NOT NULL REFERENCES tracked(id) ON DELETE CASCADE,
            label TEXT NOT NULL,
            done INTEGER NOT NULL DEFAULT 0,
            position INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS skills (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            goal TEXT NOT NULL DEFAULT '',
            level INTEGER NOT NULL DEFAULT 0,
            hours REAL NOT NULL DEFAULT 0,
            notes TEXT NOT NULL DEFAULT '',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )?;
    // Migrate databases created before the workplace/experience columns existed.
    ensure_column(&conn, "jobs", "workplace", "TEXT NOT NULL DEFAULT ''")?;
    ensure_column(
        &conn,
        "jobs",
        "experience_level",
        "TEXT NOT NULL DEFAULT ''",
    )?;
    ensure_column(&conn, "jobs", "relocation", "INTEGER NOT NULL DEFAULT 0")?;
    ensure_column(&conn, "jobs", "remote_region", "TEXT NOT NULL DEFAULT ''")?;
    seed_defaults(&conn)?;
    Ok(conn)
}

/// Add a column to a table if it isn't already present (lightweight migration).
fn ensure_column(conn: &Connection, table: &str, col: &str, decl: &str) -> Result<()> {
    let present = conn
        .prepare(&format!("PRAGMA table_info({table})"))?
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(|x| x.ok())
        .any(|name| name == col);
    if !present {
        conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {col} {decl}"), [])?;
    }
    Ok(())
}

/// Read a value from the key-value settings table.
pub fn get_setting(conn: &Connection, key: &str) -> Result<Option<String>> {
    use rusqlite::OptionalExtension;
    let v = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |r| r.get::<_, String>(0),
        )
        .optional()?;
    Ok(v)
}

/// Upsert a value into the key-value settings table.
pub fn set_setting(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO settings(key, value) VALUES(?1, ?2) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )?;
    Ok(())
}

/// Remove a key from the settings table (no-op if absent).
pub fn delete_setting(conn: &Connection, key: &str) -> Result<()> {
    conn.execute("DELETE FROM settings WHERE key = ?1", params![key])?;
    Ok(())
}

fn seed_defaults(conn: &Connection) -> Result<()> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM sources", [], |r| r.get(0))?;
    if count > 0 {
        return Ok(());
    }
    // kind, label, config, enabled, min_interval_secs
    let defaults: &[(&str, &str, &str, i64, i64)] = &[
        ("arbeitnow", "Arbeitnow (EU + remote)", "{}", 1, 3600),
        ("remotive", "Remotive (remote)", "{}", 1, 21600),
        ("jobicy", "Jobicy (remote)", "{}", 1, 3600),
        (
            "greenhouse",
            "Stripe (Greenhouse)",
            r#"{"company":"stripe"}"#,
            1,
            21600,
        ),
        (
            "greenhouse",
            "Coinbase (Greenhouse)",
            r#"{"company":"coinbase"}"#,
            1,
            21600,
        ),
        (
            "lever",
            "Example (Lever - set a company)",
            r#"{"company":""}"#,
            0,
            21600,
        ),
        (
            "ashby",
            "Example (Ashby - set a board name)",
            r#"{"name":""}"#,
            0,
            21600,
        ),
        (
            "jsonld",
            "Example careers page (set a URL)",
            r#"{"url":""}"#,
            0,
            21600,
        ),
    ];
    for (kind, label, config, enabled, interval) in defaults {
        conn.execute(
            "INSERT INTO sources (kind, label, config, enabled, min_interval_secs) VALUES (?1,?2,?3,?4,?5)",
            params![kind, label, config, enabled, interval],
        )?;
    }
    Ok(())
}

fn row_to_job(r: &rusqlite::Row) -> rusqlite::Result<Job> {
    Ok(Job {
        id: r.get(0)?,
        source_kind: r.get(1)?,
        source_label: r.get(2)?,
        external_id: r.get(3)?,
        title: r.get(4)?,
        company: r.get(5)?,
        location: r.get(6)?,
        remote: r.get::<_, i64>(7)? != 0,
        workplace: r.get(8)?,
        experience_level: r.get(9)?,
        employment_type: r.get(10)?,
        description: r.get(11)?,
        url: r.get(12)?,
        salary: r.get(13)?,
        posted_at: r.get(14)?,
        relocation: r.get::<_, i64>(15)? != 0,
        remote_region: r.get(16)?,
        first_seen_at: r.get(17)?,
        last_seen_at: r.get(18)?,
    })
}

fn row_to_source(r: &rusqlite::Row) -> rusqlite::Result<Source> {
    let config_str: String = r.get(3)?;
    let config: Value =
        serde_json::from_str(&config_str).unwrap_or_else(|_| Value::Object(Default::default()));
    Ok(Source {
        id: r.get(0)?,
        kind: r.get(1)?,
        label: r.get(2)?,
        config,
        enabled: r.get::<_, i64>(4)? != 0,
        min_interval_secs: r.get(5)?,
        last_run_at: r.get(6)?,
        last_status: r.get(7)?,
        last_count: r.get(8)?,
    })
}

/// Build an FTS5 prefix query from free text. Returns `None` for empty input so
/// callers can fall back to a plain recency listing.
fn build_fts_query(q: &str) -> Option<String> {
    let toks: Vec<String> = q
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| format!("{}*", s.to_lowercase()))
        .collect();
    if toks.is_empty() {
        None
    } else {
        Some(toks.join(" "))
    }
}

/// Append an `AND col IN (?, ...)` clause and its params when `vals` is non-empty.
fn push_in(sql: &mut String, params: &mut Vec<SqlValue>, col: &str, vals: &[String]) {
    if vals.is_empty() {
        return;
    }
    sql.push_str(&format!(" AND {col} IN ("));
    for (i, v) in vals.iter().enumerate() {
        if i > 0 {
            sql.push(',');
        }
        sql.push('?');
        params.push(SqlValue::Text(v.clone()));
    }
    sql.push(')');
}

/// Full-text + filtered search. Empty query => most-recently-seen first.
/// `workplaces` filters on remote/hybrid/onsite; `levels` on entry/mid/senior.
#[allow(clippy::too_many_arguments)]
pub fn search_jobs(
    conn: &Connection,
    query: &str,
    workplaces: &[String],
    levels: &[String],
    kinds: &[String],
    relocation_only: bool,
    remote_eligible: bool,
    limit: i64,
) -> Result<Vec<Job>> {
    let fts = build_fts_query(query);
    let mut params: Vec<SqlValue> = Vec::new();
    let mut sql = format!("SELECT {JOB_COLS} FROM ");
    if let Some(ref q) = fts {
        sql.push_str("jobs_fts f JOIN jobs j ON j.id = f.rowid LEFT JOIN sources s ON s.id = j.source_id WHERE jobs_fts MATCH ? AND j.removed_at IS NULL");
        params.push(SqlValue::Text(q.clone()));
    } else {
        sql.push_str("jobs j LEFT JOIN sources s ON s.id = j.source_id WHERE j.removed_at IS NULL");
    }
    push_in(&mut sql, &mut params, "j.workplace", workplaces);
    push_in(&mut sql, &mut params, "j.experience_level", levels);
    push_in(&mut sql, &mut params, "j.source_kind", kinds);
    if relocation_only {
        sql.push_str(" AND j.relocation = 1");
    }
    if remote_eligible {
        // Remote roles workable from Malta (EU): keep worldwide / EU / unstated,
        // drop ones explicitly restricted to the US, UK, or another country.
        sql.push_str(" AND j.workplace = 'remote' AND j.remote_region NOT IN ('us','uk','other')");
    }
    sql.push_str(if fts.is_some() {
        " ORDER BY bm25(jobs_fts)"
    } else {
        " ORDER BY j.last_seen_at DESC"
    });
    sql.push_str(" LIMIT ?");
    params.push(SqlValue::Integer(limit));

    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params_from_iter(params.iter()), row_to_job)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn list_sources(conn: &Connection) -> Result<Vec<Source>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, label, config, enabled, min_interval_secs, last_run_at, last_status, last_count \
         FROM sources ORDER BY id",
    )?;
    let rows = stmt.query_map([], row_to_source)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

/// Enabled sources whose polite interval has elapsed (or all enabled, if forced).
pub fn due_sources(conn: &Connection, force: bool, now: i64) -> Result<Vec<Source>> {
    Ok(list_sources(conn)?
        .into_iter()
        .filter(|s| s.enabled)
        .filter(|s| {
            force
                || match s.last_run_at {
                    None => true,
                    Some(t) => now - t >= s.min_interval_secs * 1000,
                }
        })
        .collect())
}

pub fn add_source(
    conn: &Connection,
    kind: &str,
    label: &str,
    config: &Value,
    min_interval_secs: i64,
) -> Result<i64> {
    conn.execute(
        "INSERT INTO sources (kind, label, config, enabled, min_interval_secs) VALUES (?1,?2,?3,1,?4)",
        params![kind, label, config.to_string(), min_interval_secs],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn update_source(
    conn: &Connection,
    id: i64,
    label: &str,
    enabled: bool,
    config: &Value,
    min_interval_secs: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE sources SET label=?2, enabled=?3, config=?4, min_interval_secs=?5 WHERE id=?1",
        params![
            id,
            label,
            enabled as i64,
            config.to_string(),
            min_interval_secs
        ],
    )?;
    Ok(())
}

pub fn remove_source(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM sources WHERE id=?1", params![id])?;
    Ok(())
}

pub fn set_source_status(
    conn: &Connection,
    id: i64,
    now: i64,
    status: &str,
    count: i64,
) -> Result<()> {
    conn.execute(
        "UPDATE sources SET last_run_at=?2, last_status=?3, last_count=?4 WHERE id=?1",
        params![id, now, status, count],
    )?;
    Ok(())
}

/// Insert or update postings for a source. `first_seen_at` is preserved across
/// updates; `removed_at` is cleared so a re-seen job is un-deleted.
pub fn upsert_jobs(
    conn: &mut Connection,
    source_id: i64,
    kind: &str,
    jobs: &[RawJob],
    now: i64,
) -> Result<usize> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare(
            "INSERT INTO jobs (source_id, source_kind, external_id, title, company, location, remote, \
                workplace, experience_level, employment_type, description, url, salary, posted_at, \
                relocation, remote_region, content_hash, first_seen_at, last_seen_at, removed_at) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?18,NULL) \
             ON CONFLICT(source_id, external_id) DO UPDATE SET \
                title=excluded.title, company=excluded.company, location=excluded.location, remote=excluded.remote, \
                workplace=excluded.workplace, experience_level=excluded.experience_level, \
                employment_type=excluded.employment_type, description=excluded.description, url=excluded.url, \
                salary=excluded.salary, posted_at=excluded.posted_at, relocation=excluded.relocation, \
                remote_region=excluded.remote_region, content_hash=excluded.content_hash, \
                last_seen_at=excluded.last_seen_at, removed_at=NULL",
        )?;
        for j in jobs {
            let workplace = infer_workplace(j.remote, &j.location, &j.title);
            let experience =
                classify_experience(&j.title, &j.description, j.employment_type.as_deref());
            let remote = (workplace == "remote") as i64;
            let relocation = detect_relocation(&j.description) as i64;
            let region =
                remote_region(workplace == "remote", &j.location, &j.title, &j.description);
            let hash = stable_hash(&format!(
                "{}|{}|{}",
                j.title.to_lowercase(),
                j.company.to_lowercase(),
                j.location.to_lowercase()
            ));
            stmt.execute(params![
                source_id,
                kind,
                j.external_id,
                j.title,
                j.company,
                j.location,
                remote,
                workplace,
                experience,
                j.employment_type,
                j.description,
                j.url,
                j.salary,
                j.posted_at,
                relocation,
                region,
                hash,
                now,
            ])?;
        }
    }
    tx.commit()?;
    Ok(jobs.len())
}

/// Soft-delete this source's jobs that were not seen in the latest crawl.
pub fn mark_stale(conn: &Connection, source_id: i64, run_started: i64, now: i64) -> Result<usize> {
    let n = conn.execute(
        "UPDATE jobs SET removed_at=?3 WHERE source_id=?1 AND removed_at IS NULL AND last_seen_at < ?2",
        params![source_id, run_started, now],
    )?;
    Ok(n)
}

/// (active jobs, distinct companies)
pub fn stats(conn: &Connection) -> Result<(i64, i64)> {
    let total: i64 = conn.query_row(
        "SELECT COUNT(*) FROM jobs WHERE removed_at IS NULL",
        [],
        |r| r.get(0),
    )?;
    let companies: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT company) FROM jobs WHERE removed_at IS NULL AND company <> ''",
        [],
        |r| r.get(0),
    )?;
    Ok((total, companies))
}

pub fn list_alerts(conn: &Connection) -> Result<Vec<Alert>> {
    let mut stmt =
        conn.prepare("SELECT id, query, remote_only, last_notified_at FROM alerts ORDER BY id")?;
    let rows = stmt.query_map([], |r| {
        Ok(Alert {
            id: r.get(0)?,
            query: r.get(1)?,
            remote_only: r.get::<_, i64>(2)? != 0,
            last_notified_at: r.get(3)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn add_alert(conn: &Connection, query: &str, remote_only: bool) -> Result<i64> {
    // Stamp last_notified_at to now so we only notify for jobs seen afterwards.
    conn.execute(
        "INSERT INTO alerts (query, remote_only, last_notified_at) VALUES (?1,?2,?3)",
        params![query, remote_only as i64, now_ms()],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn remove_alert(conn: &Connection, id: i64) -> Result<()> {
    conn.execute("DELETE FROM alerts WHERE id=?1", params![id])?;
    Ok(())
}

pub fn set_alert_notified(conn: &Connection, id: i64, now: i64) -> Result<()> {
    conn.execute(
        "UPDATE alerts SET last_notified_at=?2 WHERE id=?1",
        params![id, now],
    )?;
    Ok(())
}

/// Count jobs matching an alert that were first seen after `since`.
pub fn count_new_matches(conn: &Connection, alert: &Alert, since: i64) -> Result<i64> {
    let fts = build_fts_query(&alert.query);
    let mut params: Vec<SqlValue> = Vec::new();
    let mut sql = String::from("SELECT COUNT(*) FROM ");
    if let Some(ref q) = fts {
        sql.push_str("jobs_fts f JOIN jobs j ON j.id = f.rowid WHERE jobs_fts MATCH ? AND j.removed_at IS NULL AND j.first_seen_at > ?");
        params.push(SqlValue::Text(q.clone()));
        params.push(SqlValue::Integer(since));
    } else {
        sql.push_str("jobs j WHERE j.removed_at IS NULL AND j.first_seen_at > ?");
        params.push(SqlValue::Integer(since));
    }
    if alert.remote_only {
        sql.push_str(" AND j.remote = 1");
    }
    let n: i64 = conn.query_row(&sql, params_from_iter(params.iter()), |r| r.get(0))?;
    Ok(n)
}
