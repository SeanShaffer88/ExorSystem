//! Installed-program inventory: Windows entries plus portable apps added by the user.
//!
//! Reading the Windows inventory lives in `exor-win`; display and matching logic is portable.

use exor_storage::{PortableProgram, Session};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path};

/// Where an uninstall entry was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// `HKLM`, 64-bit view.
    Machine,
    /// `HKLM`, 32-bit view (WOW6432Node).
    Machine32,
    /// Current Windows user, or a user-added portable app.
    User,
}

/// How an inventory entry was found. Only registry entries have registered uninstall commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProgramSource {
    /// Classic program listed under Uninstall in the Windows registry.
    Registry,
    /// App package registered with Windows for the current user.
    Msix,
    /// Executable explicitly added by the user; not necessarily installed.
    Portable,
}

/// One entry in the program inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledProgram {
    /// Stable id scoped to the source.
    pub id: String,
    /// Source of the entry.
    pub source: ProgramSource,
    /// Display name.
    pub name: String,
    /// Version text.
    pub version: Option<String>,
    /// Publisher.
    pub publisher: Option<String>,
    /// Install folder.
    pub install_location: Option<String>,
    /// Command that starts the uninstaller.
    pub uninstall_command: Option<String>,
    /// Silent uninstall command, when the installer provides one.
    pub quiet_uninstall_command: Option<String>,
    /// Install date as `YYYY-MM-DD`, when valid.
    pub install_date: Option<String>,
    /// Estimated size in KiB.
    pub size_kb: Option<u64>,
    /// Where it was found.
    pub scope: Scope,
    /// Marked as a system component (hidden by Windows' own list).
    pub system_component: bool,
    /// Set for updates and patches that belong to another entry.
    pub parent_key: Option<String>,
    /// Installed by Windows Installer (MSI).
    pub windows_installer: bool,
}

/// `YYYYMMDD` (as stored by most installers) to `YYYY-MM-DD`; anything else is dropped.
pub fn format_install_date(raw: &str) -> Option<String> {
    let r = raw.trim();
    if r.len() != 8 || !r.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (y, m, d) = (&r[..4], &r[4..6], &r[6..]);
    let (mi, di): (u32, u32) = (m.parse().ok()?, d.parse().ok()?);
    ((1..=12).contains(&mi) && (1..=31).contains(&di) && y != "0000")
        .then(|| format!("{y}-{m}-{d}"))
}

impl InstalledProgram {
    /// Windows itself hides entries with no name, system components and child updates.
    pub fn is_listed(&self) -> bool {
        !self.name.trim().is_empty() && !self.system_component && self.parent_key.is_none()
    }
}

/// Tidy for display: drop hidden entries (unless `include_system`), collapse the same program
/// reported by both registry views, and sort by name ignoring case.
pub fn visible(programs: Vec<InstalledProgram>, include_system: bool) -> Vec<InstalledProgram> {
    let mut seen_registry = std::collections::HashSet::new();
    let mut seen_other = std::collections::HashSet::new();
    let mut out: Vec<InstalledProgram> = programs
        .into_iter()
        .filter(|p| !p.name.trim().is_empty() && (include_system || p.is_listed()))
        .filter(|p| {
            if p.source == ProgramSource::Registry {
                seen_registry.insert((
                    p.name.trim().to_lowercase(),
                    p.version.clone().unwrap_or_default().to_lowercase(),
                    p.publisher.clone().unwrap_or_default().to_lowercase(),
                    p.scope == Scope::User,
                ))
            } else {
                seen_other.insert((p.source, p.id.to_lowercase()))
            }
        })
        .collect();
    out.sort_by_key(|p| (p.name.to_lowercase(), p.version.clone().unwrap_or_default()));
    out
}

/// Validate an explicit portable entry. The executable is only checked, never loaded or run.
pub fn portable_registration(name: &str, executable: &str) -> Result<PortableProgram, String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 120 || name.chars().any(char::is_control) {
        return Err("enter a program name of 1–120 characters".into());
    }
    let input = executable.trim();
    let path = Path::new(
        input
            .strip_prefix('"')
            .and_then(|s| s.strip_suffix('"'))
            .unwrap_or(input),
    );
    if !path.is_absolute()
        || path.components().any(|c| c == Component::ParentDir)
        || !path
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
    {
        return Err("enter the full local path to an .exe file".into());
    }
    #[cfg(windows)]
    if !matches!(path.components().next(), Some(Component::Prefix(p)) if matches!(p.kind(), std::path::Prefix::Disk(_)))
    {
        return Err("enter a local drive path, such as C:\\Tools\\Program.exe".into());
    }
    let meta =
        std::fs::symlink_metadata(path).map_err(|e| format!("cannot check the file: {e}"))?;
    if !meta.is_file() {
        return Err("choose an executable file, not a folder or link".into());
    }
    let canonical = path
        .canonicalize()
        .map_err(|e| format!("cannot resolve the file: {e}"))?;
    #[cfg(windows)]
    if !matches!(canonical.components().next(), Some(Component::Prefix(p)) if matches!(p.kind(), std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)))
    {
        return Err("the executable must be on a local drive".into());
    }
    let resolved = canonical
        .to_str()
        .ok_or_else(|| "the path is not valid text".to_string())?;
    // Canonical paths on Windows have a verbatim prefix. Use familiar drive spelling in the UI.
    #[cfg(windows)]
    let resolved = resolved.strip_prefix(r"\\?\").unwrap_or(resolved);
    Ok(PortableProgram {
        name: name.into(),
        executable: resolved.into(),
    })
}

/// Display-only inventory entry. It never has a command to run or a cleanup target.
pub fn portable_program(entry: PortableProgram) -> InstalledProgram {
    InstalledProgram {
        id: format!("portable:{}", entry.executable),
        source: ProgramSource::Portable,
        name: entry.name,
        version: None,
        publisher: None,
        install_location: Path::new(&entry.executable)
            .parent()
            .map(|p| p.to_string_lossy().into_owned()),
        uninstall_command: None,
        quiet_uninstall_command: None,
        install_date: None,
        size_kb: None,
        scope: Scope::User,
        system_component: false,
        parent_key: None,
        windows_installer: false,
    }
}

pub(crate) fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Recorded sessions that belong to this program: equal names, or one name inside the other
/// (for example session "Foo" and program "Foo 2.1 (x64)"). Names under 4 letters must match exactly.
pub fn match_sessions<'a>(p: &InstalledProgram, sessions: &'a [Session]) -> Vec<&'a Session> {
    let pn = norm(&p.name);
    sessions
        .iter()
        .filter(|s| {
            let sn = norm(&s.app_name);
            !sn.is_empty()
                && (sn == pn
                    || (sn.len() >= 4 && pn.contains(&sn))
                    || (pn.len() >= 4 && sn.contains(&pn)))
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn prog(name: &str, ver: &str, scope: Scope) -> InstalledProgram {
        InstalledProgram {
            id: format!("{scope:?}\\{name}"),
            source: ProgramSource::Registry,
            name: name.into(),
            version: Some(ver.into()),
            publisher: Some("Pub".into()),
            install_location: None,
            uninstall_command: None,
            quiet_uninstall_command: None,
            install_date: None,
            size_kb: None,
            scope,
            system_component: false,
            parent_key: None,
            windows_installer: false,
        }
    }

    fn session(id: i64, name: &str) -> Session {
        Session {
            session_id: id,
            app_name: name.into(),
            installer_path: None,
            fingerprint_family: None,
            started_at: None,
            ended_at: None,
            status: None,
        }
    }

    #[test]
    fn install_dates() {
        assert_eq!(
            format_install_date("20240131").as_deref(),
            Some("2024-01-31")
        );
        for bad in [
            "",
            "2024-01-31",
            "20241301",
            "20240100",
            "00000101",
            "2024013",
        ] {
            assert_eq!(format_install_date(bad), None, "{bad}");
        }
    }

    #[test]
    fn hides_system_and_updates_dedupes_and_sorts() {
        let mut hidden = prog("Runtime", "1", Scope::Machine);
        hidden.system_component = true;
        let mut update = prog("Patch KB1", "1", Scope::Machine);
        update.parent_key = Some("Foo".into());
        let list = vec![
            prog("zeta", "1.0", Scope::Machine),
            prog("Alpha", "2.0", Scope::Machine),
            prog("ALPHA", "2.0", Scope::Machine32), // same program via the 32-bit view
            prog("", "1", Scope::User),
            hidden.clone(),
            update,
        ];
        let v = visible(list.clone(), false);
        assert_eq!(
            v.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            ["Alpha", "zeta"]
        );
        let all = visible(list, true);
        assert!(
            all.iter().any(|p| p.name == "Runtime"),
            "system entries shown on request"
        );
        assert_eq!(all.len(), 4);
    }

    #[test]
    fn different_versions_are_different_entries() {
        let v = visible(
            vec![
                prog("Foo", "1.0", Scope::Machine),
                prog("Foo", "2.0", Scope::Machine),
            ],
            false,
        );
        assert_eq!(v.len(), 2);
    }

    #[test]
    fn different_inventory_sources_remain_distinct() {
        let registry = prog("Widget", "1.0", Scope::Machine);
        let mut package = registry.clone();
        package.id = "msix:Vendor.Widget_abc".into();
        package.source = ProgramSource::Msix;
        let mut other = package.clone();
        other.id = "msix:Other.Widget_def".into();
        let portable = portable_program(PortableProgram {
            name: "Widget".into(),
            executable: "C:/Tools/Widget.exe".into(),
        });
        assert!(portable.uninstall_command.is_none());
        let listed = visible(
            vec![registry, package.clone(), package, other, portable],
            false,
        );
        assert_eq!(listed.len(), 4);
        assert_eq!(
            listed
                .iter()
                .filter(|p| p.source == ProgramSource::Msix)
                .count(),
            2
        );
    }

    #[test]
    fn portable_registration_checks_name_path_and_type_without_executing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("app.EXE");
        let exe = path.to_str().unwrap();
        assert!(portable_registration("App", exe).is_err());
        std::fs::write(&path, b"not an executable").unwrap();
        let entry = portable_registration("  App  ", &format!("\"{exe}\"")).unwrap();
        assert_eq!(entry.name, "App");
        assert!(portable_registration("", exe).is_err());
        assert!(portable_registration("App", "app.EXE").is_err());
        let folder = dir.path().join("folder.exe");
        std::fs::create_dir(&folder).unwrap();
        assert!(portable_registration("App", folder.to_str().unwrap()).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"not an executable");
    }

    #[test]
    fn sessions_match_by_name() {
        let s = vec![
            session(1, "Foobar"),
            session(2, "Bar Studio"),
            session(3, "Qt"),
            session(4, "7-Zip"),
        ];
        let ids = |p: &InstalledProgram| {
            match_sessions(p, &s)
                .iter()
                .map(|x| x.session_id)
                .collect::<Vec<_>>()
        };
        assert_eq!(ids(&prog("Foobar 2.1 (x64)", "2.1", Scope::Machine)), [1]);
        assert_eq!(ids(&prog("bar studio", "1", Scope::User)), [2]);
        assert_eq!(
            ids(&prog("Qt Creator", "1", Scope::Machine)),
            Vec::<i64>::new(),
            "3-letter names must match exactly"
        );
        assert_eq!(
            ids(&prog("7 Zip", "23", Scope::Machine)),
            [4],
            "punctuation is ignored"
        );
        assert_eq!(
            ids(&prog("Unrelated", "1", Scope::Machine)),
            Vec::<i64>::new()
        );
    }
}
