#![allow(clippy::unwrap_used)]

use exor_engine::execute::{purge_by_id, quarantine_decided, restore_by_id};
use exor_engine::leftovers::quarantine_leftovers;
use exor_engine::policy::{Decision, SimulationReport};
use exor_storage::{ChangeKind, NodeType, PolicyAction, QuarantineStatus, Store};
use std::path::Path;

fn report(paths: &[&Path]) -> SimulationReport {
    SimulationReport {
        decisions: paths
            .iter()
            .map(|path| Decision {
                path: path.to_str().unwrap().into(),
                node_type: NodeType::File,
                action: PolicyAction::Quarantine,
                rule: None,
                reason: "fixture".into(),
            })
            .collect(),
        keep: 0,
        quarantine: paths.len(),
        flag_for_review: 0,
    }
}

#[test]
fn both_entry_points_restore_originals_when_record_or_ledger_write_fails() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("fixture.db");
    let store = Store::open(&db).unwrap();
    let fault = rusqlite::Connection::open(db).unwrap();
    let session_id = store.begin_session("Fixture", None, None).unwrap();
    let app = d.path().join("apps").join("Fixture");
    let root = d.path().join("q");
    std::fs::create_dir_all(&app).unwrap();
    let first = app.join("a.txt");
    let second = app.join("b.txt");
    std::fs::write(&first, b"first file").unwrap();
    std::fs::write(&second, b"second file").unwrap();
    let plan = report(&[&first, &second]);
    for (table, condition) in [
        ("quarantine_manifests", "1"),
        ("change_ledger", "new.path LIKE '%b.txt'"),
    ] {
        fault
            .execute_batch(&format!(
                "CREATE TRIGGER reject_test_write BEFORE INSERT ON {table}
            WHEN {condition} BEGIN SELECT RAISE(ABORT, 'injected fixture write failure'); END;"
            ))
            .unwrap();
        let policy_error = quarantine_decided(&store, &root, session_id, &plan).unwrap_err();
        assert!(
            policy_error.to_string().contains("files put back"),
            "{policy_error}"
        );
        let leftovers_error =
            quarantine_leftovers(&store, &root, None, std::slice::from_ref(&app)).unwrap_err();
        assert!(
            leftovers_error.to_string().contains("files put back"),
            "{leftovers_error}"
        );
        assert_eq!(std::fs::read(&first).unwrap(), b"first file");
        assert_eq!(std::fs::read(&second).unwrap(), b"second file");
        assert!(store.quarantines().unwrap().is_empty());
        assert!(store.ledger_recent(10, None).unwrap().is_empty());
        fault
            .execute_batch("DROP TRIGGER reject_test_write")
            .unwrap();
    }
}

#[test]
fn restore_can_retry_ledger_commit_without_writing_files_twice() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("fixture.db");
    let store = Store::open(&db).unwrap();
    let fault = rusqlite::Connection::open(db).unwrap();
    let session_id = store.begin_session("Fixture", None, None).unwrap();
    let root = d.path().join("q");
    let file = d.path().join("test.txt");
    std::fs::write(&file, b"fixture").unwrap();
    let out = quarantine_decided(&store, &root, session_id, &report(&[&file])).unwrap();
    let id = &out.manifest.quarantine_id;
    fault.execute_batch("CREATE TRIGGER reject_restore BEFORE INSERT ON change_ledger
        WHEN new.change_kind = 'restored' BEGIN SELECT RAISE(ABORT, 'injected restore ledger failure'); END;").unwrap();
    assert!(restore_by_id(&store, &root, id).is_err());
    assert_eq!(std::fs::read(&file).unwrap(), b"fixture");
    assert_eq!(
        store.quarantine_record(id).unwrap().unwrap().status,
        QuarantineStatus::Quarantined
    );
    assert_eq!(store.ledger_for_session(session_id).unwrap().len(), 1);
    assert!(purge_by_id(&store, &root, id).is_err());
    fault.execute_batch("DROP TRIGGER reject_restore").unwrap();
    restore_by_id(&store, &root, id).unwrap();
    restore_by_id(&store, &root, id).unwrap();
    let kinds: Vec<_> = store
        .ledger_for_session(session_id)
        .unwrap()
        .into_iter()
        .map(|r| r.entry.change_kind)
        .collect();
    assert_eq!(kinds, [ChangeKind::Quarantined, ChangeKind::Restored]);
}
