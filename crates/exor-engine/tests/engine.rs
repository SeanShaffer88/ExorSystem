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
