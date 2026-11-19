//! Command implementations. Every command is read-only: it reads Windows, a folder, snapshot
//! files or the desktop profile (opened read-only), and only `snapshot --out` writes a new file.

use crate::args::{Command, Invocation};
use exor_engine::snapshot::{self, CaptureOptions};
use exor_engine::trie::{self, ChangeType, FileState, PathTrie};
use exor_engine::{Cancel, leftovers, programs};
use exor_storage::Store;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// A failure with a stable kind and exit code.
#[derive(Debug)]
pub struct CliError {
    /// Stable machine-readable kind.
    pub kind: &'static str,
    /// Plain-language message.
    pub message: String,
}

impl CliError {
    pub fn usage(m: impl Into<String>) -> Self {
        Self {
            kind: "usage",
            message: m.into(),
        }
    }
    pub fn refused(m: impl Into<String>) -> Self {
        Self {
            kind: "refused",
            message: m.into(),
        }
    }
    pub fn not_found(m: impl Into<String>) -> Self {
        Self {
            kind: "not_found",
            message: m.into(),
        }
    }
    pub fn failed(m: impl std::fmt::Display) -> Self {
        Self {
            kind: "failed",
            message: m.to_string(),
        }
    }

    /// Process exit code for this kind.
    pub fn exit_code(&self) -> u8 {
        match self.kind {
            "usage" => 2,
            "refused" => 3,
            "not_found" => 4,
            _ => 1,
        }
    }
}

type Outcome = Result<Value, CliError>;

/// Format marker and version of snapshot files written by `snapshot --out`.
const SNAPSHOT_FORMAT: &str = "exorsystem-snapshot";
const SNAPSHOT_VERSION: u32 = 1;

/// A snapshot file. Paths are as observed; content hashes are BLAKE3 when computed.
#[derive(Debug, Serialize, Deserialize)]
pub struct SnapshotFile {
    pub format: String,
    pub version: u32,
    pub root: String,
    pub captured_unix_ms: u64,
    pub strict: bool,
    pub root_digest: String,
    pub entries: Vec<FileState>,
    pub skipped: Vec<Skipped>,
}

/// A path the capture could not read or hash, with the reason.
#[derive(Debug, Serialize, Deserialize)]
pub struct Skipped {
    pub path: String,
    pub reason: String,
}

/// Run one parsed invocation.
pub fn run(inv: &Invocation) -> Outcome {
    #[cfg(windows)]
    if inv.command.needs_normal_user()
        && exor_win::process_is_elevated().map_err(CliError::failed)?
    {
        return Err(CliError::refused(
            "ExorSystem does not run with administrator rights. Run exor from a normal (not elevated) terminal.",
        ));
    }
    match &inv.command {
        Command::Help => Ok(Value::String(crate::args::USAGE.into())),
        Command::Version => Ok(json!({
            "version": env!("CARGO_PKG_VERSION"),
            "profile_schema": exor_storage::latest_version(),
            "snapshot_format": { "format": SNAPSHOT_FORMAT, "version": SNAPSHOT_VERSION },
        })),
        Command::Programs { all } => list_programs(*all),
        Command::Leftovers { id } => find_leftovers(id),
        Command::Sessions => {
            let store = open_profile(inv)?;
            let sessions = store.sessions().map_err(CliError::failed)?;
            Ok(json!({ "count": sessions.len(), "sessions": sessions }))
        }
        Command::History {
            session,
            path,
            limit,
            before,
        } => {
            let store = open_profile(inv)?;
            let page = store
                .ledger_page(*limit, path.as_deref(), *session, *before)
                .map_err(CliError::failed)?;
            Ok(json!({
                "count": page.entries.len(),
                "entries": page.entries,
                "next_before_id": page.next_before_id,
            }))
        }
        Command::Snapshot { root, out, strict } => take_snapshot(root, out.as_deref(), *strict),
        Command::Diff { before, after } => diff_files(before, after),
        Command::Env { script } => check_environment(script.as_deref()),
    }
}

fn installed() -> Result<Vec<programs::InstalledProgram>, CliError> {
    #[cfg(windows)]
    {
        exor_win::enumerate_programs().map_err(CliError::failed)
    }
    #[cfg(not(windows))]
    {
        Err(CliError::refused(
            "The installed-program list comes from the Windows registry; run this on Windows.",
        ))
    }
}

fn list_programs(all: bool) -> Outcome {
    let list = programs::visible(installed()?, all);
    Ok(json!({ "count": list.len(), "programs": list }))
}

fn find_leftovers(id: &str) -> Outcome {
    let program = installed()?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| {
            CliError::not_found(format!(
                "No installed program has the id '{id}'. Use the ids from 'exor programs'."
            ))
        })?;
    let folders = leftovers::find_leftovers(&program, &leftovers::default_roots());
    Ok(json!({
        "program": { "id": program.id, "name": program.name, "publisher": program.publisher },
        "folders": folders,
        "note": "Suggestions only. Nothing was moved; review and quarantine in the desktop app.",
    }))
}

/// The desktop app's profile, opened read-only. Never created, migrated or locked.
fn open_profile(inv: &Invocation) -> Result<Store, CliError> {
    let dir = match &inv.profile {
        Some(p) => p.clone(),
        None => default_profile()?,
    };
    let db = dir.join("exor.db");
    if !db.is_file() {
        return Err(CliError::not_found(format!(
            "No ExorSystem profile at {}. Open the desktop app once, or pass --profile.",
            dir.display()
        )));
    }
    let store = Store::open_reader(&db).map_err(CliError::failed)?;
    let found = store.schema_version().map_err(CliError::failed)?;
    let known = exor_storage::latest_version();
    if found != known {
        return Err(CliError::refused(if found > known {
            format!(
                "This profile (schema {found}) was written by a newer ExorSystem; this exor understands schema {known}."
            )
        } else {
            format!(
                "This profile uses schema {found}. Open the desktop app once to upgrade it to schema {known}."
            )
        }));
    }
    Ok(store)
}

fn default_profile() -> Result<PathBuf, CliError> {
    #[cfg(windows)]
    {
        exor_win::user_data_dir().map_err(CliError::failed)
    }
    #[cfg(not(windows))]
    {
        Err(CliError::usage(
            "There is no default ExorSystem profile on this platform; pass --profile <folder>.",
        ))
    }
}

/// `YYYY-MM-DD HH:MM UTC` for a Unix time in seconds (proleptic Gregorian calendar).
/// Only the Windows-only `env` command uses it.
#[cfg_attr(not(windows), allow(dead_code))]
fn utc_text(secs: u64) -> String {
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rem = secs % 86_400;
    // Howard Hinnant's civil-from-days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02} UTC",
        rem / 3_600,
        rem % 3_600 / 60
    )
}

/// PATH and toolchain-variable hygiene. Read-only; `script` names a new file for a reviewable
/// PowerShell fix script that ExorSystem never runs.
fn check_environment(script: Option<&Path>) -> Outcome {
    #[cfg(windows)]
    {
        use exor_engine::envcheck::{self, PathKind};
        let vars =
            exor_win::stored_variables(&envcheck::watched_names()).map_err(CliError::failed)?;
        let probe = |p: &str| match std::fs::metadata(p) {
            Ok(m) if m.is_dir() => PathKind::Dir,
            Ok(_) => PathKind::File,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => PathKind::Missing,
            Err(_) => PathKind::Unknown,
        };
        let report = envcheck::check(&vars, &exor_win::expand_environment, &probe);
        let written = match script {
            Some(path) => {
                let text = envcheck::fix_script(&vars, &report, &utc_text(now_ms() / 1_000));
                // UTF-8 with a byte-order mark, so Windows PowerShell 5.1 reads non-ASCII paths correctly.
                let mut bytes = vec![0xEF, 0xBB, 0xBF];
                bytes.extend_from_slice(text.replace('\n', "\r\n").as_bytes());
                write_new(path, &bytes)?;
                Some(path.to_string_lossy().into_owned())
            }
            None => None,
        };
        Ok(json!({
            "variables": report.variables,
            "findings": report.findings,
            "removable": report.removable,
            "to_review": report.to_review,
            "script": written,
            "note": "Nothing was changed. A fix script, if written, only runs when you run it.",
        }))
    }
    #[cfg(not(windows))]
    {
        let _ = script;
        Err(CliError::refused(
            "Stored environment variables are read from the Windows registry; run this on Windows.",
        ))
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

fn take_snapshot(root: &Path, out: Option<&Path>, strict: bool) -> Outcome {
    if !root.is_dir() {
        return Err(CliError::not_found(format!(
            "{} is not a folder",
            root.display()
        )));
    }
    let opts = CaptureOptions {
        strict,
        ..CaptureOptions::default()
    };
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(CliError::failed)?;
    let snap = runtime
        .block_on(snapshot::capture(root, &opts, None, &Cancel::new()))
        .map_err(CliError::failed)?;
    let file = SnapshotFile {
        format: SNAPSHOT_FORMAT.into(),
        version: SNAPSHOT_VERSION,
        root: root.to_string_lossy().into_owned(),
        captured_unix_ms: now_ms(),
        strict,
        root_digest: snap.trie.root_digest(),
        entries: snap.trie.states().into_iter().cloned().collect(),
        skipped: snap
            .skipped
            .iter()
            .map(|(path, reason)| Skipped {
                path: path.clone(),
                reason: reason.clone(),
            })
            .collect(),
    };
    let written = match out {
        Some(path) => {
            write_new(path, &serde_json::to_vec(&file).map_err(CliError::failed)?)?;
            Some(path.to_string_lossy().into_owned())
        }
        None => None,
    };
    Ok(json!({
        "root": file.root,
        "files": snap.files,
        "entries": file.entries.len(),
        "skipped": file.skipped.len(),
        "complete": file.skipped.is_empty(),
        "root_digest": file.root_digest,
        "strict": strict,
        "written": written,
    }))
}

/// Write `value` to a new file; an existing file is never replaced.
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), CliError> {
    use std::io::Write;
    let mut f = std::fs::File::create_new(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            CliError::refused(format!(
                "{} already exists; choose a new file name.",
                path.display()
            ))
        } else {
            CliError::failed(format!("could not create {}: {e}", path.display()))
        }
    })?;
    f.write_all(bytes)
        .and_then(|()| f.sync_all())
        .map_err(|e| CliError::failed(format!("could not write {}: {e}", path.display())))
}

fn read_snapshot(path: &Path) -> Result<SnapshotFile, CliError> {
    let bytes = std::fs::read(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            CliError::not_found(format!("{} does not exist", path.display()))
        } else {
            CliError::failed(format!("could not read {}: {e}", path.display()))
        }
    })?;
    let file: SnapshotFile = serde_json::from_slice(&bytes).map_err(|e| {
        CliError::failed(format!(
            "{} is not an ExorSystem snapshot: {e}",
            path.display()
        ))
    })?;
    if file.format != SNAPSHOT_FORMAT || file.version != SNAPSHOT_VERSION {
        return Err(CliError::failed(format!(
            "{} is format '{}' version {}; this exor reads '{SNAPSHOT_FORMAT}' version {SNAPSHOT_VERSION}.",
            path.display(),
            file.format,
            file.version
        )));
    }
    Ok(file)
}

fn trie_of(file: &SnapshotFile) -> PathTrie {
    let mut t = if file.strict {
        PathTrie::new_strict()
    } else {
        PathTrie::new()
    };
    for e in &file.entries {
        t.insert(e.clone());
    }
    t.finalize();
    t
}

fn diff_files(before: &Path, after: &Path) -> Outcome {
    let (old, new) = (read_snapshot(before)?, read_snapshot(after)?);
    let changes = trie::diff(&trie_of(&old), &trie_of(&new));
    // A path the later capture could not read is not evidence that it was removed.
    let unreadable_after: BTreeSet<String> =
        new.skipped.iter().map(|s| s.path.to_lowercase()).collect();
    let (unreadable, changes): (Vec<_>, Vec<_>) = changes.into_iter().partition(|c| {
        c.change == ChangeType::Removed && unreadable_after.contains(&c.path.to_lowercase())
    });
    let count = |t| changes.iter().filter(|c| c.change == t).count();
    Ok(json!({
        "before": { "root": old.root, "captured_unix_ms": old.captured_unix_ms, "skipped": old.skipped.len() },
        "after": { "root": new.root, "captured_unix_ms": new.captured_unix_ms, "skipped": new.skipped.len() },
        "same_root": old.root.eq_ignore_ascii_case(&new.root),
        "complete": old.skipped.is_empty() && new.skipped.is_empty(),
        "added": count(ChangeType::Added),
        "removed": count(ChangeType::Removed),
        "modified": count(ChangeType::Modified),
        "changes": changes,
        "unreadable_in_after": unreadable.into_iter().map(|c| c.path).collect::<Vec<_>>(),
    }))
}

#[cfg(test)]
mod tests {
    use super::utc_text;

    #[test]
    fn formats_utc_dates_including_leap_days() {
        assert_eq!(utc_text(0), "1970-01-01 00:00 UTC");
        assert_eq!(utc_text(951_782_400), "2000-02-29 00:00 UTC");
        assert_eq!(utc_text(1_795_057_362), "2026-11-19 03:02 UTC");
        assert_eq!(utc_text(4_102_444_799), "2099-12-31 23:59 UTC");
    }
}
