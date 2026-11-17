//! Installed-program inventory: what Windows says is installed, tidied for display and linked to
//! the installation sessions ExorSystem has recorded.
//!
//! Reading the registry lives in `exor-win`; everything here is plain, testable Rust.

use exor_storage::Session;
use serde::{Deserialize, Serialize};

/// Where an uninstall entry was found.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    /// `HKLM`, 64-bit view.
    Machine,
    /// `HKLM`, 32-bit view (WOW6432Node).
    Machine32,
    /// `HKCU`.
    User,
}

/// One entry from the Windows uninstall list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstalledProgram {
    /// Stable id: scope plus registry key name.
    pub id: String,
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
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<InstalledProgram> = programs
        .into_iter()
        .filter(|p| !p.name.trim().is_empty() && (include_system || p.is_listed()))
        .filter(|p| {
            seen.insert((
                p.name.trim().to_lowercase(),
                p.version.clone().unwrap_or_default().to_lowercase(),
                p.publisher.clone().unwrap_or_default().to_lowercase(),
            ))
        })
        .collect();
    out.sort_by_key(|p| (p.name.to_lowercase(), p.version.clone().unwrap_or_default()));
    out
}

fn norm(s: &str) -> String {
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
