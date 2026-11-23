//! One-time copy of the data left behind by the old app identifier.
//!
//! The app used to be `com.w1ck3ds0d4.jobradar` with a `jobradar.db` database. It is now
//! `com.wickitlab.lodestar` with `lodestar.db`. On the first start of the new version the old
//! data folders are COPIED into the new ones, before the database is opened, so nothing is lost.
//!
//! Safety rules, all covered by the tests below:
//! - the old folder is only ever read: nothing in it is deleted, moved or modified;
//! - the copy goes into a sibling `<folder>.migrating` folder and is moved into place only
//!   when complete, with the database files last, so a crash or power loss never leaves a
//!   half-written `lodestar.db` behind. A stale `.migrating` folder is deleted on the next
//!   start and the copy is made again;
//! - nothing is copied when the new database already exists (or the new local folder is in use);
//! - the old database files are compared before and after the copy; if they changed in the
//!   meantime (the old app is still running) the copy is repeated, and given up after a few
//!   tries rather than keeping a possibly inconsistent database;
//! - if a copy fails, the staging folder is removed and the app starts with an empty new
//!   folder, never a crash.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// The identifier the app shipped with before the rename.
pub const OLD_IDENTIFIER: &str = "com.w1ck3ds0d4.jobradar";
/// Database file name before and after the rename.
const OLD_DB: &str = "jobradar.db";
const NEW_DB: &str = "lodestar.db";
/// The files that make up the database: the main file and its SQLite side files.
const DB_SUFFIXES: [&str; 4] = ["", "-wal", "-shm", "-journal"];
/// How often the copy is repeated when the old database changes while it is being copied.
const COPY_TRIES: usize = 3;

/// What a migration run did.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing to do (already migrated, or no old data).
    NotNeeded,
    /// The old folder was copied; this many files were written.
    Copied(usize),
    /// A copy failed; the partial result was removed.
    Failed,
}

/// Rename exactly `jobradar.db`, `-wal`, `-shm` and `-journal` (top level only). Any other
/// name, including `jobradar.db.bak`, is kept as it is.
fn renamed(name: &str, rename_db: bool) -> String {
    if rename_db {
        for suffix in DB_SUFFIXES {
            if name == format!("{OLD_DB}{suffix}") {
                return format!("{NEW_DB}{suffix}");
            }
        }
    }
    name.to_string()
}

/// Whether a (new) top-level name is one of the database files.
fn is_db_file(name: &str) -> bool {
    DB_SUFFIXES
        .iter()
        .any(|suffix| name == format!("{NEW_DB}{suffix}"))
}

/// The sibling folder the copy is staged in.
fn staging_dir(new: &Path) -> PathBuf {
    let mut name = new.file_name().unwrap_or_default().to_os_string();
    name.push(".migrating");
    new.with_file_name(name)
}

/// Delete a staging folder left behind by a run that was killed half way.
fn clear_stale(new: &Path) {
    let staging = staging_dir(new);
    if staging.exists() {
        tracing::info!("removing unfinished migration folder {}", staging.display());
        let _ = fs::remove_dir_all(&staging);
    }
}

/// Copy the old roaming app-data folder into the new one, renaming the database files.
/// Runs only when `new` has no `lodestar.db` and `old` exists.
pub fn migrate_data_dir(old: &Path, new: &Path) -> Outcome {
    clear_stale(new);
    if new.join(NEW_DB).exists() {
        return Outcome::NotNeeded;
    }
    run(old, new, true, &|from, to| fs::copy(from, to).map(|_| ()))
}

/// Copy the old local app-data folder (web view storage and the like) into the new one.
/// Runs only when `new` does not exist or is empty and `old` exists.
pub fn migrate_local_dir(old: &Path, new: &Path) -> Outcome {
    clear_stale(new);
    let in_use = fs::read_dir(new)
        .map(|mut it| it.next().is_some())
        .unwrap_or(false);
    if in_use {
        return Outcome::NotNeeded;
    }
    run(old, new, false, &|from, to| fs::copy(from, to).map(|_| ()))
}

type Copier<'a> = &'a dyn Fn(&Path, &Path) -> io::Result<()>;

fn run(old: &Path, new: &Path, rename_db: bool, copier: Copier) -> Outcome {
    if old == new || !old.is_dir() {
        return Outcome::NotNeeded;
    }
    let staging = staging_dir(new);
    let mut made = Made::default();
    match stage_and_publish(old, new, &staging, rename_db, copier, &mut made) {
        Ok(()) => {
            tracing::info!(
                "migrated {} files from {} to {}",
                made.files,
                old.display(),
                new.display()
            );
            Outcome::Copied(made.files)
        }
        Err(e) => {
            tracing::warn!(
                "could not copy old data from {} ({e}); starting with an empty folder",
                old.display()
            );
            made.undo(&staging);
            Outcome::Failed
        }
    }
}

/// Copy into the staging folder (repeating if the old database changes meanwhile), then
/// move the result into place.
fn stage_and_publish(
    old: &Path,
    new: &Path,
    staging: &Path,
    rename_db: bool,
    copier: Copier,
    made: &mut Made,
) -> io::Result<()> {
    let mut stable = false;
    for _ in 0..COPY_TRIES {
        let _ = fs::remove_dir_all(staging);
        made.files = 0;
        let before = fingerprint(old);
        make_dir(staging, made)?;
        copy_tree(old, staging, rename_db, copier, made)?;
        if before == fingerprint(old) {
            stable = true;
            break;
        }
    }
    if !stable {
        return Err(io::Error::other(
            "the old database kept changing while it was copied (is the old app still running?)",
        ));
    }
    publish(staging, new, rename_db)
}

/// Name, size and modification time of the old database files, to notice a writer.
fn fingerprint(old: &Path) -> Vec<(String, u64, Option<SystemTime>)> {
    let mut out = Vec::new();
    for suffix in DB_SUFFIXES {
        let name = format!("{OLD_DB}{suffix}");
        if let Ok(meta) = fs::metadata(old.join(&name)) {
            out.push((name, meta.len(), meta.modified().ok()));
        }
    }
    out
}

/// Move the finished staging folder to `new`. A missing `new` is replaced by one rename, so
/// the whole folder appears at once. An existing `new` (other files already in it) gets the
/// staged entries moved in, never overwriting a file already there, with the database files
/// last: until `lodestar.db` is in place the next start tries again.
fn publish(staging: &Path, new: &Path, rename_db: bool) -> io::Result<()> {
    if !new.exists() {
        return fs::rename(staging, new);
    }
    let mut entries: Vec<(String, PathBuf)> = Vec::new();
    for entry in fs::read_dir(staging)? {
        let entry = entry?;
        entries.push((
            entry.file_name().to_string_lossy().to_string(),
            entry.path(),
        ));
    }
    // Database files last, the main file after its side files.
    let rank = |name: &str| -> usize {
        if !rename_db {
            return 0;
        }
        match DB_SUFFIXES
            .iter()
            .position(|s| name == format!("{NEW_DB}{s}"))
        {
            Some(0) => 2,
            Some(_) => 1,
            None => 0,
        }
    };
    entries.sort_by_key(|(name, _)| rank(name));
    for (name, from) in entries {
        let to = new.join(&name);
        let replace = rename_db && is_db_file(&name);
        if to.exists() && !replace {
            continue; // never overwrite anything already in the new folder
        }
        fs::rename(&from, &to)?;
    }
    let _ = fs::remove_dir_all(staging);
    Ok(())
}

/// What this run created, so a failure can remove exactly that and nothing else.
#[derive(Default)]
struct Made {
    files: usize,
    dirs: Vec<PathBuf>,
}

impl Made {
    fn undo(&self, staging: &Path) {
        let _ = fs::remove_dir_all(staging);
        // Children were pushed after their parents, so remove in reverse. Only folders
        // this run created, and only while empty.
        for d in self.dirs.iter().rev() {
            let _ = fs::remove_dir(d);
        }
    }
}

/// Create `dir` and any missing parents, recording each one that did not exist.
fn make_dir(dir: &Path, made: &mut Made) -> io::Result<()> {
    if dir.is_dir() {
        return Ok(());
    }
    if let Some(parent) = dir.parent() {
        make_dir(parent, made)?;
    }
    fs::create_dir(dir)?;
    made.dirs.push(dir.to_path_buf());
    Ok(())
}

fn copy_tree(
    src: &Path,
    dst: &Path,
    rename_db: bool,
    copier: Copier,
    made: &mut Made,
) -> io::Result<()> {
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let name = entry.file_name().to_string_lossy().to_string();
        let from = entry.path();
        if kind.is_dir() {
            let to = dst.join(&name);
            make_dir(&to, made)?;
            // The database rename applies to the top level only.
            copy_tree(&from, &to, false, copier, made)?;
        } else if kind.is_file() {
            let to = dst.join(renamed(&name, rename_db));
            copier(&from, &to)?;
            made.files += 1;
        }
        // Symlinks and other special entries are skipped.
    }
    Ok(())
}

/// The four folders involved, for an app identifier: old roaming, new roaming, old local,
/// new local. `None` when the platform folders cannot be found.
pub fn folders(new_identifier: &str) -> Option<(PathBuf, PathBuf, PathBuf, PathBuf)> {
    let (roaming, local) = base_dirs()?;
    Some((
        roaming.join(OLD_IDENTIFIER),
        roaming.join(new_identifier),
        local.join(OLD_IDENTIFIER),
        local.join(new_identifier),
    ))
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
}

#[cfg(windows)]
fn base_dirs() -> Option<(PathBuf, PathBuf)> {
    Some((env_path("APPDATA")?, env_path("LOCALAPPDATA")?))
}

#[cfg(target_os = "macos")]
fn base_dirs() -> Option<(PathBuf, PathBuf)> {
    let d = env_path("HOME")?.join("Library/Application Support");
    Some((d.clone(), d))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn base_dirs() -> Option<(PathBuf, PathBuf)> {
    let d =
        env_path("XDG_DATA_HOME").or_else(|| env_path("HOME").map(|h| h.join(".local/share")))?;
    Some((d.clone(), d))
}

/// Run both migrations. Call once at startup, before the database is opened and before any
/// window (and so the web view's own folder) is created.
pub fn run_startup(new_identifier: &str) {
    let Some((old_roaming, new_roaming, old_local, new_local)) = folders(new_identifier) else {
        return;
    };
    migrate_data_dir(&old_roaming, &new_roaming);
    if new_local != new_roaming {
        migrate_local_dir(&old_local, &new_local);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// A unique temporary folder that is removed when dropped.
    struct Tmp(PathBuf);

    impl Tmp {
        fn new() -> Self {
            let n = COUNTER.fetch_add(1, Ordering::SeqCst);
            let p =
                std::env::temp_dir().join(format!("lodestar-migrate-{}-{n}", std::process::id()));
            let _ = fs::remove_dir_all(&p);
            fs::create_dir_all(&p).unwrap();
            Tmp(p)
        }
        fn path(&self, rel: &str) -> PathBuf {
            self.0.join(rel)
        }
    }

    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn write(p: &Path, text: &str) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, text).unwrap();
    }

    fn make_old(tmp: &Tmp) -> PathBuf {
        let old = tmp.path("old");
        write(&old.join("jobradar.db"), "db");
        write(&old.join("jobradar.db-wal"), "wal");
        write(&old.join("jobradar.db-shm"), "shm");
        write(&old.join("models/whisper/model.bin"), "weights");
        write(&old.join("notes.txt"), "keep");
        old
    }

    /// Every file under `dir` with its contents, sorted, for before/after comparison.
    fn snapshot(dir: &Path) -> Vec<(String, String)> {
        fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, String)>) {
            for e in fs::read_dir(dir).unwrap() {
                let e = e.unwrap();
                let p = e.path();
                let rel = p.strip_prefix(base).unwrap().to_string_lossy().to_string();
                if p.is_dir() {
                    out.push((format!("{rel}/"), String::new()));
                    walk(&p, base, out);
                } else {
                    out.push((rel, fs::read_to_string(&p).unwrap()));
                }
            }
        }
        let mut out = Vec::new();
        walk(dir, dir, &mut out);
        out.sort();
        out
    }

    #[test]
    fn copies_and_renames_when_new_is_empty_and_old_exists() {
        let tmp = Tmp::new();
        let old = make_old(&tmp);
        let new = tmp.path("new");
        assert_eq!(migrate_data_dir(&old, &new), Outcome::Copied(5));
        assert_eq!(fs::read_to_string(new.join("lodestar.db")).unwrap(), "db");
        assert_eq!(
            fs::read_to_string(new.join("lodestar.db-wal")).unwrap(),
            "wal"
        );
        assert_eq!(
            fs::read_to_string(new.join("lodestar.db-shm")).unwrap(),
            "shm"
        );
        assert!(!new.join("jobradar.db").exists());
        assert_eq!(
            fs::read_to_string(new.join("models/whisper/model.bin")).unwrap(),
            "weights"
        );
        assert_eq!(fs::read_to_string(new.join("notes.txt")).unwrap(), "keep");
    }

    #[test]
    fn does_nothing_when_the_new_database_exists() {
        let tmp = Tmp::new();
        let old = make_old(&tmp);
        let new = tmp.path("new");
        write(&new.join("lodestar.db"), "mine");
        assert_eq!(migrate_data_dir(&old, &new), Outcome::NotNeeded);
        assert_eq!(snapshot(&new), vec![("lodestar.db".into(), "mine".into())]);
    }

    #[test]
    fn does_nothing_when_there_is_no_old_folder() {
        let tmp = Tmp::new();
        let new = tmp.path("new");
        assert_eq!(
            migrate_data_dir(&tmp.path("missing"), &new),
            Outcome::NotNeeded
        );
        assert!(!new.exists());
    }

    #[test]
    fn never_touches_the_old_folder() {
        let tmp = Tmp::new();
        let old = make_old(&tmp);
        let before = snapshot(&old);
        migrate_data_dir(&old, &tmp.path("new"));
        assert_eq!(snapshot(&old), before);
        // A failed run leaves it alone too.
        let fail_new = tmp.path("new2");
        let fail = |_: &Path, _: &Path| -> io::Result<()> { Err(io::Error::other("boom")) };
        assert_eq!(run(&old, &fail_new, true, &fail), Outcome::Failed);
        assert_eq!(snapshot(&old), before);
    }

    #[test]
    fn failed_copy_removes_what_it_created_and_nothing_else() {
        let tmp = Tmp::new();
        let old = make_old(&tmp);
        let new = tmp.path("new");
        write(&new.join("existing.txt"), "already here");
        // Fail on the third file copied.
        let count = AtomicU32::new(0);
        let flaky = |from: &Path, to: &Path| -> io::Result<()> {
            if count.fetch_add(1, Ordering::SeqCst) == 2 {
                // Leave a half-written file behind, like a real failure would.
                fs::write(to, "partial")?;
                return Err(io::Error::other("disk full"));
            }
            fs::copy(from, to).map(|_| ())
        };
        assert_eq!(run(&old, &new, true, &flaky), Outcome::Failed);
        assert_eq!(
            snapshot(&new),
            vec![("existing.txt".into(), "already here".into())]
        );
    }

    #[test]
    fn failed_copy_into_a_new_folder_leaves_no_folder_behind() {
        let tmp = Tmp::new();
        let old = make_old(&tmp);
        let new = tmp.path("deep/new");
        let fail = |_: &Path, _: &Path| -> io::Result<()> { Err(io::Error::other("boom")) };
        assert_eq!(run(&old, &new, true, &fail), Outcome::Failed);
        assert!(!new.exists());
        assert!(!tmp.path("deep").exists());
    }

    #[test]
    fn does_not_overwrite_files_already_in_the_new_folder() {
        let tmp = Tmp::new();
        let old = make_old(&tmp);
        let new = tmp.path("new");
        write(&new.join("notes.txt"), "newer");
        migrate_data_dir(&old, &new);
        assert_eq!(fs::read_to_string(new.join("notes.txt")).unwrap(), "newer");
    }

    #[test]
    fn local_dir_copies_as_is_only_when_new_is_empty() {
        let tmp = Tmp::new();
        let old = tmp.path("old-local");
        write(&old.join("EBWebView/Default/data"), "web");
        write(&old.join("jobradar.db"), "not renamed here");
        let new = tmp.path("new-local");
        assert_eq!(migrate_local_dir(&old, &new), Outcome::Copied(2));
        assert_eq!(
            fs::read_to_string(new.join("EBWebView/Default/data")).unwrap(),
            "web"
        );
        assert!(new.join("jobradar.db").exists());
        // Now in use: a second run does nothing.
        write(&new.join("later.txt"), "x");
        let before = snapshot(&new);
        assert_eq!(migrate_local_dir(&old, &new), Outcome::NotNeeded);
        assert_eq!(snapshot(&new), before);
    }

    #[test]
    fn only_the_exact_database_names_are_renamed() {
        let tmp = Tmp::new();
        let old = make_old(&tmp);
        write(&old.join("jobradar.db.bak"), "backup");
        write(&old.join("jobradar.db-journal"), "journal");
        let new = tmp.path("new");
        migrate_data_dir(&old, &new);
        assert_eq!(
            fs::read_to_string(new.join("jobradar.db.bak")).unwrap(),
            "backup"
        );
        assert!(!new.join("lodestar.db.bak").exists());
        assert_eq!(
            fs::read_to_string(new.join("lodestar.db-journal")).unwrap(),
            "journal"
        );
    }

    #[test]
    fn the_database_never_appears_before_the_copy_is_complete() {
        let tmp = Tmp::new();
        let old = make_old(&tmp);
        let new = tmp.path("new");
        write(&new.join("existing.txt"), "already here");
        // Fail on the very last file, whichever order the folder is read in.
        let count = AtomicU32::new(0);
        let last_fails = |from: &Path, to: &Path| -> io::Result<()> {
            if count.fetch_add(1, Ordering::SeqCst) == 4 {
                return Err(io::Error::other("power cut"));
            }
            fs::copy(from, to).map(|_| ())
        };
        assert_eq!(run(&old, &new, true, &last_fails), Outcome::Failed);
        assert!(!new.join("lodestar.db").exists());
        assert!(!staging_dir(&new).exists());
        // The next start simply tries again and succeeds.
        assert_eq!(migrate_data_dir(&old, &new), Outcome::Copied(5));
        assert_eq!(fs::read_to_string(new.join("lodestar.db")).unwrap(), "db");
        assert_eq!(
            fs::read_to_string(new.join("existing.txt")).unwrap(),
            "already here"
        );
    }

    #[test]
    fn a_stale_staging_folder_from_a_killed_run_is_deleted_and_the_copy_is_redone() {
        let tmp = Tmp::new();
        let old = make_old(&tmp);
        let new = tmp.path("new");
        // A previous run died after writing part of its staging folder.
        write(&staging_dir(&new).join("lodestar.db"), "truncated");
        write(&staging_dir(&new).join("junk.bin"), "junk");
        assert_eq!(migrate_data_dir(&old, &new), Outcome::Copied(5));
        assert_eq!(fs::read_to_string(new.join("lodestar.db")).unwrap(), "db");
        assert!(!new.join("junk.bin").exists());
        assert!(!staging_dir(&new).exists());
    }

    #[test]
    fn a_stale_staging_folder_is_removed_even_when_nothing_is_to_be_copied() {
        let tmp = Tmp::new();
        let old = make_old(&tmp);
        let new = tmp.path("new");
        write(&new.join("lodestar.db"), "mine");
        write(&staging_dir(&new).join("lodestar.db"), "truncated");
        assert_eq!(migrate_data_dir(&old, &new), Outcome::NotNeeded);
        assert!(!staging_dir(&new).exists());
        assert_eq!(snapshot(&new), vec![("lodestar.db".into(), "mine".into())]);
    }

    #[test]
    fn orphaned_side_files_without_a_database_are_replaced_not_kept() {
        let tmp = Tmp::new();
        let old = make_old(&tmp);
        let new = tmp.path("new");
        // A run that was killed while moving files into place left a stray -wal behind.
        write(&new.join("lodestar.db-wal"), "stale wal");
        assert_eq!(migrate_data_dir(&old, &new), Outcome::Copied(5));
        assert_eq!(
            fs::read_to_string(new.join("lodestar.db-wal")).unwrap(),
            "wal"
        );
        assert_eq!(fs::read_to_string(new.join("lodestar.db")).unwrap(), "db");
    }

    #[test]
    fn a_database_that_changes_while_it_is_copied_is_not_kept() {
        let tmp = Tmp::new();
        let old = make_old(&tmp);
        let new = tmp.path("new");
        let old_db = old.join("jobradar.db");
        let n = AtomicU32::new(0);
        // The old app keeps writing: every copied file, the main db grows.
        let busy = |from: &Path, to: &Path| -> io::Result<()> {
            let k = n.fetch_add(1, Ordering::SeqCst);
            fs::write(&old_db, "x".repeat(10 + k as usize))?;
            fs::copy(from, to).map(|_| ())
        };
        assert_eq!(run(&old, &new, true, &busy), Outcome::Failed);
        assert!(!new.exists());
        assert!(!staging_dir(&new).exists());
    }

    #[test]
    fn folders_use_the_old_and_new_identifiers() {
        if let Some((or, nr, ol, nl)) = folders("com.wickitlab.lodestar") {
            assert!(or.ends_with(OLD_IDENTIFIER) && ol.ends_with(OLD_IDENTIFIER));
            assert!(
                nr.ends_with("com.wickitlab.lodestar") && nl.ends_with("com.wickitlab.lodestar")
            );
        }
    }
}
