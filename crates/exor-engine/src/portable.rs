//! Portable-app transition assistant: what can move into a self-contained folder, what cannot.

use crate::error::{EngineError, Result};
use exor_storage::{Node, NodeType};
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

/// Plan for turning an installed app into a portable one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortablePlan {
    /// Files to copy beneath the portable app folder.
    pub copy_files: Vec<String>,
    /// Registry keys/values to export and replay at launch.
    pub export_registry: Vec<String>,
    /// Items that cannot be made portable (services, scheduled tasks), with the reason.
    pub unsupported: Vec<(String, String)>,
}

/// Build a plan from the nodes attributed to a session.
pub fn plan_portable(nodes: &[Node]) -> PortablePlan {
    let mut p = PortablePlan::default();
    for n in nodes {
        match n.node_type {
            NodeType::File => p.copy_files.push(n.path.clone()),
            NodeType::RegistryKey | NodeType::RegistryValue => {
                p.export_registry.push(n.path.clone())
            }
            NodeType::Service => p.unsupported.push((
                n.path.clone(),
                "services need installation and administrator rights".into(),
            )),
            NodeType::Task => p
                .unsupported
                .push((n.path.clone(), "scheduled tasks are system-wide".into())),
        }
    }
    p
}

/// Result of [`build_portable`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortableBuild {
    /// Files copied.
    pub copied: usize,
    /// Planned files that no longer exist or are not regular files.
    pub missing: Vec<String>,
    /// Where the plan was written.
    pub plan_file: String,
}

/// Where a source file lands inside the portable folder: `Files/<drive>/<rest>`.
fn target_for(dest: &Path, src: &str) -> Option<PathBuf> {
    let mut t = dest.join("Files");
    let mut any = false;
    for c in Path::new(src).components() {
        match c {
            Component::Prefix(p) => {
                let d: String = p
                    .as_os_str()
                    .to_string_lossy()
                    .chars()
                    .filter(|c| c.is_alphanumeric())
                    .collect();
                t.push(if d.is_empty() { "drive".into() } else { d });
            }
            Component::Normal(n) => {
                t.push(n);
                any = true;
            }
            Component::RootDir | Component::CurDir => {}
            Component::ParentDir => return None,
        }
    }
    any.then_some(t)
}

/// Copy a plan's files into `dest` (created; must be new or empty) and write `portable-plan.json`
/// beside them listing the registry keys to replay and what could not be made portable.
/// Sources are only read.
pub fn build_portable(plan: &PortablePlan, dest: &Path) -> Result<PortableBuild> {
    let io = |e: std::io::Error| EngineError::Quarantine(format!("{}: {e}", dest.display()));
    if dest.exists() && std::fs::read_dir(dest).map_err(io)?.next().is_some() {
        return Err(EngineError::Quarantine(format!(
            "{} is not empty",
            dest.display()
        )));
    }
    std::fs::create_dir_all(dest).map_err(io)?;
    let (mut copied, mut missing) = (0, Vec::new());
    for f in &plan.copy_files {
        let src = Path::new(f);
        let Some(t) = target_for(dest, f).filter(|_| src.is_file()) else {
            missing.push(f.clone());
            continue;
        };
        if let Some(parent) = t.parent() {
            std::fs::create_dir_all(parent).map_err(io)?;
        }
        std::fs::copy(src, &t).map_err(io)?;
        copied += 1;
    }
    let plan_file = dest.join("portable-plan.json");
    std::fs::write(&plan_file, serde_json::to_string_pretty(plan)?).map_err(io)?;
    Ok(PortableBuild {
        copied,
        missing,
        plan_file: plan_file.to_string_lossy().into_owned(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn copies_files_keeps_sources_and_reports_missing() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("app/bin");
        std::fs::create_dir_all(&src).unwrap();
        let f = src.join("a.txt");
        std::fs::write(&f, b"hi").unwrap();
        let plan = PortablePlan {
            copy_files: vec![f.to_string_lossy().into_owned(), "/nope/x.txt".into()],
            export_registry: vec!["HKCU\\Software\\Foo".into()],
            unsupported: vec![("Svc".into(), "needs admin".into())],
        };
        let dest = d.path().join("out");
        let r = build_portable(&plan, &dest).unwrap();
        assert_eq!(r.copied, 1);
        assert_eq!(r.missing, vec!["/nope/x.txt".to_string()]);
        assert!(f.exists(), "source untouched");
        assert!(dest.join("portable-plan.json").is_file());
        let copied: Vec<_> = walk(&dest.join("Files"));
        assert_eq!(copied.len(), 1);
        assert!(build_portable(&plan, &dest).is_err(), "no overwrite");
    }

    #[test]
    fn parent_dir_paths_are_rejected() {
        assert!(target_for(Path::new("/o"), "/a/../b").is_none());
        assert!(target_for(Path::new("/o"), "/").is_none());
    }

    fn walk(p: &Path) -> Vec<PathBuf> {
        let mut v = vec![];
        for e in std::fs::read_dir(p).unwrap().flatten() {
            if e.path().is_dir() {
                v.extend(walk(&e.path()));
            } else {
                v.push(e.path());
            }
        }
        v
    }
}
