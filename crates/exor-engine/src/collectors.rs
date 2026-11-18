//! Service and scheduled-task collectors (portable parts).
//!
//! Enumerating live services needs the SCM (`exor-win`); everything else — snapshot diffing and
//! reading task definitions from `%SystemRoot%\System32\Tasks` — is plain, testable Rust.

use crate::error::{Result, io_err};
use crate::trie::ChangeType;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// A Windows service or kernel-driver service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceInfo {
    /// Service key name.
    pub name: String,
    /// Display name.
    pub display_name: String,
    /// Image path / command line.
    pub binary_path: String,
    /// Start type text (`auto`, `manual`, `disabled`, `boot`, `system`).
    pub start_type: String,
}

/// A scheduled task definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskInfo {
    /// Task path such as `\Vendor\Updater`.
    pub path: String,
    /// First action command, if any.
    pub command: Option<String>,
    /// Author field, if any.
    pub author: Option<String>,
}

/// One service difference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceChange {
    /// New state for added/modified, old state for removed.
    pub service: ServiceInfo,
    /// Kind of change.
    pub change: ChangeType,
}

/// One task difference.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskChange {
    /// New state for added/modified, old state for removed.
    pub task: TaskInfo,
    /// Kind of change.
    pub change: ChangeType,
}

pub(crate) fn diff_by_key<T: Clone>(
    old: &[T],
    new: &[T],
    key: impl Fn(&T) -> String,
    same: impl Fn(&T, &T) -> bool,
) -> Vec<(T, ChangeType)> {
    let o: BTreeMap<String, &T> = old.iter().map(|x| (key(x), x)).collect();
    let n: BTreeMap<String, &T> = new.iter().map(|x| (key(x), x)).collect();
    let mut out = Vec::new();
    for (k, v) in &n {
        match o.get(k) {
            None => out.push(((*v).clone(), ChangeType::Added)),
            Some(prev) if !same(prev, v) => out.push(((*v).clone(), ChangeType::Modified)),
            _ => {}
        }
    }
    for (k, v) in &o {
        if !n.contains_key(k) {
            out.push(((*v).clone(), ChangeType::Removed));
        }
    }
    out
}

/// Compare two service snapshots (names are case-insensitive).
pub fn diff_services(old: &[ServiceInfo], new: &[ServiceInfo]) -> Vec<ServiceChange> {
    diff_by_key(
        old,
        new,
        |s| s.name.to_lowercase(),
        |a, b| {
            a.name.eq_ignore_ascii_case(&b.name)
                && a.display_name == b.display_name
                && a.binary_path == b.binary_path
                && a.start_type == b.start_type
        },
    )
    .into_iter()
    .map(|(service, change)| ServiceChange { service, change })
    .collect()
}

/// Compare two task snapshots (paths are case-insensitive).
pub fn diff_tasks(old: &[TaskInfo], new: &[TaskInfo]) -> Vec<TaskChange> {
    diff_by_key(
        old,
        new,
        |t| t.path.to_lowercase(),
        |a, b| {
            a.path.eq_ignore_ascii_case(&b.path) && a.command == b.command && a.author == b.author
        },
    )
    .into_iter()
    .map(|(task, change)| TaskChange { task, change })
    .collect()
}

fn tag(xml: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&format!("</{name}>"))? + start;
    let v = xml[start..end].trim();
    (!v.is_empty()).then(|| {
        v.replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
    })
}

/// Parse a task definition. `rel_path` is the file's path below the Tasks folder.
pub fn parse_task_xml(rel_path: &str, xml: &str) -> TaskInfo {
    let path = format!(
        "\\{}",
        rel_path.trim_start_matches(['\\', '/']).replace('/', "\\")
    );
    TaskInfo {
        path,
        command: tag(xml, "Command"),
        author: tag(xml, "Author"),
    }
}

fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        String::from_utf8_lossy(bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes))
            .into_owned()
    }
}

/// Read task definitions under a Tasks folder (blocking). An unreadable definition fails the
/// snapshot so it cannot look like a removed task when the snapshots are compared.
pub fn read_tasks_dir(root: &Path) -> Result<Vec<TaskInfo>> {
    fn rec(base: &Path, dir: &Path, out: &mut Vec<TaskInfo>) -> Result<()> {
        for entry in std::fs::read_dir(dir).map_err(io_err(dir))? {
            let entry = entry.map_err(io_err(dir))?;
            let p = entry.path();
            let ft = entry.file_type().map_err(io_err(&p))?;
            if ft.is_dir() {
                rec(base, &p, out)?;
            } else if ft.is_file()
                && let Ok(rel) = p.strip_prefix(base)
            {
                let bytes = std::fs::read(&p).map_err(io_err(&p))?;
                out.push(parse_task_xml(&rel.to_string_lossy(), &decode(&bytes)));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    rec(root, root, &mut out)?;
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn svc(name: &str, path: &str) -> ServiceInfo {
        ServiceInfo {
            name: name.into(),
            display_name: name.into(),
            binary_path: path.into(),
            start_type: "auto".into(),
        }
    }

    #[test]
    fn service_diff() {
        let old = [svc("A", "a.exe"), svc("B", "b.exe"), svc("C", "c.exe")];
        let new = [
            ServiceInfo {
                name: "a".into(),
                display_name: "A".into(),
                binary_path: "a.exe".into(),
                start_type: "auto".into(),
            },
            svc("B", "b2.exe"),
            svc("D", "d.exe"),
        ];
        let d = diff_services(&old, &new);
        let get = |n: &str| {
            d.iter()
                .find(|c| c.service.name.eq_ignore_ascii_case(n))
                .map(|c| c.change)
        };
        assert_eq!(get("a"), None, "case-insensitive, unchanged");
        assert_eq!(get("B"), Some(ChangeType::Modified));
        assert_eq!(get("C"), Some(ChangeType::Removed));
        assert_eq!(get("D"), Some(ChangeType::Added));
    }

    #[test]
    fn parses_task_xml_utf8_and_utf16() {
        let xml = "<Task><RegistrationInfo><Author>Foo &amp; Co</Author></RegistrationInfo><Actions><Exec><Command>\"C:\\Foo\\up.exe\"</Command></Exec></Actions></Task>";
        let t = parse_task_xml("Foo/Updater", xml);
        assert_eq!(t.path, "\\Foo\\Updater");
        assert_eq!(t.command.as_deref(), Some("\"C:\\Foo\\up.exe\""));
        assert_eq!(t.author.as_deref(), Some("Foo & Co"));

        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("Foo")).unwrap();
        let mut u16bytes = vec![0xFF, 0xFE];
        u16bytes.extend(xml.encode_utf16().flat_map(u16::to_le_bytes));
        std::fs::write(dir.path().join("Foo").join("Updater"), u16bytes).unwrap();
        std::fs::write(dir.path().join("Plain"), "<Task/>").unwrap();
        let tasks = read_tasks_dir(dir.path()).unwrap();
        assert_eq!(tasks.len(), 2);
        assert_eq!(tasks[0].path, "\\Foo\\Updater");
        assert_eq!(tasks[0].command.as_deref(), Some("\"C:\\Foo\\up.exe\""));
    }

    #[cfg(windows)]
    #[test]
    fn unreadable_task_definition_does_not_return_a_partial_snapshot() {
        use std::os::windows::fs::OpenOptionsExt;

        let dir = tempfile::tempdir().unwrap();
        let locked = dir.path().join("Locked");
        std::fs::write(&locked, "<Task/>").unwrap();
        std::fs::write(dir.path().join("Readable"), "<Task/>").unwrap();
        let handle = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(locked)
            .unwrap();
        assert!(read_tasks_dir(dir.path()).is_err());
        drop(handle);
        assert_eq!(read_tasks_dir(dir.path()).unwrap().len(), 2);
    }

    #[test]
    fn task_diff() {
        let t = |p: &str, c: &str| TaskInfo {
            path: p.into(),
            command: Some(c.into()),
            author: None,
        };
        let d = diff_tasks(&[t("\\A", "x")], &[t("\\A", "y"), t("\\B", "z")]);
        assert_eq!(d.len(), 2);
        assert!(
            d.iter()
                .any(|c| c.task.path == "\\A" && c.change == ChangeType::Modified)
        );
        assert!(
            d.iter()
                .any(|c| c.task.path == "\\B" && c.change == ChangeType::Added)
        );
    }
}
