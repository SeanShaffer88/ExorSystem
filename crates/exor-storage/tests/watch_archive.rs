#![allow(clippy::unwrap_used, clippy::expect_used)]

use exor_storage::{
    ChangeKind, ChangeSource, NewLedgerEntry, ScopeCoverage, SessionStatus, StorageError, Store,
    WatchArchiveQuery, WatchCoverage, WatchSummary, latest_version,
};

fn save_comparison(
    store: &Store,
    name: &str,
    requested: usize,
    usable: usize,
    files: usize,
) -> i64 {
    let session_id = store.begin_session(name, None, None).unwrap();
    store
        .record_watch_comparison::<StorageError>(|_| {
            Ok(WatchSummary {
                session_id,
                coverage: WatchCoverage {
                    folders: ScopeCoverage { requested, usable },
                    ..WatchCoverage::default()
                },
                files_changed: files,
                services_changed: 0,
                tasks_changed: 0,
                registry_keys_changed: 0,
                warnings: vec!["Saved scope detail".into()],
            })
        })
        .unwrap();
    session_id
}

#[test]
fn archive_keeps_saved_zero_skipped_unknown_and_unfinished_sessions_distinct() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("archive.db");
    let writer = Store::open(&db).unwrap();
    let legacy = writer
        .begin_session("Legacy", Some("private.exe"), None)
        .unwrap();
    writer
        .end_session(legacy, SessionStatus::Completed)
        .unwrap();
    writer
        .append_ledger(&NewLedgerEntry {
            session_id: Some(legacy),
            node_id: None,
            path: "legacy.dat".into(),
            change_kind: ChangeKind::Created,
            old_hash: None,
            new_hash: None,
            source: ChangeSource::SnapshotDiff,
        })
        .unwrap();
    let cancelled = writer.begin_session("Cancelled", None, None).unwrap();
    writer
        .end_session(cancelled, SessionStatus::Failed)
        .unwrap();
    let interrupted = writer.begin_session("Interrupted", None, None).unwrap();
    assert_eq!(writer.recover_unfinished_sessions().unwrap(), 1);
    let zero = save_comparison(&writer, "Zero", 1, 1, 0);
    let skipped = save_comparison(&writer, "No pairs", 1, 0, 0);
    let partial = save_comparison(&writer, "Partial", 2, 1, 3);
    let active = writer.begin_session("Working", None, None).unwrap();

    let reader = Store::open_reader(&db).unwrap();
    let page = reader
        .watch_archive(10, WatchArchiveQuery::default())
        .unwrap();
    assert_eq!(page.next_before_id, None);
    let ids = page.rows.iter().map(|r| r.session_id).collect::<Vec<_>>();
    assert_eq!(
        ids,
        [
            active,
            partial,
            skipped,
            zero,
            interrupted,
            cancelled,
            legacy
        ]
    );
    let find = |id| page.rows.iter().find(|r| r.session_id == id).unwrap();
    assert_eq!(find(active).status, Some(SessionStatus::Monitoring));
    assert!(find(active).comparison.is_none());
    assert_eq!(
        find(partial).comparison.as_ref().unwrap().recorded_changes,
        Some(3)
    );
    assert_eq!(
        find(partial)
            .comparison
            .as_ref()
            .unwrap()
            .coverage
            .folders
            .usable,
        1
    );
    assert_eq!(find(skipped).status, Some(SessionStatus::Failed));
    assert_eq!(
        find(skipped).comparison.as_ref().unwrap().recorded_changes,
        None
    );
    assert_eq!(find(zero).status, Some(SessionStatus::Completed));
    assert_eq!(
        find(zero).comparison.as_ref().unwrap().recorded_changes,
        Some(0)
    );
    assert_eq!(find(interrupted).status, Some(SessionStatus::Failed));
    assert!(find(interrupted).ended_at.is_none());
    assert!(find(interrupted).comparison.is_none());
    assert_eq!(find(cancelled).status, Some(SessionStatus::Failed));
    assert!(find(cancelled).ended_at.is_some());
    assert!(find(cancelled).comparison.is_none());
    assert_eq!(find(legacy).status, Some(SessionStatus::Completed));
    assert!(
        find(legacy).comparison.is_none(),
        "a legacy ledger row is not saved coverage"
    );

    let serialized = serde_json::to_string(&page).unwrap();
    for unused in ["private.exe", "legacy.dat", "Saved scope detail"] {
        assert!(!serialized.contains(unused));
    }
    assert_eq!(writer.schema_version().unwrap(), latest_version());
    assert_eq!(writer.ledger_for_session(legacy).unwrap().len(), 1);
    assert!(writer.ledger_for_session(zero).unwrap().is_empty());
    assert!(writer.watch_comparison(legacy).unwrap().is_none());
}

#[test]
fn archive_search_is_literal_applies_before_paging_and_reaches_old_sessions() {
    let store = Store::open_in_memory().unwrap();
    let old = save_comparison(&store, "Editor%_ old comparison", 1, 1, 0);
    for i in 0..165 {
        let name = format!("Editor{i} recent");
        if i < 7 {
            save_comparison(&store, &name, 1, 1, 0);
        } else {
            store.begin_session(&name, None, None).unwrap();
        }
    }
    assert!(
        !store
            .recent_watch_runs(6)
            .unwrap()
            .iter()
            .any(|run| run.summary.session_id == old)
    );
    assert!(
        !store
            .recent_sessions(150)
            .unwrap()
            .iter()
            .any(|session| session.session_id == old)
    );
    let mid = store
        .begin_session("EDITOR%_ previous", None, None)
        .unwrap();
    store.end_session(mid, SessionStatus::Completed).unwrap();
    let recent = store.begin_session("Editor%_ now", None, None).unwrap();
    store.end_session(recent, SessionStatus::Completed).unwrap();
    let other_status = store.begin_session("Editor%_ failed", None, None).unwrap();
    store
        .end_session(other_status, SessionStatus::Failed)
        .unwrap();

    let query = WatchArchiveQuery {
        search: Some(" editor%_ "),
        status: Some(SessionStatus::Completed),
        ..WatchArchiveQuery::default()
    };
    let first = store.watch_archive(2, query).unwrap();
    assert_eq!(
        first.rows.iter().map(|r| r.session_id).collect::<Vec<_>>(),
        [recent, mid]
    );
    assert_eq!(first.next_before_id, Some(mid));
    let inserted = store
        .begin_session("editor%_ inserted later", None, None)
        .unwrap();
    store
        .end_session(inserted, SessionStatus::Completed)
        .unwrap();
    let second = store
        .watch_archive(
            2,
            WatchArchiveQuery {
                before_id: first.next_before_id,
                ..query
            },
        )
        .unwrap();
    assert_eq!(second.rows.len(), 1);
    assert_eq!(second.rows[0].session_id, old);
    assert_eq!(
        second.rows[0].comparison.as_ref().unwrap().recorded_changes,
        Some(0)
    );
    assert_eq!(second.next_before_id, None);

    let id = format!("#{old}");
    let exact = store
        .watch_archive(
            10,
            WatchArchiveQuery {
                search: Some(&id),
                ..query
            },
        )
        .unwrap();
    assert_eq!(exact.rows, second.rows);
    let none = store
        .watch_archive(
            10,
            WatchArchiveQuery {
                search: Some("%' OR 1=1 --"),
                ..query
            },
        )
        .unwrap();
    assert!(none.rows.is_empty());
    assert_eq!(none.next_before_id, None);
}

#[test]
fn archive_bounds_pages_and_rejects_invalid_search_inputs() {
    let store = Store::open_in_memory().unwrap();
    for _ in 0..51 {
        store.begin_session("Entry", None, None).unwrap();
    }
    let large = store
        .watch_archive(usize::MAX, WatchArchiveQuery::default())
        .unwrap();
    assert_eq!(large.rows.len(), 50);
    assert_eq!(large.next_before_id, Some(2));
    let tail = store
        .watch_archive(
            1,
            WatchArchiveQuery {
                before_id: large.next_before_id,
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(tail.rows.len(), 1);
    assert_eq!(tail.next_before_id, None);

    for search in [
        "#0",
        "#99999999999999999999999",
        "line\nfeed",
        &"x".repeat(241),
    ] {
        assert!(matches!(
            store.watch_archive(
                20,
                WatchArchiveQuery {
                    search: Some(search),
                    ..Default::default()
                }
            ),
            Err(StorageError::InvalidValue { .. })
        ));
    }
    assert!(matches!(
        store.watch_archive(
            20,
            WatchArchiveQuery {
                before_id: Some(-1),
                ..Default::default()
            }
        ),
        Err(StorageError::InvalidValue { .. })
    ));
}
