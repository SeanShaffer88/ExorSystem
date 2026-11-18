#![allow(clippy::unwrap_used)]

use exor_engine::collectors::{ServiceInfo, TaskInfo};
use exor_engine::registry::{RegistryKeyState, RegistryScope, RegistryScopes};
use exor_engine::watch::{InstallWatch, ScopeCoverage, SystemProbe, WatchPlan};
use exor_engine::{Cancel, EngineError};
use exor_storage::{ChangeKind, SessionStatus, Store};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Probe(AtomicUsize);

impl SystemProbe for Probe {
    fn services(&self) -> Result<Vec<ServiceInfo>, String> {
        self.0.fetch_add(1, Ordering::Relaxed);
        Ok(Vec::new())
    }

    fn tasks(&self) -> Result<Vec<TaskInfo>, String> {
        let calls = self.0.fetch_add(1, Ordering::Relaxed);
        if calls == 0 {
            Ok(vec![TaskInfo {
                path: "\\Fixture".into(),
                command: None,
                author: None,
            }])
        } else {
            Err("access denied".into())
        }
    }

    fn registry_keys(&self, _: &RegistryScope) -> Result<Vec<RegistryKeyState>, String> {
        self.0.fetch_add(1, Ordering::Relaxed);
        Ok(Vec::new())
    }
}

fn write(path: &Path, content: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

#[tokio::test]
async fn file_only_watch_skips_incomplete_scopes_but_compares_missing_folders() {
    let dir = tempfile::tempdir().unwrap();
    let good = dir.path().join("good");
    let created = dir.path().join("created");
    let bad_before = dir.path().join("bad-before");
    let bad_after = dir.path().join("bad-after");
    let edited = good.join("edited.txt");
    let removed = good.join("removed.txt");
    let added = created.join("added.txt");
    write(&edited, b"before");
    write(&removed, b"retire");
    write(&bad_before, b"not a folder");
    write(&bad_after.join("uncompared.txt"), b"still present");
    let store = Arc::new(Mutex::new(Store::open_in_memory().unwrap()));
    let probe = Arc::new(Probe::default());
    let cancel = Cancel::new();
    let watch = InstallWatch::begin(
        Arc::clone(&store),
        "Files only",
        None,
        WatchPlan {
            folders: &[good.clone(), created, bad_before.clone(), bad_after.clone()],
            registry: &RegistryScopes::default(),
            services: false,
            tasks: false,
        },
        probe.clone(),
        &cancel,
    )
    .await
    .unwrap();
    assert_eq!(
        watch.coverage().folders,
        ScopeCoverage {
            requested: 4,
            usable: 3
        }
    );
    assert_eq!(watch.warnings().len(), 1);
    assert!(watch.warnings()[0].contains("bad-before"));
    write(&edited, b"longer after content");
    fs::remove_file(&removed).unwrap();
    write(&added, b"added");
    fs::remove_file(&bad_before).unwrap();
    write(&bad_before.join("also-uncompared.txt"), b"no baseline");
    fs::rename(&bad_after, dir.path().join("moved")).unwrap();
    write(&bad_after, b"no longer a folder");
    let summary = watch.finish(&cancel).await.unwrap();
    assert_eq!(probe.0.load(Ordering::Relaxed), 0);
    assert_eq!(
        summary.coverage.folders,
        ScopeCoverage {
            requested: 4,
            usable: 2
        }
    );
    assert_eq!(summary.coverage.registry, ScopeCoverage::default());
    assert_eq!(summary.coverage.services, ScopeCoverage::default());
    assert_eq!(summary.coverage.tasks, ScopeCoverage::default());
    assert_eq!(summary.warnings.len(), 2);
    assert!(summary.warnings[1].contains("bad-after"));
    assert_eq!(summary.files_changed, 3);
    let store = store.lock().unwrap();
    let ledger = store.ledger_for_session(summary.session_id).unwrap();
    assert_eq!(ledger.len(), 3);
    for (path, kind) in [
        (edited, ChangeKind::Modified),
        (removed, ChangeKind::Deleted),
        (added, ChangeKind::Created),
    ] {
        let event = &ledger
            .iter()
            .find(|row| Path::new(&row.entry.path) == path)
            .unwrap()
            .entry;
        assert_eq!(event.change_kind, kind);
        if kind == ChangeKind::Modified {
            assert!(event.old_hash.is_some() && event.new_hash.is_some());
            assert_ne!(event.old_hash, event.new_hash);
        }
    }
}

#[tokio::test]
async fn follow_up_denial_never_records_empty_tasks_as_removals() {
    let store = Arc::new(Mutex::new(Store::open_in_memory().unwrap()));
    let probe = Arc::new(Probe::default());
    let cancel = Cancel::new();
    let watch = InstallWatch::begin(
        Arc::clone(&store),
        "Tasks only",
        None,
        WatchPlan {
            folders: &[],
            registry: &RegistryScopes::default(),
            services: false,
            tasks: true,
        },
        probe.clone(),
        &cancel,
    )
    .await
    .unwrap();
    assert_eq!(
        watch.coverage().tasks,
        ScopeCoverage {
            requested: 1,
            usable: 1
        }
    );
    let summary = watch.finish(&cancel).await.unwrap();
    assert_eq!(
        summary.coverage.tasks,
        ScopeCoverage {
            requested: 1,
            usable: 0
        }
    );
    assert!(!summary.coverage.has_usable());
    assert!(summary.warnings[0].contains("after changes: access denied"));
    assert_eq!(probe.0.load(Ordering::Relaxed), 2);
    let store = store.lock().unwrap();
    assert_eq!(
        store.session(summary.session_id).unwrap().unwrap().status,
        Some(SessionStatus::Failed)
    );
    assert!(
        store
            .ledger_for_session(summary.session_id)
            .unwrap()
            .is_empty()
    );
}

#[tokio::test]
async fn no_input_is_rejected_and_an_entirely_unusable_baseline_is_closed() {
    let store = Arc::new(Mutex::new(Store::open_in_memory().unwrap()));
    let probe = Arc::new(Probe::default());
    let cancel = Cancel::new();
    let registry = RegistryScopes::default();
    let invalid = [PathBuf::from("ambiguous")];
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("not-a-folder.txt");
    write(&file, b"plain file");
    let inaccessible = [file];
    let begin = |folders| {
        InstallWatch::begin(
            Arc::clone(&store),
            "Unavailable",
            None,
            WatchPlan {
                folders,
                registry: &registry,
                services: false,
                tasks: false,
            },
            probe.clone(),
            &cancel,
        )
    };
    assert!(matches!(begin(&[]).await, Err(EngineError::Watch(_))));
    assert!(matches!(begin(&invalid).await, Err(EngineError::Watch(_))));
    assert!(store.lock().unwrap().sessions().unwrap().is_empty());
    let unusable = begin(&inaccessible).await;
    assert!(
        matches!(unusable, Err(EngineError::Watch(msg)) if msg.contains("No requested scopes could be captured"))
    );
    let sessions = store.lock().unwrap().sessions().unwrap();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].status, Some(SessionStatus::Failed));
}

#[cfg(windows)]
#[tokio::test]
async fn a_sharing_violation_prevents_a_partial_file_comparison() {
    use std::os::windows::fs::OpenOptionsExt;

    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().to_path_buf();
    let locked = root.join("locked.txt");
    write(&locked, b"before");
    let store = Arc::new(Mutex::new(Store::open_in_memory().unwrap()));
    let cancel = Cancel::new();
    let watch = InstallWatch::begin(
        Arc::clone(&store),
        "Sharing violation",
        None,
        WatchPlan {
            folders: &[root],
            registry: &RegistryScopes::default(),
            services: false,
            tasks: false,
        },
        Arc::new(Probe::default()),
        &cancel,
    )
    .await
    .unwrap();
    // A different length forces hashing instead of reusing the baseline hash.
    write(&locked, b"different after content");
    let handle = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(&locked)
        .unwrap();
    let summary = watch.finish(&cancel).await.unwrap();
    drop(handle);
    assert_eq!(
        summary.coverage.folders,
        ScopeCoverage {
            requested: 1,
            usable: 0
        }
    );
    assert_eq!(summary.warnings.len(), 1);
    assert!(summary.warnings[0].contains("locked.txt"));
    assert!(
        store
            .lock()
            .unwrap()
            .ledger_for_session(summary.session_id)
            .unwrap()
            .is_empty()
    );
}
