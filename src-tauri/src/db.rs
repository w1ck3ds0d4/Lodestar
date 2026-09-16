//! SQLite storage: schema and the key-value settings table.

use anyhow::Result;
use rusqlite::{params, Connection};
use std::path::Path;

use crate::util::now_ms;

/// Open the database and apply the schema.
pub fn init(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;
        PRAGMA busy_timeout = 5000;

        CREATE TABLE IF NOT EXISTS stories (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            content TEXT NOT NULL DEFAULT '',
            tags TEXT NOT NULL DEFAULT '',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS sessions (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            title TEXT NOT NULL,
            transcript TEXT NOT NULL DEFAULT '',
            answers TEXT NOT NULL DEFAULT '',
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS profiles (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL,
            role TEXT NOT NULL DEFAULT '',
            company TEXT NOT NULL DEFAULT '',
            resume TEXT NOT NULL DEFAULT '',
            notes TEXT NOT NULL DEFAULT '',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );
        "#,
    )?;
    // Multi-profile model: a story belongs to a profile, and the legacy single context
    // (context_* settings + any pre-existing stories) folds into a "Default" profile.
    ensure_column(&conn, "stories", "profile_id", "INTEGER")?;
    // Cached per-session topic verdicts for the running debrief (JSON). Null = not yet
    // analysed, so the debrief can fill gaps incrementally instead of re-analysing.
    ensure_column(&conn, "sessions", "analysis", "TEXT")?;
    migrate_profiles(&conn)?;
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

/// One-time migration to the multi-profile model: when no profile exists yet, create a
/// "Default" profile from the legacy single context (the `context_*` settings), adopt every
/// existing story into it, and mark it active. Idempotent - a no-op once any profile exists.
fn migrate_profiles(conn: &Connection) -> Result<()> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM profiles", [], |r| r.get(0))?;
    if count > 0 {
        return Ok(());
    }
    let now = now_ms();
    let get = |k: &str| get_setting(conn, k).ok().flatten().unwrap_or_default();
    conn.execute(
        "INSERT INTO profiles (name, role, company, resume, notes, created_at, updated_at) \
         VALUES ('Default', ?1, ?2, ?3, ?4, ?5, ?5)",
        params![
            get("context_role"),
            get("context_company"),
            get("context_resume"),
            get("context_notes"),
            now
        ],
    )?;
    let id = conn.last_insert_rowid();
    conn.execute(
        "UPDATE stories SET profile_id=?1 WHERE profile_id IS NULL",
        params![id],
    )?;
    set_setting(conn, "active_profile_id", &id.to_string())?;
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
