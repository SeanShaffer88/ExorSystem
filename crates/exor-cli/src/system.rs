//! Commands that read the live system rather than saved evidence: installed programs, leftover
//! suggestions, folder snapshots and their comparison, and environment-variable hygiene.
//!
//! Same contract as the evidence commands: one JSON object with `schema_version` and `kind` on
//! stdout; usage errors exit 2 and other failures exit 1 with a message on stderr. Nothing here
//! changes the system. The only writes are a new `--out` snapshot or `--script` file, never over
//! an existing file.

use exor_engine::snapshot::{self, CaptureOptions};
use exor_engine::trie::{self, ChangeType, FileState, PathTrie};
use exor_engine::{Cancel, leftovers, programs};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

/// Commands handled here.
pub const COMMANDS: &[&str] = &["programs", "leftovers", "snapshot", "diff", "env"];

/// A failure: usage errors exit 2, everything else 1.
pub enum Failure {
    Usage(String),
    Failed(String),
}

fn failed(e: impl std::fmt::Display) -> Failure {
    Failure::Failed(e.to_string())
}

/// `(value options, flags)` accepted by each command.
fn accepted(command: &str) -> (&'static [&'static str], &'static [&'static str]) {
    match command {
        "programs" => (&[], &["--all"]),
        "leftovers" => (&["--program"], &[]),
        "snapshot" => (&["--root", "--out"], &["--strict"]),
        "diff" => (&["--before", "--after"], &[]),
        "env" => (&["--script"], &[]),
        _ => (&[], &[]),
    }
}

struct Args {
    values: BTreeMap<&'static str, String>,
    flags: BTreeSet<&'static str>,
}

impl Args {
    fn path(&mut self, key: &str) -> Option<PathBuf> {
        self.values.remove(key).map(PathBuf::from)
    }

    fn required(&mut self, key: &str) -> Result<PathBuf, Failure> {
        self.path(key)
            .ok_or_else(|| Failure::Usage(format!("{key} PATH is required")))
    }
}

fn parse(command: &str, mut args: impl Iterator<Item = OsString>) -> Result<Args, Failure> {
    let (value_keys, flag_keys) = accepted(command);
    let mut out = Args {
        values: BTreeMap::new(),
        flags: BTreeSet::new(),
    };
    while let Some(arg) = args.next() {
        if let Some(flag) = flag_keys.iter().find(|k| arg == **k) {
            if !out.flags.insert(flag) {
                return Err(Failure::Usage(format!("{flag} may be supplied only once")));
            }
            continue;
        }
        let key = value_keys
            .iter()
            .find(|k| arg == **k)
            .ok_or_else(|| Failure::Usage(format!("unknown option for {command}")))?;
        let value = args
            .next()
            .and_then(|v| v.into_string().ok())
            .filter(|v| !v.is_empty() && !v.starts_with("--") && !v.chars().any(char::is_control))
            .ok_or_else(|| Failure::Usage(format!("missing or invalid value after {key}")))?;
        if out.values.insert(key, value).is_some() {
            return Err(Failure::Usage(format!("{key} may be supplied only once")));
        }
    }
    Ok(out)
}

/// The desktop never runs elevated; the commands that read the live system follow the same rule.
fn refuse_elevated() -> Result<(), Failure> {
    #[cfg(windows)]
    if exor_win::process_is_elevated().map_err(failed)? {
        return Err(Failure::Failed(
            "ExorSystem does not run with administrator rights; run exor from a normal (not elevated) terminal"
                .into(),
        ));
    }
    Ok(())
}

/// Run one of [`COMMANDS`].
pub fn run(command: &str, args: impl Iterator<Item = OsString>) -> Result<Value, Failure> {
    let mut a = parse(command, args)?;
    if command != "diff" {
        refuse_elevated()?;
    }
    let body = match command {
        "programs" => list_programs(a.flags.contains("--all"))?,
        "leftovers" => {
            let id = a
                .values
                .remove("--program")
                .ok_or_else(|| Failure::Usage("--program ID is required".into()))?;
            find_leftovers(&id)?
        }
        "snapshot" => {
            let root = a.required("--root")?;
            take_snapshot(
                &root,
                a.path("--out").as_deref(),
                a.flags.contains("--strict"),
            )?
        }
        "diff" => {
            let (before, after) = (a.required("--before")?, a.required("--after")?);
            diff_files(&before, &after)?
        }
        "env" => check_environment(a.path("--script").as_deref())?,
        _ => return Err(Failure::Usage(format!("unknown command {command}"))),
    };
    Ok(body)
}

fn envelope(kind: &str, mut body: Value) -> Value {
    if let Value::Object(map) = &mut body {
        map.insert("schema_version".into(), json!(1));
        map.insert("kind".into(), json!(kind));
    }
    body
}

fn installed() -> Result<Vec<programs::InstalledProgram>, Failure> {
    #[cfg(windows)]
    {
        exor_win::enumerate_programs().map_err(failed)
    }
    #[cfg(not(windows))]
    {
        Err(Failure::Failed(
            "the installed-program list comes from the Windows registry; run this on Windows"
                .into(),
        ))
    }
}

fn list_programs(all: bool) -> Result<Value, Failure> {
    let rows = programs::visible(installed()?, all);
    Ok(envelope("installed_programs", json!({ "rows": rows })))
}

fn find_leftovers(id: &str) -> Result<Value, Failure> {
    let program = installed()?
        .into_iter()
        .find(|p| p.id == id)
        .ok_or_else(|| {
            Failure::Failed(format!(
                "no installed program has the id {id}; use an id from exor programs"
            ))
        })?;
    let rows = leftovers::find_leftovers(&program, &leftovers::default_roots());
    Ok(envelope(
        "leftover_suggestions",
        json!({
            "program": { "id": program.id, "name": program.name, "publisher": program.publisher },
            "rows": rows,
        }),
    ))
}

// ---- snapshots ----

const SNAPSHOT_FORMAT: &str = "exorsystem-snapshot";
const SNAPSHOT_VERSION: u32 = 1;

/// A snapshot file written by `snapshot --out`.
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

/// A path the capture could not read or hash.
#[derive(Debug, Serialize, Deserialize)]
pub struct Skipped {
    pub path: String,
    pub reason: String,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
}

/// Write a new file; an existing file is never replaced.
fn write_new(path: &Path, bytes: &[u8]) -> Result<(), Failure> {
    use std::io::Write;
    let mut f = std::fs::File::create_new(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::AlreadyExists {
            Failure::Failed(format!(
                "{} already exists; choose a new file name",
                path.display()
            ))
        } else {
            Failure::Failed(format!("could not create {}: {e}", path.display()))
        }
    })?;
    f.write_all(bytes)
        .and_then(|()| f.sync_all())
        .map_err(|e| Failure::Failed(format!("could not write {}: {e}", path.display())))
}

fn take_snapshot(root: &Path, out: Option<&Path>, strict: bool) -> Result<Value, Failure> {
    if !root.is_dir() {
        return Err(Failure::Failed(format!(
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
        .map_err(failed)?;
    let snap = runtime
        .block_on(snapshot::capture(root, &opts, None, &Cancel::new()))
        .map_err(failed)?;
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
            write_new(path, &serde_json::to_vec(&file).map_err(failed)?)?;
            Some(path.to_string_lossy().into_owned())
        }
        None => None,
    };
    Ok(envelope(
        "folder_snapshot",
        json!({
            "root": file.root,
            "files": snap.files,
            "entries": file.entries.len(),
            "skipped": file.skipped.len(),
            "complete": file.skipped.is_empty(),
            "root_digest": file.root_digest,
            "strict": strict,
            "written": written,
        }),
    ))
}

fn read_snapshot(path: &Path) -> Result<SnapshotFile, Failure> {
    let bytes = std::fs::read(path)
        .map_err(|e| Failure::Failed(format!("could not read {}: {e}", path.display())))?;
    let file: SnapshotFile = serde_json::from_slice(&bytes).map_err(|e| {
        Failure::Failed(format!(
            "{} is not an ExorSystem snapshot: {e}",
            path.display()
        ))
    })?;
    if file.format != SNAPSHOT_FORMAT || file.version != SNAPSHOT_VERSION {
        return Err(Failure::Failed(format!(
            "{} is format {} version {}; this exor reads {SNAPSHOT_FORMAT} version {SNAPSHOT_VERSION}",
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

fn diff_files(before: &Path, after: &Path) -> Result<Value, Failure> {
    let (old, new) = (read_snapshot(before)?, read_snapshot(after)?);
    let changes = trie::diff(&trie_of(&old), &trie_of(&new));
    // A path the later capture could not read is not evidence that it was removed.
    let unreadable_after: BTreeSet<String> =
        new.skipped.iter().map(|s| s.path.to_lowercase()).collect();
    let (unreadable, rows): (Vec<_>, Vec<_>) = changes.into_iter().partition(|c| {
        c.change == ChangeType::Removed && unreadable_after.contains(&c.path.to_lowercase())
    });
    let count = |t| rows.iter().filter(|c| c.change == t).count();
    Ok(envelope(
        "snapshot_diff",
        json!({
            "before": { "root": old.root, "captured_unix_ms": old.captured_unix_ms, "skipped": old.skipped.len() },
            "after": { "root": new.root, "captured_unix_ms": new.captured_unix_ms, "skipped": new.skipped.len() },
            "same_root": old.root.eq_ignore_ascii_case(&new.root),
            "complete": old.skipped.is_empty() && new.skipped.is_empty(),
            "added": count(ChangeType::Added),
            "removed": count(ChangeType::Removed),
            "modified": count(ChangeType::Modified),
            "rows": rows,
            "unreadable_in_after": unreadable.into_iter().map(|c| c.path).collect::<Vec<_>>(),
        }),
    ))
}

// ---- environment ----

/// `YYYY-MM-DD HH:MM UTC` for a Unix time in seconds (proleptic Gregorian calendar).
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

fn check_environment(script: Option<&Path>) -> Result<Value, Failure> {
    #[cfg(windows)]
    {
        use exor_engine::envcheck::{self, PathKind};
        let vars =
            exor_win::stored_variables(&envcheck::watched_names()).map_err(Failure::Failed)?;
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
                // UTF-8 with a byte-order mark, so Windows PowerShell 5.1 reads non-ASCII paths.
                let mut bytes = vec![0xEF, 0xBB, 0xBF];
                bytes.extend_from_slice(text.replace('\n', "\r\n").as_bytes());
                write_new(path, &bytes)?;
                Some(path.to_string_lossy().into_owned())
            }
            None => None,
        };
        Ok(envelope(
            "environment_check",
            json!({
                "variables": report.variables,
                "rows": report.findings,
                "removable": report.removable,
                "to_review": report.to_review,
                "script": written,
            }),
        ))
    }
    #[cfg(not(windows))]
    {
        let _ = script;
        Err(Failure::Failed(
            "stored environment variables are read from the Windows registry; run this on Windows"
                .into(),
        ))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn formats_utc_dates_including_leap_days() {
        assert_eq!(utc_text(0), "1970-01-01 00:00 UTC");
        assert_eq!(utc_text(951_782_400), "2000-02-29 00:00 UTC");
        assert_eq!(utc_text(1_795_057_362), "2026-11-19 03:02 UTC");
        assert_eq!(utc_text(4_102_444_799), "2099-12-31 23:59 UTC");
    }

    #[test]
    fn parses_values_and_flags_strictly() {
        let p = |c: &str, a: &[&str]| parse(c, a.iter().map(OsString::from));
        let a = p("snapshot", &["--root", "C:\\A", "--strict"])
            .ok()
            .unwrap();
        assert_eq!(a.values["--root"], "C:\\A");
        assert!(a.flags.contains("--strict"));
        for (c, args, want) in [
            ("programs", &["--everything"][..], "unknown option"),
            ("snapshot", &["--root"], "missing or invalid value"),
            (
                "snapshot",
                &["--root", "--strict"],
                "missing or invalid value",
            ),
            ("programs", &["--all", "--all"], "only once"),
            ("diff", &["--before", "a", "--before", "b"], "only once"),
        ] {
            match p(c, args) {
                Err(Failure::Usage(e)) => assert!(e.contains(want), "{c} {args:?}: {e}"),
                _ => panic!("{c} {args:?} should be a usage error"),
            }
        }
    }
}
