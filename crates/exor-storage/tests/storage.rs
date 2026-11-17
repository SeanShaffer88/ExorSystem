#![allow(clippy::unwrap_used, clippy::expect_used)]
use exor_storage::*;
use serde_json::json;

fn node(path: &str, t: NodeType, s: Option<i64>) -> NewNode {
    NewNode {
        node_type: t,
        path: path.into(),
        hash: None,
        acl_fingerprint: None,
        first_detected_in: s,
    }
}

#[test]
fn migrations_apply_once_and_are_idempotent() {
    let mut c = rusqlite::Connection::open_in_memory().unwrap();
    assert_eq!(migrate(&mut c).unwrap(), vec![1, 2]);
    assert!(migrate(&mut c).unwrap().is_empty());
    assert_eq!(current_version(&c).unwrap(), latest_version());
}

#[test]
fn newer_schema_is_refused() {
    let mut c = rusqlite::Connection::open_in_memory().unwrap();
    migrate(&mut c).unwrap();
    c.execute(
        "INSERT INTO schema_migrations(version,name) VALUES (99,'future')",
        [],
    )
    .unwrap();
    assert!(matches!(
        migrate(&mut c),
        Err(StorageError::SchemaTooNew { found: 99, .. })
    ));
}

#[test]
fn wal_and_foreign_keys_on_file_db() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("exor.db");
    let s = Store::open(&db).unwrap();
    let mode: String = rusqlite::Connection::open(&db)
        .unwrap()
        .query_row("PRAGMA journal_mode", [], |r| r.get(0))
        .unwrap();
    assert_eq!(mode, "wal");
    // FK enforced: node pointing at a missing session fails
    assert!(
        s.upsert_node(&node("C:\\a", NodeType::File, Some(999)))
            .is_err()
    );
}

#[test]
fn upgrade_of_existing_db_keeps_backup() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("exor.db");
    {
        let mut c = rusqlite::Connection::open(&db).unwrap();
        migrate_to(&mut c, 1).unwrap();
    }
    let s = Store::open(&db).unwrap();
    assert_eq!(s.schema_version().unwrap(), 2);
    assert!(dir.path().join("exor.pre-v1.bak").exists());
}

#[test]
fn upsert_keeps_first_detection_and_refreshes_hash() {
    let s = Store::open_in_memory().unwrap();
    let a = s.begin_session("App", None, None).unwrap();
    let b = s.begin_session("App2", None, None).unwrap();
    let id = s
        .upsert_node(&node("C:\\x\\a.dll", NodeType::File, Some(a)))
        .unwrap();
    let mut n = node("C:\\x\\a.dll", NodeType::File, Some(b));
    n.hash = Some("abc".into());
    assert_eq!(s.upsert_node(&n).unwrap(), id);
    let got = s.node(id).unwrap().unwrap();
    assert_eq!(got.first_detected_in, Some(a));
    assert_eq!(got.hash.as_deref(), Some("abc"));
}

#[test]
fn batch_upsert_and_session_listing() {
    let mut s = Store::open_in_memory().unwrap();
    let a = s
        .begin_session("App", Some("C:\\setup.exe"), Some("inno"))
        .unwrap();
    let ids = s
        .upsert_nodes(&[
            node("p1", NodeType::File, Some(a)),
            node("HKLM\\K", NodeType::RegistryKey, Some(a)),
        ])
        .unwrap();
    assert_eq!(ids.len(), 2);
    assert_eq!(s.nodes_for_session(a).unwrap().len(), 2);
    s.end_session(a, SessionStatus::Completed).unwrap();
    let sess = s.session(a).unwrap().unwrap();
    assert_eq!(sess.status, Some(SessionStatus::Completed));
    assert!(sess.ended_at.is_some());
    assert!(matches!(
        s.end_session(999, SessionStatus::Failed),
        Err(StorageError::NotFound(_))
    ));
}

#[test]
fn dag_rejects_cycles_and_keeps_best_confidence() {
    let s = Store::open_in_memory().unwrap();
    let a = s.upsert_node(&node("a", NodeType::File, None)).unwrap();
    let b = s.upsert_node(&node("b", NodeType::File, None)).unwrap();
    let c = s.upsert_node(&node("c", NodeType::File, None)).unwrap();
    s.add_edge(a, b, RelationshipType::CreatedBy, Evidence::Heuristic)
        .unwrap();
    s.add_edge(b, c, RelationshipType::DependsOn, Evidence::Temporal)
        .unwrap();
    assert!(matches!(
        s.add_edge(c, a, RelationshipType::DependsOn, Evidence::Manifest),
        Err(StorageError::CycleDetected { .. })
    ));
    assert!(matches!(
        s.add_edge(a, a, RelationshipType::ModifiedBy, Evidence::Manifest),
        Err(StorageError::CycleDetected { .. })
    ));
    // shared_with is symmetric and allowed
    s.add_edge(c, a, RelationshipType::SharedWith, Evidence::Temporal)
        .unwrap();
    // re-adding raises confidence, never lowers it
    let e1 = s
        .add_edge(a, b, RelationshipType::CreatedBy, Evidence::Manifest)
        .unwrap();
    let e2 = s
        .add_edge(a, b, RelationshipType::CreatedBy, Evidence::Heuristic)
        .unwrap();
    assert_eq!(e1, e2);
    let edge = &s.edges_as_child(b).unwrap()[0];
    assert_eq!(edge.confidence_score, 1.0);
    assert_eq!(edge.evidence_source.as_deref(), Some("manifest"));
}

#[test]
fn confidence_scores_match_blueprint() {
    assert_eq!(Evidence::Manifest.score(), 1.0);
    assert_eq!(Evidence::Temporal.score(), 0.7);
    assert_eq!(Evidence::Heuristic.score(), 0.3);
}

#[test]
fn db_check_constraint_blocks_out_of_range_confidence() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("c.db");
    let s = Store::open(&db).unwrap();
    let a = s.upsert_node(&node("a", NodeType::File, None)).unwrap();
    let b = s.upsert_node(&node("b", NodeType::File, None)).unwrap();
    let raw = rusqlite::Connection::open(&db).unwrap();
    let r = raw.execute(
        "INSERT INTO node_relationships(parent_node_id, child_node_id, relationship_type, confidence_score) VALUES (?1, ?2, 'depends_on', 1.5)",
        [a, b],
    );
    assert!(r.is_err());
}

#[test]
fn provenance_traces_to_session_with_ledger() {
    let s = Store::open_in_memory().unwrap();
    let sess = s
        .begin_session("Foo", Some("C:\\foo-setup.exe"), Some("nsis"))
        .unwrap();
    let inst = s
        .upsert_node(&node("C:\\foo-setup.exe", NodeType::File, Some(sess)))
        .unwrap();
    let art = s
        .upsert_node(&node(
            "C:\\Program Files\\Foo\\foo.exe",
            NodeType::File,
            Some(sess),
        ))
        .unwrap();
    s.add_edge(inst, art, RelationshipType::CreatedBy, Evidence::Temporal)
        .unwrap();
    s.append_ledger(&NewLedgerEntry {
        session_id: Some(sess),
        node_id: Some(art),
        path: "C:\\Program Files\\Foo\\foo.exe".into(),
        change_kind: ChangeKind::Created,
        old_hash: None,
        new_hash: Some("h1".into()),
        source: ChangeSource::UsnJournal,
    })
    .unwrap();
    // case-insensitive path lookup in ledger
    let p = s
        .provenance_for_path("C:\\Program Files\\Foo\\foo.exe")
        .unwrap()
        .unwrap();
    assert_eq!(
        p.session.unwrap().fingerprint_family.as_deref(),
        Some("nsis")
    );
    assert_eq!(p.ledger.len(), 1);
    assert_eq!(p.confidence, 0.7);
    assert!(s.provenance_for_path("nope").unwrap().is_none());
    assert_eq!(s.ledger_for_session(sess).unwrap().len(), 1);
}

#[test]
fn unattributed_node_defaults_to_heuristic_confidence() {
    let s = Store::open_in_memory().unwrap();
    s.upsert_node(&node("lonely", NodeType::File, None))
        .unwrap();
    assert_eq!(
        s.provenance_for_path("lonely").unwrap().unwrap().confidence,
        0.3
    );
}

#[test]
fn ledger_is_append_only() {
    let s = Store::open_in_memory().unwrap();
    let id = s
        .append_ledger(&NewLedgerEntry {
            session_id: None,
            node_id: None,
            path: "p".into(),
            change_kind: ChangeKind::Deleted,
            old_hash: None,
            new_hash: None,
            source: ChangeSource::SnapshotDiff,
        })
        .unwrap();
    assert!(id > 0);
    // no public mutate API; prove the trigger by raw SQL on a second connection to same memory is impossible,
    // so exercise via a file db
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("l.db");
    let s2 = Store::open(&db).unwrap();
    s2.append_ledger(&NewLedgerEntry {
        session_id: None,
        node_id: None,
        path: "p".into(),
        change_kind: ChangeKind::Created,
        old_hash: None,
        new_hash: None,
        source: ChangeSource::Manifest,
    })
    .unwrap();
    let raw = rusqlite::Connection::open(&db).unwrap();
    assert!(
        raw.execute("UPDATE change_ledger SET path='x'", [])
            .is_err()
    );
    assert!(raw.execute("DELETE FROM change_ledger", []).is_err());
}

#[test]
fn shared_component_impact_and_orphans() {
    let s = Store::open_in_memory().unwrap();
    let s1 = s.begin_session("A", None, None).unwrap();
    let s2 = s.begin_session("B", None, None).unwrap();
    let app_a = s
        .upsert_node(&node("a.exe", NodeType::File, Some(s1)))
        .unwrap();
    let app_b = s
        .upsert_node(&node("b.exe", NodeType::File, Some(s2)))
        .unwrap();
    let only_a = s
        .upsert_node(&node("liba.dll", NodeType::File, Some(s1)))
        .unwrap();
    let common = s
        .upsert_node(&node("common.dll", NodeType::File, Some(s1)))
        .unwrap();
    let shared = s
        .upsert_node(&node("vcrt.dll", NodeType::File, Some(s1)))
        .unwrap();
    s.add_edge(
        only_a,
        app_a,
        RelationshipType::DependsOn,
        Evidence::Manifest,
    )
    .unwrap();
    s.add_edge(
        common,
        app_a,
        RelationshipType::DependsOn,
        Evidence::Manifest,
    )
    .unwrap();
    s.add_edge(
        common,
        app_b,
        RelationshipType::DependsOn,
        Evidence::Manifest,
    )
    .unwrap();
    s.add_edge(
        shared,
        app_a,
        RelationshipType::DependsOn,
        Evidence::Temporal,
    )
    .unwrap();
    s.add_edge(
        shared,
        app_b,
        RelationshipType::SharedWith,
        Evidence::Temporal,
    )
    .unwrap();

    let impact = s.shared_component_impact(common).unwrap();
    assert_eq!(impact.dependents.len(), 2);
    assert_eq!(impact.sessions, vec![s1, s2]);

    let orphans: Vec<String> = s
        .orphaned_dependencies(s1)
        .unwrap()
        .into_iter()
        .map(|n| n.path)
        .collect();
    assert_eq!(orphans, vec!["liba.dll"]); // common used by B, vcrt explicitly shared
}

#[test]
fn policy_rules_roundtrip_and_validation() {
    let s = Store::open_in_memory().unwrap();
    let id = s
        .add_rule(
            "keep signed",
            &json!({"==": [{"var": "signed"}, true]}),
            PolicyAction::Keep,
        )
        .unwrap();
    assert!(
        s.add_rule("bad", &json!("not an object"), PolicyAction::Keep)
            .is_err()
    );
    let rules = s.rules().unwrap();
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0].action, PolicyAction::Keep);
    assert!(s.delete_rule(id).unwrap());
    assert!(!s.delete_rule(id).unwrap());
}

#[test]
fn quarantine_records_roundtrip() {
    let s = Store::open_in_memory().unwrap();
    s.record_quarantine(&QuarantineRecord {
        quarantine_id: "q1".into(),
        rule_id: None,
        session_id: None,
        status: QuarantineStatus::Quarantined,
        manifest_path: "m.json".into(),
        manifest_json: "{}".into(),
    })
    .unwrap();
    s.set_quarantine_status("q1", QuarantineStatus::Restored)
        .unwrap();
    assert_eq!(
        s.quarantines().unwrap()[0].status,
        QuarantineStatus::Restored
    );
    assert!(
        s.set_quarantine_status("zz", QuarantineStatus::Purged)
            .is_err()
    );
}

// ---- failure drills ----

#[test]
fn corrupt_database_is_rejected_and_left_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("exor.db");
    let junk =
        b"this is definitely not a sqlite database, it is long enough to look like a header....."
            .repeat(8);
    std::fs::write(&db, &junk).unwrap();
    assert!(Store::open(&db).is_err());
    assert_eq!(
        std::fs::read(&db).unwrap(),
        junk,
        "never rewrite or truncate a file we cannot read"
    );
}

#[test]
fn concurrent_writers_lose_no_ledger_rows() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("exor.db");
    Store::open(&db).unwrap();
    let handles: Vec<_> = (0..4)
        .map(|t| {
            let db = db.clone();
            std::thread::spawn(move || {
                let s = Store::open(&db).unwrap();
                for i in 0..50 {
                    s.append_ledger(&NewLedgerEntry {
                        session_id: None,
                        node_id: None,
                        path: format!("t{t}-{i}"),
                        change_kind: ChangeKind::Created,
                        old_hash: None,
                        new_hash: None,
                        source: ChangeSource::SnapshotDiff,
                    })
                    .unwrap();
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let s = Store::open(&db).unwrap();
    let n: i64 = rusqlite::Connection::open(&db)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM change_ledger", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 200);
    drop(s);
}

#[test]
fn backup_before_upgrade_is_a_working_old_version_database() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("exor.db");
    {
        let mut c = rusqlite::Connection::open(&db).unwrap();
        migrate_to(&mut c, 1).unwrap();
        c.execute(
            "INSERT INTO installation_sessions(app_name) VALUES ('Old')",
            [],
        )
        .unwrap();
    }
    Store::open(&db).unwrap();
    let bak = rusqlite::Connection::open(dir.path().join("exor.pre-v1.bak")).unwrap();
    let v: u32 = bak
        .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
            r.get(0)
        })
        .unwrap();
    let name: String = bak
        .query_row("SELECT app_name FROM installation_sessions", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        (v, name.as_str()),
        (1, "Old"),
        "rolling back means copying the .bak over the db"
    );
}
