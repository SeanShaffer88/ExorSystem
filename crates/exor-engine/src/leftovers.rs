//! Guided uninstall helpers: split an uninstall command safely, find what an uninstaller left
//! behind, and quarantine it (reversible).
//!
//! Evidence ladder: the recorded install location is `0.7`; a folder whose name matches the
//! program's name is `0.3`. A publisher name alone never matches, because publisher folders are
//! shared between that publisher's programs.

use crate::error::{EngineError, Result};
use crate::programs::{InstalledProgram, norm};
use crate::quarantine::{self, QuarantineItem};
use exor_storage::{ChangeKind, ChangeSource, NewLedgerEntry, Store};
use serde::Serialize;
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

/// Build a Windows command line from a program and its arguments, quoting the way the C runtime
/// (and `std::process::Command`) parses it back. The inverse of [`split_command`].
pub fn join_command(exe: &str, args: &[String]) -> String {
    fn quote(out: &mut String, a: &str) {
        if !a.is_empty() && !a.contains([' ', '\t', '"']) {
            out.push_str(a);
            return;
        }
        out.push('"');
        let mut slashes = 0usize;
        for c in a.chars() {
            match c {
                '\\' => slashes += 1,
                '"' => {
                    out.extend(std::iter::repeat_n('\\', slashes * 2 + 1));
                    out.push('"');
                    slashes = 0;
                }
                c => {
                    out.extend(std::iter::repeat_n('\\', slashes));
                    out.push(c);
                    slashes = 0;
                }
            }
        }
        out.extend(std::iter::repeat_n('\\', slashes * 2));
        out.push('"');
    }
    // the program name is parsed by different rules: quote it whole, it cannot contain quotes
    let mut out = format!("\"{exe}\"");
    for a in args {
        out.push(' ');
        quote(&mut out, a);
    }
    out
}

/// Folder names that are shared by many programs (or hold user data) and are never a leftover.
const SHARED_NAMES: &[&str] = &[
    "appdata",
    "common files",
    "default",
    "desktop",
    "documents",
    "downloads",
    "local",
    "locallow",
    "microsoft",
    "microsoft shared",
    "onedrive",
    "packages",
    "program files",
    "program files (x86)",
    "programdata",
    "programs",
    "public",
    "roaming",
    "temp",
    "users",
    "windowsapps",
];

/// Environment variables naming folders that must never be quarantined, nor any folder above them.
const PROTECTED_VARS: &[&str] = &[
    "ProgramFiles",
    "ProgramFiles(x86)",
    "ProgramW6432",
    "CommonProgramFiles",
    "CommonProgramFiles(x86)",
    "CommonProgramW6432",
    "ProgramData",
    "LOCALAPPDATA",
    "APPDATA",
    "USERPROFILE",
    "PUBLIC",
    "SystemRoot",
    "TEMP",
    "TMP",
];

/// Folders inside a user profile that hold the user's own data.
const PROFILE_FOLDERS: &[&str] = &[
    "Contacts",
    "Desktop",
    "Documents",
    "Downloads",
    "Favorites",
    "Links",
    "Music",
    "OneDrive",
    "Pictures",
    "Saved Games",
    "Searches",
    "Videos",
];

/// Lower-case, backslash-separated, without a `\\?\` prefix or trailing separators.
fn norm_path(p: &Path) -> String {
    let s = p.to_string_lossy().replace('/', "\\").to_lowercase();
    let s = s.strip_prefix("\\\\?\\").unwrap_or(&s);
    s.trim_end_matches('\\').to_string()
}

/// Folders (normalized) that a leftover may be neither equal to nor above: `extra` plus the
/// well-known system and profile folders from the environment.
fn protected_paths(extra: &[PathBuf]) -> Vec<String> {
    let mut out: Vec<PathBuf> = extra.to_vec();
    out.extend(
        PROTECTED_VARS
            .iter()
            .filter_map(std::env::var_os)
            .map(PathBuf::from),
    );
    if let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
        out.extend(PROFILE_FOLDERS.iter().map(|f| home.join(f)));
    }
    out.iter()
        .map(|p| norm_path(p))
        .filter(|s| !s.is_empty())
        .collect()
}

/// Paths that must never be suggested or quarantined: roots, top-level folders, the Windows
/// directory, shared folders such as `Common Files`, and any folder that is (or contains) a
/// protected folder such as Program Files, AppData or the user's profile.
fn too_broad(p: &Path, protected: &[String]) -> bool {
    let depth = p
        .components()
        .filter(|c| matches!(c, Component::Normal(_)))
        .count();
    if depth < 2 {
        return true;
    }
    let s = norm_path(p);
    if s.contains("\\windows\\") || s.ends_with("\\windows") {
        return true;
    }
    let last = s.rsplit('\\').next().unwrap_or("");
    if SHARED_NAMES.contains(&last) {
        return true;
    }
    let under = format!("{s}\\");
    protected.iter().any(|q| *q == s || q.starts_with(&under))
}

/// First word of `s`, normalized.
fn first_word(s: &str) -> String {
    s.split_whitespace().next().map(norm).unwrap_or_default()
}

/// A folder named only after the vendor ("Google" for "Google Chrome", "Mozilla" for
/// "Mozilla Firefox") is shared by that vendor's programs and is not a leftover.
fn is_vendor_folder(folder: &str, p: &InstalledProgram, name: &str) -> bool {
    if folder == name {
        return false;
    }
    if let Some(publisher) = p.publisher.as_deref()
        && (folder == norm(publisher) || folder == first_word(publisher))
    {
        return true;
    }
    p.name.split_whitespace().count() > 1 && folder == first_word(&p.name)
}

/// Folders left after uninstalling `p`. `roots` are the folders to look inside (Program Files,
/// AppData and similar); only direct children are examined.
pub fn find_leftovers(p: &InstalledProgram, roots: &[PathBuf]) -> Vec<Leftover> {
    let protected = protected_paths(roots);
    let mut out: Vec<Leftover> = Vec::new();
    let mut add = |path: PathBuf, reason: &str, confidence: f64| {
        if path.is_dir() && !too_broad(&path, &protected) {
            let s = path.to_string_lossy().into_owned();
            if !out.iter().any(|l| l.path.eq_ignore_ascii_case(&s)) {
                out.push(Leftover {
                    path: s,
                    reason: reason.into(),
                    confidence,
                });
            }
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
    if name.len() >= 4 {
        for root in roots {
            let Ok(rd) = std::fs::read_dir(root) else {
                continue;
            };
            for e in rd.flatten() {
                let n = norm(&e.file_name().to_string_lossy());
                if n.len() >= 4
                    && (n == name || name.contains(&n) && n.len() * 2 >= name.len())
                    && !is_vendor_folder(&n, p, &name)
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

/// Every regular file under `dirs` (symlinks are not followed). At most `cap` files.
fn files_under(dirs: &[PathBuf], cap: usize) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut stack: Vec<PathBuf> = dirs.to_vec();
    while let Some(d) = stack.pop() {
        let rd = match std::fs::read_dir(&d) {
            Ok(r) => r,
            Err(_) => continue,
        };
        for e in rd.flatten() {
            let Ok(t) = e.file_type() else { continue };
            if t.is_symlink() {
                continue;
            }
            if t.is_dir() {
                stack.push(e.path());
            } else if t.is_file() {
                if files.len() >= cap {
                    return Err(EngineError::Quarantine(format!(
                        "more than {cap} files; choose a smaller folder"
                    )));
                }
                files.push(e.path());
            }
        }
    }
    Ok(files)
}

/// Quarantine every file under the chosen folders and record it. Empty folders are left in place.
/// Returns the manifest id and the number of files moved.
pub fn quarantine_leftovers(
    store: &Store,
    quarantine_root: &Path,
    session_id: Option<i64>,
    dirs: &[PathBuf],
) -> Result<(String, usize)> {
    let protected = protected_paths(&[]);
    if let Some(bad) = dirs.iter().find(|d| too_broad(d, &protected)) {
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
    crate::execute::record_or_undo(store, &manifest, &manifest_path, session_id)?;
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
    use crate::programs::Scope;

    fn prog(name: &str, loc: Option<&str>) -> InstalledProgram {
        InstalledProgram {
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
    fn joins_commands_with_crt_quoting() {
        let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            join_command(r"C:\A B\u.exe", &args(&["/S", r"/DIR=C:\x y"])),
            r#""C:\A B\u.exe" /S "/DIR=C:\x y""#
        );
        assert_eq!(
            join_command("MsiExec.exe", &args(&["/X{ABC-1}"])),
            r#""MsiExec.exe" /X{ABC-1}"#
        );
        // empty arguments survive, quotes are escaped, trailing backslashes are doubled
        assert_eq!(
            join_command("u.exe", &args(&["", r#"a"b"#, r"C:\dir\ x\"])),
            r#""u.exe" "" "a\"b" "C:\dir\ x\\""#
        );
        // splitting what was joined gives the same program and arguments back
        let (exe, a) = split_command(r#""C:\A B\u.exe" /S "/DIR=C:\x y""#).unwrap();
        assert_eq!(join_command(&exe, &a), r#""C:\A B\u.exe" /S "/DIR=C:\x y""#);
    }

    #[test]
    fn broad_paths_are_refused() {
        assert!(too_broad(Path::new("/"), &[]));
        assert!(too_broad(Path::new("/tmp"), &[]));
        assert!(too_broad(Path::new(r"C:\Windows\System32"), &[]));
        assert!(!too_broad(Path::new("/tmp/a/b"), &[]));
    }

    #[test]
    fn shared_and_protected_folders_are_refused() {
        let prot = vec![
            norm_path(Path::new(r"C:\Users\Sam\AppData\Local")),
            norm_path(Path::new(r"C:\Users\Sam")),
        ];
        // a protected folder itself, and every folder above it
        assert!(too_broad(Path::new(r"C:\Users\Sam\AppData\Local"), &prot));
        assert!(too_broad(Path::new("c:/users/sam/appdata/local/"), &prot));
        assert!(too_broad(Path::new(r"C:\Users\Sam\AppData"), &prot));
        assert!(too_broad(Path::new(r"C:\Users\Sam"), &prot));
        assert!(too_broad(Path::new(r"\\?\C:\Users\Sam"), &prot));
        // shared folders, wherever they are
        assert!(too_broad(Path::new(r"C:\Program Files\Common Files"), &[]));
        assert!(too_broad(Path::new(r"D:\Apps\Microsoft"), &[]));
        // a program's own folder inside a protected one is fine
        assert!(!too_broad(
            Path::new(r"C:\Users\Sam\AppData\Local\Foo"),
            &prot
        ));
    }

    #[test]
    fn install_location_pointing_at_a_root_is_not_suggested() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("data").join("Roots");
        std::fs::create_dir_all(root.join("Foo")).unwrap();
        let p = prog("Foo Bar", Some(root.to_str().unwrap()));
        let found = find_leftovers(&p, std::slice::from_ref(&root));
        assert!(
            !found.iter().any(|l| Path::new(&l.path) == root),
            "a search root is never a leftover: {found:?}"
        );
    }

    #[test]
    fn vendor_folders_are_not_leftovers() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("pf");
        for n in [
            "Google",
            "Chrome",
            "Mozilla",
            "Mozilla Firefox",
            "Microsoft",
            "Teams",
        ] {
            std::fs::create_dir_all(root.join(n)).unwrap();
        }
        let names = |p: &InstalledProgram| -> Vec<String> {
            find_leftovers(p, std::slice::from_ref(&root))
                .into_iter()
                .filter_map(|l| {
                    Path::new(&l.path)
                        .file_name()
                        .map(|f| f.to_string_lossy().into_owned())
                })
                .collect()
        };
        let mut chrome = prog("Google Chrome", None);
        chrome.publisher = Some("Google LLC".into());
        assert_eq!(names(&chrome), vec!["Chrome".to_string()]);
        let mut firefox = prog("Mozilla Firefox", None);
        firefox.publisher = Some("Mozilla".into());
        assert_eq!(names(&firefox), vec!["Mozilla Firefox".to_string()]);
        let mut teams = prog("Microsoft Teams", None);
        teams.publisher = None;
        assert!(!names(&teams).contains(&"Microsoft".to_string()));
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
