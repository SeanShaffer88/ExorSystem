#![allow(clippy::unwrap_used)]

use exor_engine::execute::{purge_by_id, quarantine_decided, restore_by_id};
use exor_engine::leftovers::quarantine_leftovers;
use exor_engine::policy::{Decision, SimulationReport};
use exor_engine::{execute, quarantine};
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

#[test]
fn preview_pages_read_only_the_saved_copy_and_do_not_authorize_restore() {
    let d = tempfile::tempdir().unwrap();
    let store = Store::open_in_memory().unwrap();
    let session_id = store.begin_session("Preview fixture", None, None).unwrap();
    let root = d.path().join("q");
    let originals: Vec<_> = (0..26)
        .map(|i| {
            let path = d.path().join(format!("sample-{i:02}.bin"));
            std::fs::write(&path, vec![42_u8; i + 1]).unwrap();
            path
        })
        .collect();
    let paths: Vec<_> = originals.iter().map(|p| p.as_path()).collect();
    let out = quarantine_decided(&store, &root, session_id, &report(&paths)).unwrap();
    let id = &out.manifest.quarantine_id;
    let manifest = root.join(id).join("manifest.json");
    let hidden = root.join(id).join("manifest.temp");
    std::fs::rename(&manifest, &hidden).unwrap();
    let first = exor_engine::execute::preview_batch(&store, id, 0).unwrap();
    let last = exor_engine::execute::preview_batch(&store, id, first.next_offset.unwrap()).unwrap();
    assert_eq!(first.status, "quarantined");
    assert_eq!(first.total_files, 26);
    assert_eq!(first.created_unix_ms as u64, out.manifest.created_unix_ms);
    assert_eq!(first.files.len(), 24);
    assert_eq!(last.files.len(), 2);
    assert_eq!(Path::new(&last.files[1].original_path), paths[25]);
    assert_eq!(last.files[1].size, 26);
    assert_eq!(last.next_offset, None);
    assert!(exor_engine::execute::preview_batch(&store, "../unknown", 0).is_err());
    assert!(
        restore_by_id(&store, &root, id).is_err(),
        "restoring must read the disk manifest"
    );
    assert!(!manifest.exists());
    assert!(originals.iter().all(|path| !path.exists()));
    assert_eq!(store.ledger_for_session(session_id).unwrap().len(), 26);

    std::fs::rename(&hidden, &manifest).unwrap();
    std::fs::write(paths[25], b"replacement").unwrap();
    assert!(restore_by_id(&store, &root, id).is_err());
    assert_eq!(std::fs::read(paths[25]).unwrap(), b"replacement");
    assert_eq!(store.ledger_for_session(session_id).unwrap().len(), 26);
    std::fs::remove_file(paths[25]).unwrap();
    restore_by_id(&store, &root, id).unwrap();
    let saved = exor_engine::execute::preview_batch(&store, id, 24).unwrap();
    assert_eq!(saved.status, "restored");
    assert_eq!(saved.files[1].original_path, last.files[1].original_path);
}

#[test]
fn reviewed_plan_is_exact_and_rejects_a_replaced_file_before_any_move() {
    use exor_engine::{Cancel, cleanup};
    let d = tempfile::tempdir().unwrap();
    let app = d.path().join("apps").join("example");
    let profile = d.path().join("profile");
    std::fs::create_dir_all(&app).unwrap();
    std::fs::create_dir_all(&profile).unwrap();
    let a = app.join("a.dll");
    let b = app.join("b.dll");
    let added = app.join("added.dll");
    let outside = d.path().join("outside.dll");
    std::fs::write(&a, b"A").unwrap();
    std::fs::write(&b, b"B").unwrap();
    std::fs::write(&outside, b"Z").unwrap();
    let cancel = Cancel::new();
    assert!(
        quarantine::review_file(
            &outside,
            std::slice::from_ref(&app),
            &profile,
            None,
            &cancel
        )
        .is_err()
    );
    assert!(
        cleanup::prepare_leftovers(&[d.path().to_path_buf()], &profile, None, &cancel).is_err()
    );
    let plan =
        cleanup::prepare_leftovers(std::slice::from_ref(&app), &profile, None, &cancel).unwrap();
    assert_eq!((plan.files.len(), plan.bytes), (2, 2));
    std::fs::write(&added, b"not reviewed").unwrap();
    std::fs::write(&b, b"C").unwrap();
    let store = Store::open_in_memory().unwrap();
    let root = profile.join("quarantine");
    assert!(execute::record_reviewed(&store, &root, None, &plan.files, &cancel).is_err());
    assert!(a.exists() && b.exists() && added.exists() && outside.exists());
    assert!(store.quarantines().unwrap().is_empty());
    // Holding on to the original makes inode/file-ID reuse impossible. Matching bytes do not
    // make a substituted object eligible, even if timestamps happened to match.
    std::fs::rename(&b, app.join("old-object.dll")).unwrap();
    std::fs::write(&b, b"B").unwrap();
    assert!(execute::record_reviewed(&store, &root, None, &plan.files, &cancel).is_err());
    assert!(a.exists() && b.exists());
    let single = quarantine::review_file(&a, &[app], &profile, None, &cancel).unwrap();
    let (manifest, _) = execute::record_reviewed(&store, &root, None, &[single], &cancel).unwrap();
    assert_eq!(manifest.items.len(), 1);
    assert!(!a.exists() && b.exists() && added.exists() && outside.exists());
}

#[test]
fn verified_profile_copies_include_held_bytes_and_reject_corruption() {
    use exor_engine::{Cancel, backup};
    let d = tempfile::tempdir().unwrap();
    let profile = d.path().join("profile");
    let app = d.path().join("app");
    std::fs::create_dir(&profile).unwrap();
    std::fs::create_dir(&app).unwrap();
    let f = app.join("a.dll");
    std::fs::write(&f, b"held file").unwrap();
    let store = Store::open(&profile.join("exor.db")).unwrap();
    let cancel = Cancel::new();
    let item = quarantine::review_file(&f, &[app], &profile, None, &cancel).unwrap();
    let root = profile.join("quarantine");
    let (batch, _) = execute::record_reviewed(&store, &root, None, &[item], &cancel).unwrap();
    let saved = backup::create(&store, &profile, d.path()).unwrap();
    assert_eq!(saved.files, 3);
    let archived = std::path::Path::new(&saved.path);
    backup::verify_archived_batch(&store, &root, archived, &batch.quarantine_id).unwrap();
    let copy = backup::restore_copy(&profile, archived, d.path()).unwrap();
    assert_ne!(copy.path, saved.path);
    assert_eq!(
        Store::open_reader(&std::path::Path::new(&copy.path).join("exor.db"))
            .unwrap()
            .quarantines()
            .unwrap()
            .len(),
        1
    );
    let id = &batch.quarantine_id;
    let relative = std::path::Path::new(id).join("blobs").join("0");
    let local_blob = root.join(&relative);
    let saved_blob = archived.join("quarantine").join(&relative);
    std::fs::write(&local_blob, b"altered!!").unwrap();
    assert!(execute::rotate_by_id(&store, &root, id, archived).is_err());
    assert!(local_blob.exists() && !f.exists());
    std::fs::write(&local_blob, b"held file").unwrap();
    std::fs::write(&saved_blob, b"altered!!").unwrap();
    assert!(backup::restore_copy(&profile, archived, d.path()).is_err());
    assert!(execute::rotate_by_id(&store, &root, id, archived).is_err());
    assert!(local_blob.exists());
    std::fs::write(&saved_blob, b"held file").unwrap();
    let sidecar = archived.join("exor.db-wal");
    std::fs::write(&sidecar, b"unindexed").unwrap();
    assert!(execute::rotate_by_id(&store, &root, id, archived).is_err());
    std::fs::remove_file(sidecar).unwrap();
    execute::rotate_by_id(&store, &root, id, archived).unwrap();
    assert!(!local_blob.exists() && !f.exists());
    assert!(saved_blob.exists());
    assert_eq!(
        store.quarantine_record(id).unwrap().unwrap().status,
        QuarantineStatus::Purged
    );
    assert!(!store.ledger_recent(10, None).unwrap().is_empty());
}

#[test]
fn restart_finalizes_disk_only_restore_and_never_resumes_purge() {
    use exor_engine::Cancel;
    use exor_storage::{QuarantineOperation, QuarantineStatus};
    let d = tempfile::tempdir().unwrap();
    let app = d.path().join("app");
    let profile = d.path().join("profile");
    std::fs::create_dir(&app).unwrap();
    std::fs::create_dir(&profile).unwrap();
    let root = profile.join("quarantine");
    let db = profile.join("exor.db");
    let store = Store::open(&db).unwrap();
    let cancel = Cancel::new();
    let path = app.join("file");
    std::fs::write(&path, "file").unwrap();
    let item = || {
        quarantine::review_file(&path, std::slice::from_ref(&app), &profile, None, &cancel).unwrap()
    };
    let (restored, manifest) =
        execute::record_reviewed(&store, &root, None, &[item()], &cancel).unwrap();
    store
        .begin_quarantine_operation(&restored.quarantine_id, QuarantineOperation::Restoring)
        .unwrap();
    quarantine::restore(&manifest).unwrap(); // simulated interruption before updating SQLite
    let one = app.join("other");
    let two = app.join("another");
    std::fs::write(&one, "one").unwrap();
    std::fs::write(&two, "two").unwrap();
    let review = |path: &Path| {
        quarantine::review_file(path, std::slice::from_ref(&app), &profile, None, &cancel).unwrap()
    };
    let (held, manifest) =
        execute::record_reviewed(&store, &root, None, &[review(&one), review(&two)], &cancel)
            .unwrap();
    store
        .begin_quarantine_operation(&held.quarantine_id, QuarantineOperation::Purging)
        .unwrap();
    let mut bytes: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    bytes["status"] = serde_json::json!("purging");
    std::fs::write(&manifest, serde_json::to_vec(&bytes).unwrap()).unwrap();
    // Simulate a prior authorized purge deleting exactly one blob before power was lost.
    std::fs::remove_file(manifest.with_file_name("blobs").join("1")).unwrap();
    drop(store);
    let store = Store::open(&db).unwrap();
    let out = execute::recover_interrupted(&store, &root).unwrap();
    assert_eq!(out.finalized, 1);
    assert_eq!(out.warnings.len(), 1);
    assert!(manifest.with_file_name("blobs").join("0").is_file());
    assert_eq!(
        store
            .quarantine_record(&restored.quarantine_id)
            .unwrap()
            .unwrap()
            .status,
        QuarantineStatus::Restored
    );
    assert!(execute::restore_by_id(&store, &root, &held.quarantine_id).is_err());
    execute::purge_by_id(&store, &root, &held.quarantine_id).unwrap();
    assert_eq!(
        store
            .quarantine_record(&held.quarantine_id)
            .unwrap()
            .unwrap()
            .status,
        QuarantineStatus::Purged
    );
}

#[test]
fn extra_or_misrecorded_held_bytes_never_get_deleted_by_a_fresh_purge() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("q");
    let a = d.path().join("a");
    let b = d.path().join("b");
    std::fs::write(&a, b"alpha").unwrap();
    std::fs::write(&b, b"beta").unwrap();
    let store = Store::open_in_memory().unwrap();
    let session = store.begin_session("app", None, None).unwrap();
    let q = quarantine_decided(&store, &root, session, &report(&[&a, &b])).unwrap();
    let id = &q.manifest.quarantine_id;
    let blobs = q.manifest_path.with_file_name("blobs");
    std::fs::write(blobs.join("1"), b"fake").unwrap();
    assert!(purge_by_id(&store, &root, id).is_err());
    assert!(store.quarantine_operation(id).unwrap().is_none());
    assert_eq!(std::fs::read(blobs.join("0")).unwrap(), b"alpha");
    assert_eq!(std::fs::read(blobs.join("1")).unwrap(), b"fake");
    // A disk-only state label is not proof of successful recovery.
    let mut disk = q.manifest;
    disk.status = quarantine::ManifestStatus::Purged;
    std::fs::write(&q.manifest_path, serde_json::to_vec(&disk).unwrap()).unwrap();
    assert!(purge_by_id(&store, &root, &disk.quarantine_id).is_err());
    assert_eq!(
        execute::recover_interrupted(&store, &root)
            .unwrap()
            .warnings
            .len(),
        1
    );
    assert_eq!(
        store
            .quarantine_record(&disk.quarantine_id)
            .unwrap()
            .unwrap()
            .status,
        QuarantineStatus::Quarantined
    );
}

#[cfg(windows)]
#[test]
fn windows_same_volume_move_keeps_named_streams_and_archive_refuses_them() {
    use exor_engine::{Cancel, backup};
    let d = tempfile::tempdir().unwrap();
    let profile = d.path().join("profile");
    let app = d.path().join("app");
    std::fs::create_dir(&profile).unwrap();
    std::fs::create_dir(&app).unwrap();
    let file = app.join("installer.dat");
    let stream = |p: &Path| format!("{}:exor-test", p.display());
    std::fs::write(&file, b"primary data").unwrap();
    std::fs::write(stream(&file), b"additional data").unwrap();
    let store = Store::open(&profile.join("exor.db")).unwrap();
    let root = profile.join("quarantine");
    let cancel = Cancel::new();
    let item = quarantine::review_file(&file, &[app], &profile, None, &cancel).unwrap();
    let (batch, manifest) =
        execute::record_reviewed(&store, &root, None, &[item], &cancel).unwrap();
    let blob = manifest.with_file_name("blobs").join("0");
    assert!(!file.exists());
    assert_eq!(std::fs::read(stream(&blob)).unwrap(), b"additional data");
    assert!(backup::create(&store, &profile, d.path()).is_err());
    restore_by_id(&store, &root, &batch.quarantine_id).unwrap();
    assert_eq!(std::fs::read(stream(&file)).unwrap(), b"additional data");
}
