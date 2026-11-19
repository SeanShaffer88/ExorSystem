//! Read-only export of recorded files and a descriptive plan, not a runnable application.

use crate::error::{EngineError, Result, io_err};
use crate::publication::{Staging, sync_dir};
use crate::quarantine::{local_path, plain, plain_dirs};
use exor_storage::{Node, NodeType, Store};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

/// Plan for turning an installed app into a portable one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortablePlan {
    /// Recorded file identities, if required for this export. No registry data is applied.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_hashes: Option<BTreeMap<String, String>>,
    /// Files to copy beneath the portable app folder.
    pub copy_files: Vec<String>,
    /// Recorded registry paths for reference only, never applied.
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

/// Load a bounded plan without trusting current contents as recorded data.
pub fn plan_recorded(store: &Store, session_id: i64) -> Result<PortablePlan> {
    let mut nodes = Vec::new();
    loop {
        let page = store.session_nodes_page(session_id, None, nodes.len(), 100)?;
        if usize::try_from(page.total).unwrap_or(usize::MAX) > crate::cleanup::MAX_FINDINGS {
            return Err(EngineError::Quarantine(
                "Too many recorded items for one file export; use History for the paged evidence."
                    .into(),
            ));
        }
        if page.nodes.is_empty() {
            break;
        }
        nodes.extend(page.nodes);
    }
    let mut plan = plan_portable(&nodes);
    plan.file_hashes = Some(
        nodes
            .into_iter()
            .filter_map(|n| n.hash.map(|h| (n.path, h)))
            .collect(),
    );
    Ok(plan)
}

fn fill(plan: &PortablePlan, dest: &Path) -> Result<PortableBuild> {
    let (mut copied, mut missing) = (0, Vec::new());
    let mut remaining = 2_u64 * 1024 * 1024 * 1024;
    for f in &plan.copy_files {
        let src = Path::new(f);
        let Some(expected) = plan
            .file_hashes
            .as_ref()
            .map_or(Some(None), |hashes| hashes.get(f).map(Some))
        else {
            missing.push(f.clone());
            continue;
        };
        let Some(t) = target_for(dest, f).filter(|_| {
            local_path(src).is_ok()
                && src
                    .parent()
                    .is_some_and(|p| matches!(plain_dirs(p), Ok(true)))
        }) else {
            missing.push(f.clone());
            continue;
        };
        let Ok(Some(meta)) = plain(src, false) else {
            missing.push(f.clone());
            continue;
        };
        if meta.len() > remaining {
            missing.push(f.clone());
            continue;
        }
        remaining -= meta.len();
        #[cfg(windows)]
        let input = exor_fs::open_locked(src, false);
        #[cfg(not(windows))]
        let input = File::open(src);
        let Ok(input) = input else {
            missing.push(f.clone());
            continue;
        };
        let parent = t
            .parent()
            .ok_or_else(|| EngineError::Quarantine("Missing export directory".into()))?;
        fs::create_dir_all(parent).map_err(io_err(parent))?;
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&t)
            .map_err(io_err(&t))?;
        let n = std::io::copy(&mut input.take(meta.len() + 1), &mut output).map_err(io_err(&t))?;
        output.sync_all().map_err(io_err(&t))?;
        drop(output);
        if n != meta.len()
            || expected
                .is_some_and(|h| crate::hashing::hash_file_blocking(&t).as_ref().ok() != Some(h))
        {
            fs::remove_file(&t).map_err(io_err(&t))?;
            missing.push(f.clone());
            continue;
        }
        sync_dir(parent)?;
        copied += 1;
    }
    let plan_file = dest.join("portable-plan.json");
    let mut file = File::create(&plan_file).map_err(io_err(&plan_file))?;
    let payload = serde_json::json!({ "format": "exorsystem.recorded-files", "version": 1, "plan": plan,
        "not_copied": missing, "copied": copied, "note": "This is not a runnable application. No registry entries were exported or replayed." });
    file.write_all(&serde_json::to_vec_pretty(&payload)?)
        .and_then(|()| file.sync_all())
        .map_err(io_err(&plan_file))?;
    Ok(PortableBuild {
        copied,
        missing,
        plan_file: String::new(),
    })
}

/// Copy into a private staging directory and publish complete files and plan under a new name.
pub fn export_recorded(
    plan: &PortablePlan,
    directory: &Path,
    label: &str,
) -> Result<(PathBuf, PortableBuild)> {
    let stage = Staging::new(directory)?;
    let mut result = fill(plan, stage.path())?;
    let folder = stage.publish_directory(label)?;
    result.plan_file = folder
        .join("portable-plan.json")
        .to_string_lossy()
        .into_owned();
    Ok((folder, result))
}

/// Export to an exact, new destination. Existing directories are never intentionally reused.
pub fn build_portable(plan: &PortablePlan, dest: &Path) -> Result<PortableBuild> {
    let parent = dest
        .parent()
        .ok_or_else(|| EngineError::Quarantine("Missing output directory".into()))?;
    let stage = Staging::new(parent)?;
    let mut result = fill(plan, stage.path())?;
    stage.publish_as(dest)?;
    result.plan_file = dest
        .join("portable-plan.json")
        .to_string_lossy()
        .into_owned();
    Ok(result)
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
            ..Default::default()
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
