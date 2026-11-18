//! Guided uninstall helpers: split an uninstall command safely, find what an uninstaller left
//! behind, and quarantine it (reversible).
//!
//! Evidence ladder: the recorded install location is `0.7`; a folder whose name matches the
//! program's name is `0.3`. A publisher name alone never matches, because publisher folders are
//! shared between that publisher's programs.

use crate::collectors::{ServiceInfo, TaskInfo};
use crate::error::{EngineError, Result};
use crate::programs::{InstalledProgram, norm};
use crate::quarantine::{self, QuarantineItem};
use exor_storage::{
    ChangeKind, ChangeSource, NewLedgerEntry, QuarantineRecord, QuarantineStatus, Store,
};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

/// A folder an uninstaller may have left behind.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Leftover {
    /// Folder path.
    pub path: String,
    /// Why it was suggested, in plain words.
    pub reason: String,
    /// `0.7` recorded location, `0.3` name match.
    pub confidence: f64,
    /// Only folders below the configured application roots are eligible for file quarantine.
    pub can_quarantine: bool,
}

/// A registry key name read during the review. No registry values are read.
#[derive(Debug, Clone)]
pub struct RegistryKey {
    /// Key in its registry view, for display only.
    pub path: String,
    /// Final key component.
    pub name: String,
}

/// A system entry that can be inspected but not changed by ExorSystem.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewKind {
    /// Registry key.
    RegistryKey,
    /// Windows service.
    Service,
    /// Scheduled task.
    Task,
}

/// A possible leftover, always read-only. The match is not proof of ownership.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReviewLeftover {
    /// Class of item.
    pub kind: ReviewKind,
    /// Key, service name or task path.
    pub path: String,
    /// Why the item was suggested.
    pub reason: &'static str,
}

/// Split a registry uninstall command into program and arguments. No shell is involved.
/// Handles `"C:\a b\u.exe" /S`, `C:\a\u.exe /S` and `MsiExec.exe /X{GUID}`.
pub fn split_command(cmd: &str) -> Option<(String, Vec<String>)> {
    let cmd = cmd.trim();
    if cmd.is_empty() {
        return None;
    }
    let (exe, rest) = if let Some(r) = cmd.strip_prefix('"') {
        let end = r.find('"')?;
        (&r[..end], &r[end + 1..])
    } else {
        // an unquoted path may contain spaces: cut after the first ".exe" if there is one
        let lower = cmd.to_ascii_lowercase();
        match lower.find(".exe") {
            Some(i) => (&cmd[..i + 4], &cmd[i + 4..]),
            None => match cmd.split_once(char::is_whitespace) {
                Some((a, b)) => (a, b),
                None => (cmd, ""),
            },
        }
    };
    if exe.is_empty() {
        return None;
    }
    let mut args = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut any = false;
    for ch in rest.chars() {
        match ch {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any || !cur.is_empty() {
                    args.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => cur.push(c),
        }
    }
    if any || !cur.is_empty() {
        args.push(cur);
    }
    Some((exe.to_string(), args))
}

/// Paths that must never be suggested: roots, top-level folders and the Windows directory.
fn too_broad(p: &Path) -> bool {
    let depth = p
        .components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .count();
    if !p.is_absolute() || depth < 2 || p.components().any(|c| c == Component::ParentDir) {
        return true;
    }
    #[cfg(windows)]
    if !matches!(p.components().next(), Some(Component::Prefix(prefix)) if matches!(prefix.kind(), std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)))
    {
        return true;
    }
    let s = p.to_string_lossy().to_ascii_lowercase().replace('/', "\\");
    s.contains("\\windows\\") || s.ends_with("\\windows")
}

fn is_link(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // Junctions and other reparse points aren't necessarily reported as symlinks.
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    meta.file_type().is_symlink()
}

fn plain_folder(path: &Path) -> bool {
    path.ancestors()
        .filter(|p| p.parent().is_some())
        .all(|p| std::fs::symlink_metadata(p).is_ok_and(|m| m.is_dir() && !is_link(&m)))
}

fn in_application_root(path: &Path, roots: &[PathBuf]) -> bool {
    let Ok(real) = path.canonicalize() else {
        return false;
    };
    roots
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .any(|root| real != root && real.starts_with(root))
}

/// Folders left after uninstalling `p`. `roots` are the folders to look inside (Program Files,
/// AppData and similar); only direct children are examined.
pub fn find_leftovers(p: &InstalledProgram, roots: &[PathBuf]) -> Vec<Leftover> {
    let mut out: Vec<Leftover> = Vec::new();
    let mut add = |path: PathBuf, reason: &str, confidence: f64| {
        // The UI round-trips paths as strings; never offer a lossy path for an action.
        let Some(s) = path.to_str() else { return };
        if !too_broad(&path)
            && !out.iter().any(|l| l.path.eq_ignore_ascii_case(s))
            && plain_folder(&path)
        {
            out.push(Leftover {
                path: s.into(),
                reason: reason.into(),
                confidence,
                can_quarantine: in_application_root(&path, roots),
            });
        }
    };
    if let Some(loc) = p
        .install_location
        .as_deref()
        .filter(|l| !l.trim().is_empty())
    {
        add(
            PathBuf::from(loc.trim().trim_matches('"')),
            "This was the install folder.",
            0.7,
        );
    }
    let name = norm(&p.name);
    let publisher = p.publisher.as_deref().map(norm);
    if name.len() >= 4 {
        for root in roots {
            let Ok(rd) = std::fs::read_dir(root) else {
                continue;
            };
            for e in rd.flatten() {
                let n = norm(&e.file_name().to_string_lossy());
                if n.len() >= 4
                    && publisher.as_deref() != Some(n.as_str())
                    && (n == name || name.contains(&n) && n.len() * 2 >= name.len())
                {
                    add(e.path(), "The folder name matches the program.", 0.3);
                }
            }
        }
    }
    out.sort_by(|a, b| {
        b.confidence
            .total_cmp(&a.confidence)
            .then(a.path.cmp(&b.path))
    });
    out
}

// Compare Windows paths independent of the host OS, without wildcard, environment or UNC
// resolution. Unknown formats are excluded rather than used as evidence.
fn local_windows_path(raw: &str) -> Option<Vec<String>> {
    let raw = raw.trim().trim_matches('"');
    let bytes = raw.as_bytes();
    if !bytes.first().is_some_and(u8::is_ascii_alphabetic)
        || bytes.get(1) != Some(&b':')
        || !matches!(bytes.get(2), Some(b'\\' | b'/'))
    {
        return None;
    }
    let mut parts = vec![raw[..2].to_ascii_lowercase()];
    for word in raw[3..].split(['\\', '/']).filter(|word| !word.is_empty()) {
        if word.ends_with(['.', ' ']) || word.contains([':', '%', '"', '*', '?', '\0']) {
            return None;
        }
        parts.push(word.to_lowercase());
    }
    Some(parts)
}

/// Suggest possible registry keys, services and tasks. Name matching is exact after normalising
/// punctuation and case; publisher-only keys and services/tasks outside the install folder are
/// ignored. This is advisory only, even when a configured command matches.
pub fn review_system_leftovers(
    p: &InstalledProgram,
    registry: &[RegistryKey],
    services: &[ServiceInfo],
    tasks: &[TaskInfo],
) -> Vec<ReviewLeftover> {
    let mut out = Vec::new();
    let name = norm(&p.name);
    let publisher = p.publisher.as_deref().map(norm);
    if name.chars().count() >= 4 && publisher.as_deref() != Some(name.as_str()) {
        for key in registry.iter().filter(|key| norm(&key.name) == name) {
            out.push(ReviewLeftover {
                kind: ReviewKind::RegistryKey,
                path: key.path.clone(),
                reason: "The key name matches the program name.",
            });
        }
    }
    let root = p
        .install_location
        .as_deref()
        .and_then(local_windows_path)
        .filter(|parts| {
            // Ignore drive roots, top-level folders, Windows and bare profile/AppData roots.
            parts.len() >= 3
                && parts[1] != "windows"
                && !(parts[1] == "users"
                    && (parts.len() < 4 || (parts[3] == "appdata" && parts.len() < 6)))
        });
    let under_install = |path: &str| {
        root.as_ref().is_some_and(|root| {
            local_windows_path(path)
                .is_some_and(|parts| parts.len() > root.len() && parts.starts_with(root))
        })
    };
    for service in services {
        if split_command(&service.binary_path).is_some_and(|(exe, _)| under_install(&exe)) {
            out.push(ReviewLeftover {
                kind: ReviewKind::Service,
                path: service.name.clone(),
                reason: "The service command points inside the install folder.",
            });
        }
    }
    for task in tasks
        .iter()
        .filter(|task| task.command.as_deref().is_some_and(under_install))
    {
        out.push(ReviewLeftover {
            kind: ReviewKind::Task,
            path: task.path.clone(),
            reason: "The task command points inside the install folder.",
        });
    }
    out.sort_by_cached_key(|item| (item.kind, item.path.to_lowercase()));
    out.dedup_by(|a, b| a.kind == b.kind && a.path.to_lowercase() == b.path.to_lowercase());
    out
}

/// Every regular file under `dirs` (symlinks are not followed). At most `cap` files.
fn files_under(dirs: &[PathBuf], cap: usize) -> Result<Vec<PathBuf>> {
    let mut files = BTreeSet::new();
    let mut stack: Vec<PathBuf> = dirs.to_vec();
    while let Some(d) = stack.pop() {
        if !plain_folder(&d) {
            continue;
        }
        let rd = match std::fs::read_dir(&d) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for e in rd.flatten() {
            let Ok(meta) = std::fs::symlink_metadata(e.path()) else {
                continue;
            };
            if is_link(&meta) {
                continue;
            }
            if meta.is_dir() {
                stack.push(e.path());
            } else if meta.is_file() && files.insert(e.path()) && files.len() > cap {
                return Err(EngineError::Quarantine(format!(
                    "more than {cap} files; choose a smaller folder"
                )));
            }
        }
    }
    Ok(files.into_iter().collect())
}

/// Quarantine every file under the chosen folders and record it. Empty folders are left in place.
/// Returns the manifest id and the number of files moved.
pub fn quarantine_leftovers(
    store: &Store,
    quarantine_root: &Path,
    session_id: Option<i64>,
    dirs: &[PathBuf],
) -> Result<(String, usize)> {
    if let Some(bad) = dirs.iter().find(|d| too_broad(d)) {
        return Err(EngineError::Quarantine(format!(
            "{} is too broad to quarantine",
            bad.display()
        )));
    }
    let files = files_under(dirs, 50_000)?;
    if files.is_empty() {
        return Err(EngineError::Quarantine(
            "no files found in those folders".into(),
        ));
    }
    let (manifest, manifest_path) = quarantine::quarantine(quarantine_root, &files, &[])?;
    store.record_quarantine(&QuarantineRecord {
        quarantine_id: manifest.quarantine_id.clone(),
        rule_id: None,
        session_id,
        status: QuarantineStatus::Quarantined,
        manifest_path: manifest_path.to_string_lossy().into_owned(),
        manifest_json: serde_json::to_string(&manifest)?,
    })?;
    for it in &manifest.items {
        if let QuarantineItem::File {
            original_path,
            blake3,
            ..
        } = it
        {
            store.append_ledger(&NewLedgerEntry {
                session_id,
                node_id: None,
                path: original_path.clone(),
                change_kind: ChangeKind::Quarantined,
                old_hash: Some(blake3.clone()),
                new_hash: None,
                source: ChangeSource::Policy,
            })?;
        }
    }
    Ok((manifest.quarantine_id, files.len()))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use crate::programs::ProgramSource;
    use crate::programs::Scope;

    fn prog(name: &str, loc: Option<&str>) -> InstalledProgram {
        InstalledProgram {
            source: ProgramSource::Registry,
            id: "x".into(),
            name: name.into(),
            version: None,
            publisher: Some("Acme".into()),
            install_location: loc.map(Into::into),
            uninstall_command: None,
            quiet_uninstall_command: None,
            install_date: None,
            size_kb: None,
            scope: Scope::Machine,
            system_component: false,
            parent_key: None,
            windows_installer: false,
        }
    }

    #[test]
    fn splits_commands() {
        assert_eq!(
            split_command(r#""C:\Program Files\A B\unins000.exe" /SILENT /DIR="C:\x y""#),
            Some((
                r"C:\Program Files\A B\unins000.exe".into(),
                vec!["/SILENT".into(), r"/DIR=C:\x y".into()]
            ))
        );
        assert_eq!(
            split_command(r"C:\Program Files\A B\unins000.exe /S"),
            Some((
                r"C:\Program Files\A B\unins000.exe".into(),
                vec!["/S".into()]
            ))
        );
        assert_eq!(
            split_command("MsiExec.exe /X{ABC-1}"),
            Some(("MsiExec.exe".into(), vec!["/X{ABC-1}".into()]))
        );
        assert_eq!(split_command("setup"), Some(("setup".into(), vec![])));
        assert_eq!(split_command("   "), None);
        assert_eq!(split_command("\"unterminated"), None);
    }

    #[test]
    fn broad_paths_are_refused() {
        assert!(too_broad(Path::new("/")));
        assert!(too_broad(Path::new("/tmp")));
        assert!(too_broad(Path::new(r"C:\Windows\System32")));
        #[cfg(windows)]
        {
            assert!(too_broad(Path::new("/tmp/a/b")), "no drive letter");
            assert!(!too_broad(Path::new(r"C:\Program Files\Example")));
        }
        #[cfg(not(windows))]
        assert!(!too_broad(Path::new("/tmp/a/b")));
    }

    #[test]
    fn finds_install_folder_and_name_matches_but_not_publisher() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("pf");
        for n in ["FooBar", "Acme", "Other"] {
            std::fs::create_dir_all(root.join(n)).unwrap();
        }
        let loc = d.path().join("elsewhere/Foo");
        std::fs::create_dir_all(&loc).unwrap();
        let p = prog("Foo Bar", Some(loc.to_str().unwrap()));
        let found = find_leftovers(&p, std::slice::from_ref(&root));
        let names: Vec<_> = found.iter().map(|l| l.path.clone()).collect();
        assert_eq!(found[0].confidence, 0.7);
        assert!(names.iter().any(|n| n.ends_with("FooBar")));
        assert!(
            !names.iter().any(|n| n.ends_with("Acme")),
            "publisher folders are shared"
        );
        assert!(!names.iter().any(|n| n.ends_with("Other")));
    }

    #[test]
    fn review_requires_app_name_or_a_command_under_the_install_folder() {
        let p = prog("Foo Bar", Some(r"C:\Program Files\Foo Bar"));
        let registry = ["FooBar", "Acme", "FooBar2"].map(|name| RegistryKey {
            name: name.into(),
            path: format!(r"HKCU\Software\{name}"),
        });
        let service = |name: &str, command: &str| ServiceInfo {
            name: name.into(),
            display_name: "Foo Bar".into(),
            binary_path: command.into(),
            start_type: "auto".into(),
        };
        let services = [
            service("matches", r#""c:\PROGRAM FILES\foo bar\svc.exe" /service"#),
            service("sibling", r"C:\Program Files\Foo Bar Tools\svc.exe"),
            service("traversal", r"C:\Program Files\Foo Bar\..\Other\svc.exe"),
            service("environment", r"%ProgramFiles%\Foo Bar\svc.exe"),
        ];
        let task = |name: &str, command: &str| TaskInfo {
            path: name.into(),
            command: Some(command.into()),
            author: Some("Acme".into()),
        };
        let tasks = [
            task(r"\Foo\Updater", r"C:/Program Files/Foo Bar/update.exe"),
            task(r"\Foo\Other", r"C:\Windows\system32\cmd.exe"),
        ];
        let found = review_system_leftovers(&p, &registry, &services, &tasks);
        assert_eq!(
            found
                .iter()
                .map(|i| (i.kind, i.path.as_str()))
                .collect::<Vec<_>>(),
            [
                (ReviewKind::RegistryKey, r"HKCU\Software\FooBar"),
                (ReviewKind::Service, "matches"),
                (ReviewKind::Task, r"\Foo\Updater"),
            ]
        );
        let broad = prog("Acme", Some(r"C:\Program Files"));
        assert!(review_system_leftovers(&broad, &registry, &services, &tasks).is_empty());
    }

    #[test]
    fn install_folder_outside_application_roots_is_review_only() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("programs");
        let app = dir.path().join("other/App");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir_all(&app).unwrap();
        let p = prog("TestApp", app.to_str());
        let found = find_leftovers(&p, &[root]);
        assert_eq!(found.len(), 1);
        assert!(!found[0].can_quarantine);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_install_folder_is_not_suggested() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("programs");
        let target = dir.path().join("outside");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&target).unwrap();
        let link = root.join("TestApp");
        std::os::unix::fs::symlink(target, &link).unwrap();
        let p = prog("TestApp", link.to_str());
        assert!(find_leftovers(&p, &[root]).is_empty());
    }

    #[test]
    fn quarantines_and_restores_leftovers() {
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("app/Foo");
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a.txt"), b"a").unwrap();
        std::fs::write(dir.join("sub/b.txt"), b"b").unwrap();
        let store = Store::open_in_memory().unwrap();
        let q = d.path().join("q");
        let (id, n) = quarantine_leftovers(&store, &q, None, std::slice::from_ref(&dir)).unwrap();
        assert_eq!(n, 2);
        assert!(!dir.join("a.txt").exists());
        crate::execute::restore_by_id(&store, &id, false).unwrap();
        assert_eq!(std::fs::read(dir.join("sub/b.txt")).unwrap(), b"b");
        assert!(quarantine_leftovers(&store, &q, None, &[PathBuf::from("/tmp")]).is_err());
    }
}
