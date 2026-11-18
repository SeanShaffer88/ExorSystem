#![allow(clippy::unwrap_used, clippy::expect_used)]

use exor_engine::events::{EventSource, FsReason, MockEventSource, RawEvent};
use exor_engine::monitor::{ManifestEntry, SessionMonitor};
use exor_engine::noise::NoiseFilter;
use exor_engine::policy::{candidates_for_session, simulate};
use exor_engine::snapshot::{CaptureOptions, capture};
use exor_engine::trie::{ChangeType, diff};
use exor_engine::{Cancel, EngineError, report, templates};
use exor_storage::{
    Evidence, NewNode, NodeType, PolicyAction, RelationshipType, SessionStatus, Store,
};
use std::sync::{Arc, Mutex};

fn write(p: &std::path::Path, data: &[u8]) {
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, data).unwrap();
}

#[tokio::test]
async fn snapshot_diff_finds_add_modify_remove() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    write(&root.join("keep.txt"), b"same");
    write(&root.join("sub/mod.txt"), b"v1");
    write(&root.join("sub/gone.txt"), b"bye");
    let cancel = Cancel::new();
    let opts = CaptureOptions::default();
    let before = capture(root, &opts, None, &cancel).await.unwrap();

    write(&root.join("sub/mod.txt"), b"v2-changed");
    std::fs::remove_file(root.join("sub/gone.txt")).unwrap();
    write(&root.join("new/dir/added.txt"), b"hello");
    let after = capture(root, &opts, Some(&before.trie), &cancel)
        .await
        .unwrap();

    let changes = diff(&before.trie, &after.trie);
    let find = |suffix: &str| {
        changes
            .iter()
            .find(|c| c.path.replace('\\', "/").ends_with(suffix))
            .unwrap()
    };
    assert_eq!(find("sub/mod.txt").change, ChangeType::Modified);
    assert_eq!(find("sub/gone.txt").change, ChangeType::Removed);
    assert_eq!(find("new/dir/added.txt").change, ChangeType::Added);
    assert!(changes.iter().all(|c| !c.path.ends_with("keep.txt")));
    assert_ne!(before.trie.root_digest(), after.trie.root_digest());
    // identical snapshots: no changes, equal digests
    let again = capture(root, &opts, Some(&after.trie), &cancel)
        .await
        .unwrap();
    assert!(diff(&after.trie, &again.trie).is_empty());
    assert_eq!(after.trie.root_digest(), again.trie.root_digest());
}

#[tokio::test]
async fn incremental_capture_reuses_hashes_and_exclusions_work() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    write(&root.join("a.bin"), b"abc");
    write(&root.join("cache/skip.bin"), b"zzz");
    let mut opts = CaptureOptions::default();
    opts.walk.exclude_contains = vec!["\\Cache\\".into()];
    let s = capture(root, &opts, None, &Cancel::new()).await.unwrap();
    assert!(
        s.trie
            .states()
            .iter()
            .all(|st| !st.path.contains("skip.bin"))
    );
    let a = s
        .trie
        .states()
        .into_iter()
        .find(|st| st.path.ends_with("a.bin"))
        .unwrap()
        .clone();
    assert_eq!(
        a.hash.as_deref(),
        Some(exor_engine::hashing::hash_bytes(b"abc").as_str())
    );
}

#[tokio::test]
async fn cancel_aborts_walk() {
    let d = tempfile::tempdir().unwrap();
    write(&d.path().join("x/y.txt"), b"1");
    let c = Cancel::new();
    c.cancel();
    let r = capture(d.path(), &CaptureOptions::default(), None, &c).await;
    assert!(matches!(r, Err(EngineError::Cancelled)));
}

#[tokio::test]
async fn monitor_pipeline_filters_noise_and_attributes() {
    let store = Arc::new(Mutex::new(Store::open_in_memory().unwrap()));
    let mon = SessionMonitor::start(
        Arc::clone(&store),
        "Foo",
        Some("C:\\dl\\foo-setup.exe"),
        Some("nsis"),
    )
    .unwrap();
    let events = exor_engine::events::generate(42, 400, "C:\\Program Files\\Foo");
    let rx = Box::new(MockEventSource::scripted(events)).start();
    let stats = mon
        .run(rx, NoiseFilter::with_defaults(), &Cancel::new())
        .await
        .unwrap();
    assert_eq!(stats.received, 400);
    assert_eq!(stats.received, stats.kept + stats.dropped);
    assert!(stats.dropped > 50, "noise must be filtered: {stats:?}");
    assert!(stats.kept > 100);
    mon.finish(SessionStatus::Completed).unwrap();

    let s = store.lock().unwrap();
    let nodes = s.nodes_for_session(mon.session_id()).unwrap();
    assert!(
        nodes
            .iter()
            .all(|n| !n.path.to_lowercase().contains("chrome")
                && !n.path.to_lowercase().contains("defender"))
    );
    let sample = nodes
        .iter()
        .find(|n| n.path.starts_with("C:\\Program Files\\Foo"))
        .unwrap();
    let prov = s.provenance_for_path(&sample.path).unwrap().unwrap();
    assert_eq!(prov.confidence, 0.7, "event-attributed = temporal");
    assert_eq!(prov.session.unwrap().app_name, "Foo");
    assert!(!s.ledger_for_session(mon.session_id()).unwrap().is_empty());
    let text = report::describe_provenance(&s.provenance_for_path(&sample.path).unwrap().unwrap());
    assert!(text.contains("very likely"));
}

#[tokio::test]
async fn confidence_ladder_heuristic_temporal_manifest() {
    let store = Arc::new(Mutex::new(Store::open_in_memory().unwrap()));
    let mon =
        SessionMonitor::start(Arc::clone(&store), "Bar", Some("C:\\bar-setup.exe"), None).unwrap();
    let d = tempfile::tempdir().unwrap();
    let root = d.path();
    let cancel = Cancel::new();
    let before = capture(root, &CaptureOptions::default(), None, &cancel)
        .await
        .unwrap();
    write(&root.join("bar.exe"), b"exe");
    let after = capture(
        root,
        &CaptureOptions::default(),
        Some(&before.trie),
        &cancel,
    )
    .await
    .unwrap();
    let changes = diff(&before.trie, &after.trie);
    assert_eq!(mon.attribute_diff(&changes).unwrap(), 1);
    let path = after
        .trie
        .states()
        .into_iter()
        .find(|s| s.path.ends_with("bar.exe"))
        .unwrap()
        .path
        .clone();
    let conf = |store: &Arc<Mutex<Store>>| {
        store
            .lock()
            .unwrap()
            .provenance_for_path(&path)
            .unwrap()
            .unwrap()
            .confidence
    };
    assert_eq!(conf(&store), 0.3);

    let ev = RawEvent::Fs {
        usn: 1,
        path: path.clone(),
        reason: FsReason::Create,
        process: None,
        ts_ms: 1,
    };
    let rx = Box::new(MockEventSource::scripted(vec![ev])).start();
    mon.run(rx, NoiseFilter::empty(), &cancel).await.unwrap();
    assert_eq!(conf(&store), 0.7);

    mon.ingest_manifest(&[ManifestEntry {
        path: path.clone(),
        node_type: NodeType::File,
        hash: None,
    }])
    .unwrap();
    assert_eq!(conf(&store), 1.0);
}

#[tokio::test]
async fn monitor_run_is_cancellable_while_idle() {
    let store = Arc::new(Mutex::new(Store::open_in_memory().unwrap()));
    let mon = SessionMonitor::start(store, "Idle", None, None).unwrap();
    let (_tx, rx) = tokio::sync::mpsc::channel::<RawEvent>(1);
    let c = Cancel::new();
    let c2 = c.clone();
    tokio::spawn(async move {
        c2.cancel();
    });
    let r = mon.run(rx, NoiseFilter::empty(), &c).await;
    assert!(matches!(r, Err(EngineError::Cancelled)));
}

#[test]
fn policy_simulation_with_templates_and_precedence() {
    let store = Store::open_in_memory().unwrap();
    let sess = store
        .begin_session("Foo", Some("C:\\foo-setup.exe"), None)
        .unwrap();
    let other = store.begin_session("Other", None, None).unwrap();
    let inst = store
        .upsert_node(&NewNode {
            node_type: NodeType::File,
            path: "C:\\foo-setup.exe".into(),
            hash: None,
            acl_fingerprint: None,
            first_detected_in: Some(sess),
        })
        .unwrap();
    let mk = |p: &str, s: i64| NewNode {
        node_type: NodeType::File,
        path: p.into(),
        hash: None,
        acl_fingerprint: None,
        first_detected_in: Some(s),
    };
    let confirmed = store
        .upsert_node(&mk("C:\\Program Files\\Foo\\foo.exe", sess))
        .unwrap();
    let inferred = store
        .upsert_node(&mk("C:\\Program Files\\Foo\\b.dll", sess))
        .unwrap();
    let guess = store
        .upsert_node(&mk("C:\\Program Files\\Foo\\c.dat", sess))
        .unwrap();
    let shared = store
        .upsert_node(&mk("C:\\Program Files\\Common\\vcrt.dll", sess))
        .unwrap();
    let other_app = store
        .upsert_node(&mk("C:\\Program Files\\Other\\o.exe", other))
        .unwrap();
    store
        .add_edge(
            inst,
            confirmed,
            RelationshipType::CreatedBy,
            Evidence::Manifest,
        )
        .unwrap();
    store
        .add_edge(
            inst,
            inferred,
            RelationshipType::CreatedBy,
            Evidence::Temporal,
        )
        .unwrap();
    store
        .add_edge(
            inst,
            guess,
            RelationshipType::CreatedBy,
            Evidence::Heuristic,
        )
        .unwrap();
    store
        .add_edge(
            inst,
            shared,
            RelationshipType::CreatedBy,
            Evidence::Manifest,
        )
        .unwrap();
    store
        .add_edge(
            shared,
            other_app,
            RelationshipType::DependsOn,
            Evidence::Temporal,
        )
        .unwrap();

    templates::install(&store, "standard").unwrap();
    let cands = candidates_for_session(&store, sess).unwrap();
    let report = simulate(&store.rules().unwrap(), &cands).unwrap();
    let action = |suffix: &str| {
        report
            .decisions
            .iter()
            .find(|d| d.path.ends_with(suffix))
            .unwrap()
            .action
    };
    assert_eq!(action("foo.exe"), PolicyAction::Quarantine);
    assert_eq!(action("b.dll"), PolicyAction::Quarantine);
    assert_eq!(action("c.dat"), PolicyAction::FlagForReview);
    assert_eq!(
        action("vcrt.dll"),
        PolicyAction::Keep,
        "keep beats quarantine for shared components"
    );
    assert_eq!(
        report.keep + report.quarantine + report.flag_for_review,
        cands.len()
    );
    let text = report::summarize_simulation(&report);
    assert!(text.contains("Nothing was changed"));

    // dry run: simulation never touches nodes
    assert_eq!(store.nodes_for_session(sess).unwrap().len(), 5);
    assert!(templates::install(&store, "nope").is_err());
}

#[test]
fn no_rules_means_review() {
    let store = Store::open_in_memory().unwrap();
    let s = store.begin_session("A", None, None).unwrap();
    store
        .upsert_node(&NewNode {
            node_type: NodeType::File,
            path: "x".into(),
            hash: None,
            acl_fingerprint: None,
            first_detected_in: Some(s),
        })
        .unwrap();
    let r = simulate(&[], &candidates_for_session(&store, s).unwrap()).unwrap();
    assert_eq!(r.flag_for_review, 1);
}

#[tokio::test]
async fn capture_streams_progress_and_ends_done() {
    use exor_engine::snapshot::{Phase, Progress, capture_with_progress};
    let d = tempfile::tempdir().unwrap();
    for i in 0..30 {
        write(&d.path().join(format!("f{i}.txt")), b"data");
    }
    let seen: Mutex<Vec<Progress>> = Mutex::new(Vec::new());
    let snap = capture_with_progress(
        d.path(),
        &CaptureOptions::default(),
        None,
        &Cancel::new(),
        &|p| {
            seen.lock().unwrap().push(p);
        },
    )
    .await
    .unwrap();
    let seen = seen.into_inner().unwrap();
    assert_eq!(seen.first().unwrap().phase, Phase::Walking);
    let last = seen.last().unwrap();
    assert_eq!(last.phase, Phase::Done);
    assert_eq!(
        (last.files_seen, last.files_hashed, last.bytes_hashed),
        (30, 30, 120)
    );
    assert!(
        seen.windows(2)
            .all(|w| w[0].files_hashed <= w[1].files_hashed),
        "monotonic"
    );
    assert_eq!(snap.trie.len(), 30);
}

#[tokio::test]
async fn services_and_tasks_attributed_with_dependency_link() {
    use exor_engine::collectors::{ServiceInfo, TaskInfo, diff_services, diff_tasks};
    let store = Arc::new(Mutex::new(Store::open_in_memory().unwrap()));
    let mon =
        SessionMonitor::start(Arc::clone(&store), "Foo", Some("C:\\foo-setup.exe"), None).unwrap();
    mon.ingest_manifest(&[ManifestEntry {
        path: "C:\\Program Files\\Foo\\svc.exe".into(),
        node_type: NodeType::File,
        hash: None,
    }])
    .unwrap();
    let svc = ServiceInfo {
        name: "FooSvc".into(),
        display_name: "Foo".into(),
        binary_path: "\"C:\\Program Files\\Foo\\svc.exe\" -run".into(),
        start_type: "auto".into(),
    };
    let task = TaskInfo {
        path: "\\Foo\\Update".into(),
        command: Some("C:\\Program Files\\Foo\\svc.exe /update".into()),
        author: None,
    };
    assert_eq!(
        mon.attribute_services(&diff_services(&[], &[svc])).unwrap(),
        1
    );
    assert_eq!(mon.attribute_tasks(&diff_tasks(&[], &[task])).unwrap(), 1);
    let s = store.lock().unwrap();
    let exe = s
        .node_by_path("C:\\Program Files\\Foo\\svc.exe")
        .unwrap()
        .unwrap();
    let impact = s.shared_component_impact(exe.node_id).unwrap();
    let kinds: Vec<_> = impact.dependents.iter().map(|n| n.node_type).collect();
    assert!(kinds.contains(&NodeType::Service) && kinds.contains(&NodeType::Task));
    let p = s.provenance_for_path("service:FooSvc").unwrap().unwrap();
    assert_eq!(p.confidence, 0.3);
}

#[test]
fn execute_quarantine_restore_purge_with_ledger() {
    use exor_engine::execute::{
        purge_by_id, quarantine_decided, recover_interrupted, restore_by_id,
    };
    let d = tempfile::tempdir().unwrap();
    let store = Store::open_in_memory().unwrap();
    let sess = store.begin_session("Foo", None, None).unwrap();
    let f1 = d.path().join("a.dll");
    let f2 = d.path().join("b.dll");
    write(&f1, b"one");
    write(&f2, b"two");
    let report = exor_engine::policy::SimulationReport {
        decisions: vec![
            exor_engine::policy::Decision {
                path: f1.to_string_lossy().into(),
                node_type: NodeType::File,
                action: PolicyAction::Quarantine,
                rule: None,
                reason: "t".into(),
            },
            exor_engine::policy::Decision {
                path: f2.to_string_lossy().into(),
                node_type: NodeType::File,
                action: PolicyAction::Keep,
                rule: None,
                reason: "t".into(),
            },
            exor_engine::policy::Decision {
                path: f2.to_string_lossy().into(),
                node_type: NodeType::RegistryKey,
                action: PolicyAction::Quarantine,
                rule: None,
                reason: "t".into(),
            },
        ],
        keep: 1,
        quarantine: 2,
        flag_for_review: 0,
    };
    let q = d.path().join("q");
    let out = quarantine_decided(&store, &q, sess, &report).unwrap();
    assert!(!f1.exists() && f2.exists());
    assert_eq!(out.skipped, vec![f2.to_string_lossy().into_owned()]);
    let id = out.manifest.quarantine_id.clone();
    restore_by_id(&store, &id, false).unwrap();
    assert_eq!(std::fs::read(&f1).unwrap(), b"one");
    let kinds: Vec<_> = store
        .ledger_for_session(sess)
        .unwrap()
        .into_iter()
        .map(|e| e.entry.change_kind)
        .collect();
    assert_eq!(
        kinds,
        vec![
            exor_storage::ChangeKind::Quarantined,
            exor_storage::ChangeKind::Restored
        ]
    );
    // second round, purge
    let out2 = quarantine_decided(&store, &q, sess, &report).unwrap();
    purge_by_id(&store, &out2.manifest.quarantine_id).unwrap();
    assert!(!f1.exists());
    assert_eq!(store.quarantines().unwrap().len(), 2);
    assert!(recover_interrupted(&q).unwrap().is_empty());
    assert!(restore_by_id(&store, "nope", false).is_err());
    // empty plan is an error, not a silent no-op
    let empty = exor_engine::policy::SimulationReport {
        decisions: vec![],
        keep: 0,
        quarantine: 0,
        flag_for_review: 0,
    };
    assert!(quarantine_decided(&store, &q, sess, &empty).is_err());
}

#[tokio::test]
async fn install_watch_records_files_and_services_with_registry_opted_out() {
    use exor_engine::collectors::{ServiceInfo, TaskInfo};
    use exor_engine::registry::{RegistryKeyState, RegistryScope, RegistryScopes};
    use exor_engine::watch::{InstallWatch, SystemProbe};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Mock(AtomicUsize);
    impl SystemProbe for Mock {
        fn services(&self) -> Result<Vec<ServiceInfo>, String> {
            let n = self.0.fetch_add(1, Ordering::SeqCst);
            Ok(if n == 0 {
                vec![]
            } else {
                vec![ServiceInfo {
                    name: "FooSvc".into(),
                    display_name: "Foo".into(),
                    binary_path: "C:\\Foo\\svc.exe".into(),
                    start_type: "auto".into(),
                }]
            })
        }
        fn tasks(&self) -> Result<Vec<TaskInfo>, String> {
            Err("denied".into())
        }
        fn registry_keys(&self, _: &RegistryScope) -> Result<Vec<RegistryKeyState>, String> {
            Err("must not be called without any scopes".into())
        }
    }

    let dir = tempfile::tempdir().unwrap();
    write(&dir.path().join("keep.txt"), b"old");
    let store = Arc::new(Mutex::new(Store::open_in_memory().unwrap()));
    let cancel = exor_engine::Cancel::new();
    let w = InstallWatch::begin(
        Arc::clone(&store),
        "Foo",
        None,
        &[dir.path().to_path_buf()],
        &RegistryScopes::default(),
        Arc::new(Mock(AtomicUsize::new(0))),
        &cancel,
    )
    .await
    .unwrap();
    let sid = w.session_id();
    write(&dir.path().join("foo.dll"), b"new");
    let s = w.finish(&cancel).await.unwrap();
    assert_eq!(s.session_id, sid);
    assert_eq!(
        (s.registry_keys_changed, s.registry_scopes_compared),
        (0, 0)
    );
    assert_eq!(
        (s.files_changed, s.services_changed, s.tasks_changed),
        (1, 1, 0)
    );
    assert_eq!(
        s.warnings.len(),
        1,
        "baseline probe failure warns once and skips the compare: {:?}",
        s.warnings
    );
    assert!(s.warnings[0].contains("Scheduled tasks"));
    let st = store.lock().unwrap();
    let dll = dir.path().join("foo.dll").to_string_lossy().into_owned();
    assert!(st.provenance_for_path(&dll).unwrap().is_some());
    assert!(st.provenance_for_path("service:FooSvc").unwrap().is_some());
    let sess = st
        .sessions()
        .unwrap()
        .into_iter()
        .find(|x| x.session_id == sid)
        .unwrap();
    assert_eq!(sess.status, Some(exor_storage::SessionStatus::Completed));
}

#[tokio::test]
async fn install_watch_abandon_and_failed_baseline_close_the_session() {
    use exor_engine::registry::RegistryScopes;
    use exor_engine::watch::{InstallWatch, NullProbe};
    let store = Arc::new(Mutex::new(Store::open_in_memory().unwrap()));
    let cancel = exor_engine::Cancel::new();
    let dir = tempfile::tempdir().unwrap();
    // a watch folder the installer will create later is fine: it baselines as empty
    let later = dir.path().join("Foo");
    InstallWatch::begin(
        Arc::clone(&store),
        "Later",
        None,
        std::slice::from_ref(&later),
        &RegistryScopes::default(),
        Arc::new(NullProbe),
        &cancel,
    )
    .await
    .unwrap()
    .abandon()
    .unwrap();
    // a cancelled baseline fails and closes its session
    let stop = exor_engine::Cancel::new();
    stop.cancel();
    assert!(
        InstallWatch::begin(
            Arc::clone(&store),
            "Bad",
            None,
            &[dir.path().to_path_buf()],
            &RegistryScopes::default(),
            Arc::new(NullProbe),
            &stop
        )
        .await
        .is_err()
    );
    let w = InstallWatch::begin(
        Arc::clone(&store),
        "Ok",
        None,
        &[],
        &RegistryScopes::default(),
        Arc::new(NullProbe),
        &cancel,
    )
    .await
    .unwrap();
    let sid = w.session_id();
    w.abandon().unwrap();
    let st = store.lock().unwrap();
    let all = st.sessions().unwrap();
    assert!(
        all.iter()
            .all(|s| s.status == Some(exor_storage::SessionStatus::Failed)),
        "{all:?}"
    );
    assert!(all.iter().any(|s| s.session_id == sid));
}
