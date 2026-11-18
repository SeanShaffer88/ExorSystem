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
    assert_eq!(migrate(&mut c).unwrap(), vec![1, 2, 3, 4]);
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
    assert_eq!(s.schema_version().unwrap(), latest_version());
    assert!(dir.path().join("exor.pre-v1.bak").exists());
}

#[test]
fn inventory_migration_preserves_v2_nodes_ledger_and_backup() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("exor.db");
    {
        let mut c = rusqlite::Connection::open(&db).unwrap();
        migrate_to(&mut c, 2).unwrap();
        c.execute(
            "INSERT INTO system_nodes(node_type, path) VALUES ('file', ?1)",
            [r"C:\Tools\Widget.exe"],
        )
        .unwrap();
        c.execute(
            "INSERT INTO change_ledger(node_id, path, change_kind, source) VALUES (1, ?1, 'created', 'snapshot_diff')",
            [r"C:\Tools\Widget.exe"],
        ).unwrap();
    }
    let store = Store::open(&db).unwrap();
    assert_eq!(store.schema_version().unwrap(), latest_version());
    assert!(store.portable_programs().unwrap().is_empty());
    let provenance = store
        .provenance_for_path(r"c:\tools\widget.EXE")
        .unwrap()
        .unwrap();
    assert_eq!(provenance.node.path, r"C:\Tools\Widget.exe");
    assert_eq!(provenance.ledger.len(), 1);
    let backup = rusqlite::Connection::open(dir.path().join("exor.pre-v2.bak")).unwrap();
    assert_eq!(current_version(&backup).unwrap(), 2);
    assert_eq!(
        backup
            .query_row("SELECT path FROM change_ledger", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        r"C:\Tools\Widget.exe"
    );
}

#[test]
fn comparison_migration_keeps_existing_evidence_without_inventing_coverage() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("exor.db");
    {
        let mut c = rusqlite::Connection::open(&db).unwrap();
        migrate_to(&mut c, 3).unwrap();
        c.execute_batch(
            "INSERT INTO installation_sessions(app_name, status) VALUES ('Older watch', 'completed');
             INSERT INTO change_ledger(session_id, path, change_kind, source)
             VALUES (1, 'old-file.txt', 'deleted', 'snapshot_diff');",
        ).unwrap();
    }
    let store = Store::open(&db).unwrap();
    assert_eq!(store.schema_version().unwrap(), 4);
    assert_eq!(store.session(1).unwrap().unwrap().app_name, "Older watch");
    assert_eq!(
        store.ledger_for_session(1).unwrap()[0].entry.path,
        "old-file.txt"
    );
    assert_eq!(store.watch_comparison(1).unwrap(), None);
    let backup = rusqlite::Connection::open(dir.path().join("exor.pre-v3.bak")).unwrap();
    assert_eq!(current_version(&backup).unwrap(), 3);
    assert_eq!(
        backup
            .query_row("SELECT COUNT(*) FROM change_ledger", [], |r| r
                .get::<_, u32>(0))
            .unwrap(),
        1
    );
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
    // case-insensitive path lookup for both the node and ledger
    let p = s
        .provenance_for_path("c:\\PROGRAM FILES\\foo\\FOO.EXE")
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
fn provenance_does_not_merge_legacy_case_only_siblings() {
    let store = Store::open_in_memory().unwrap();
    for path in [r"HKCU\Software\Widget", r"HKCU\Software\widget"] {
        let id = store
            .upsert_node(&node(path, NodeType::RegistryKey, None))
            .unwrap();
        store
            .append_ledger(&NewLedgerEntry {
                session_id: None,
                node_id: Some(id),
                path: path.into(),
                change_kind: ChangeKind::Created,
                old_hash: None,
                new_hash: None,
                source: ChangeSource::RegistryCallback,
            })
            .unwrap();
    }
    let exact = store
        .provenance_for_path(r"HKCU\Software\Widget")
        .unwrap()
        .unwrap();
    assert_eq!(exact.ledger.len(), 1);
    assert_eq!(exact.ledger[0].entry.node_id, Some(exact.node.node_id));
    assert!(matches!(
        store.provenance_for_path(r"HKCU\Software\WIDGET"),
        Err(StorageError::AmbiguousPath(_))
    ));
}

#[test]
fn portable_entries_survive_reopen_without_touching_executables() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("exor.db");
    let exe = dir.path().join("Widget.exe");
    std::fs::write(&exe, b"test data").unwrap();
    let path = exe.to_str().unwrap().to_string();
    let store = Store::open(&db).unwrap();
    store
        .save_portable_program(&PortableProgram {
            name: "Widget".into(),
            executable: path.clone(),
        })
        .unwrap();
    let alias = path.to_ascii_lowercase();
    store
        .save_portable_program(&PortableProgram {
            name: "My Widget".into(),
            executable: alias.clone(),
        })
        .unwrap();
    drop(store);
    let store = Store::open(&db).unwrap();
    assert_eq!(
        store.portable_programs().unwrap(),
        [PortableProgram {
            name: "My Widget".into(),
            executable: path
        }]
    );
    assert!(store.forget_portable_program(&alias).unwrap());
    assert!(!store.forget_portable_program(&alias).unwrap());
    assert!(store.portable_programs().unwrap().is_empty());
    assert_eq!(std::fs::read(exe).unwrap(), b"test data");
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

#[test]
fn ledger_recent_is_newest_first_and_filters_by_path() {
    let s = Store::open_in_memory().unwrap();
    for p in ["C:\\a\\One.txt", "C:\\b\\two_%.txt", "C:\\a\\three.txt"] {
        s.append_ledger(&NewLedgerEntry {
            session_id: None,
            node_id: None,
            path: p.into(),
            change_kind: ChangeKind::Created,
            old_hash: None,
            new_hash: None,
            source: ChangeSource::Policy,
        })
        .unwrap();
    }
    let all = s.ledger_recent(10, None).unwrap();
    assert_eq!(all.len(), 3);
    assert!(all[0].entry.path.ends_with("three.txt"));
    assert_eq!(s.ledger_recent(2, None).unwrap().len(), 2);
    assert_eq!(s.ledger_recent(10, Some("one")).unwrap().len(), 1);
    // wildcards in the filter are literal
    assert_eq!(s.ledger_recent(10, Some("%")).unwrap().len(), 1);
    assert_eq!(s.ledger_recent(10, Some("_")).unwrap().len(), 1);
    assert_eq!(s.ledger_recent(10, Some("zzz")).unwrap().len(), 0);
}

#[test]
fn ledger_pages_filter_before_limiting_and_keep_their_boundary_after_appends() {
    let store = Store::open_in_memory().unwrap();
    let watch = store.begin_session("Watch", None, None).unwrap();
    let other = store.begin_session("Other", None, None).unwrap();
    let append = |session_id, path: &str, change_kind| {
        store
            .append_ledger(&NewLedgerEntry {
                session_id,
                node_id: None,
                path: path.to_string(),
                change_kind,
                old_hash: None,
                new_hash: None,
                source: ChangeSource::SnapshotDiff,
            })
            .unwrap()
    };
    let removed = append(
        Some(watch),
        r"HKCU\Software\Example\App_%\Removed",
        ChangeKind::Deleted,
    );
    let changed = append(
        Some(watch),
        r"HKCU\Software\Example\App_%\Changed",
        ChangeKind::Modified,
    );
    let new = append(
        Some(watch),
        r"HKCU\Software\Example\App_%\New",
        ChangeKind::Created,
    );
    append(
        Some(watch),
        r"HKCU\Software\Example\App_123\Nonmatch",
        ChangeKind::Created,
    );
    let other_id = append(
        Some(other),
        r"HKCU\Software\Example\App_%\Other",
        ChangeKind::Created,
    );
    let unassigned = append(
        None,
        r"HKCU\Software\Example\App_%\Unassigned",
        ChangeKind::Modified,
    );

    let filter = Some(r"software\example\app_%");
    let ids = |page: &LedgerPage| {
        page.entries
            .iter()
            .map(|entry| entry.entry_id)
            .collect::<Vec<_>>()
    };
    let first = store.ledger_page(2, filter, Some(watch), None).unwrap();
    assert_eq!(ids(&first), [new, changed]);
    assert_eq!(first.next_before_id, Some(changed));
    append(
        Some(watch),
        r"HKCU\Software\Example\App_%\AppendedLater",
        ChangeKind::Created,
    );
    let older = store
        .ledger_page(2, filter, Some(watch), first.next_before_id)
        .unwrap();
    assert_eq!(ids(&older), [removed]);
    assert_eq!(older.next_before_id, None);
    assert_eq!(older.entries[0].entry.change_kind, ChangeKind::Deleted);
    assert_eq!(older.entries[0].entry.source, ChangeSource::SnapshotDiff);
    assert!(older.entries[0].entry.node_id.is_none());

    let exact = store.ledger_page(1, filter, Some(other), None).unwrap();
    assert_eq!(ids(&exact), [other_id]);
    assert_eq!(exact.next_before_id, None);
    let all = store.ledger_page(10, filter, None, None).unwrap();
    assert!(ids(&all).contains(&other_id) && ids(&all).contains(&unassigned));
    assert!(
        store
            .ledger_page(10, Some("absent"), Some(watch), None)
            .unwrap()
            .entries
            .is_empty()
    );
}

#[test]
fn inspector_pages_classify_unlinked_removals_and_filter_before_limit() {
    let s = Store::open_in_memory().unwrap();
    let append = |path: &str| {
        s.append_ledger(&NewLedgerEntry {
            session_id: None,
            node_id: None,
            path: path.into(),
            change_kind: ChangeKind::Deleted,
            old_hash: None,
            new_hash: None,
            source: ChangeSource::SnapshotDiff,
        })
        .unwrap()
    };
    let user = append(r"hkcu\Software\Example");
    let machine32 = append(r"HKLM32\Software\Example");
    let path = append(r"C:\Example\main.exe");
    let task = append(r"task:\Example\Updater");
    let page = s
        .ledger_page_in_domain(1, Some("example"), None, None, Some(ChangeDomain::Registry))
        .unwrap();
    assert_eq!(page.entries[0].entry_id, machine32);
    assert_eq!(page.entries[0].domain, ChangeDomain::Registry);
    assert_eq!(page.next_before_id, Some(machine32));
    let older = s
        .ledger_page_in_domain(
            1,
            None,
            None,
            page.next_before_id,
            Some(ChangeDomain::Registry),
        )
        .unwrap();
    assert_eq!(older.entries[0].entry_id, user);
    assert!(older.next_before_id.is_none());
    let all = s.ledger_page(4, None, None, None).unwrap();
    assert_eq!(
        (all.entries[0].entry_id, all.entries[0].domain),
        (task, ChangeDomain::System)
    );
    assert_eq!(
        (all.entries[1].entry_id, all.entries[1].domain),
        (path, ChangeDomain::File)
    );
}

#[test]
fn restart_marks_only_open_sessions_failed_without_inventing_end_time_or_comparisons() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("restart.db");
    let store = Store::open(&db).unwrap();
    let older = store.begin_session("Older install", None, None).unwrap();
    store.end_session(older, SessionStatus::Completed).unwrap();
    let saved = store.begin_session("No changes", None, None).unwrap();
    let skipped = store.begin_session("Only coverage", None, None).unwrap();
    for (session_id, usable) in [(saved, 1), (skipped, 0)] {
        store
            .record_watch_comparison::<StorageError>(|_| {
                Ok(WatchSummary {
                    session_id,
                    coverage: WatchCoverage {
                        folders: ScopeCoverage {
                            requested: 1,
                            usable,
                        },
                        ..Default::default()
                    },
                    files_changed: 0,
                    registry_keys_changed: 0,
                    services_changed: 0,
                    tasks_changed: 0,
                    warnings: Vec::new(),
                })
            })
            .unwrap();
    }
    let open = store.begin_session("Interrupted", None, None).unwrap();
    let original = store.session(open).unwrap().unwrap();
    drop(store);

    let store = Store::open(&db).unwrap();
    let before = store.recent_watch_runs(6).unwrap();
    assert_eq!(store.recover_unfinished_sessions().unwrap(), 1);
    assert_eq!(store.recover_unfinished_sessions().unwrap(), 0);
    let recovered = store.session(open).unwrap().unwrap();
    assert_eq!(
        recovered,
        Session {
            ended_at: None,
            status: Some(SessionStatus::Failed),
            ..original
        }
    );
    assert!(store.watch_comparison(open).unwrap().is_none());
    assert!(store.ledger_for_session(open).unwrap().is_empty());
    assert_eq!(store.recent_watch_runs(6).unwrap(), before);
    assert_eq!(
        before
            .iter()
            .map(|run| run.summary.session_id)
            .collect::<Vec<_>>(),
        [skipped, saved]
    );
    assert_eq!(before[0].app_name, "Only coverage");
    assert!(before[0].ended_at.is_some());
    assert!(!before[0].summary.coverage.has_usable());
    assert!(before[1].summary.coverage.has_usable());
    assert_eq!(before[1].summary.files_changed, 0);
    assert_eq!(store.recent_watch_runs(1).unwrap(), before[..1]);
    assert_eq!(
        store.session(older).unwrap().unwrap().status,
        Some(SessionStatus::Completed)
    );
}

#[test]
fn ledger_kind_composes_with_domain_session_path_and_cursor_before_limit() {
    let store = Store::open_in_memory().unwrap();
    let a = store.begin_session("A", None, None).unwrap();
    let b = store.begin_session("B", None, None).unwrap();
    let append = |session_id, path: &str, change_kind| {
        store
            .append_ledger(&NewLedgerEntry {
                session_id: Some(session_id),
                node_id: None,
                path: path.into(),
                change_kind,
                old_hash: None,
                new_hash: None,
                source: ChangeSource::SnapshotDiff,
            })
            .unwrap()
    };
    let old = append(a, r"HKLM32\Software\App_%\Old", ChangeKind::Deleted);
    let new = append(a, r"HKCU\Software\App_%\New", ChangeKind::Deleted);
    append(b, r"HKCU\Software\App_%\Other", ChangeKind::Deleted);
    append(a, r"C:\Software\App_%\File", ChangeKind::Deleted);
    append(a, r"HKCU\Software\App_%\Modified", ChangeKind::Modified);
    append(a, r"HKCU\Software\App_ab\NotLiteral", ChangeKind::Deleted);

    let query = LedgerQuery {
        session_id: Some(a),
        domain: Some(ChangeDomain::Registry),
        path_contains: Some(r"software\app_%"),
        change_kind: Some(ChangeKind::Deleted),
        ..Default::default()
    };
    let first = store.ledger_query(1, query).unwrap();
    assert_eq!(first.entries[0].entry_id, new);
    assert_eq!(first.next_before_id, Some(new));
    append(a, r"HKCU\Software\App_%\Later", ChangeKind::Deleted);
    let older = store
        .ledger_query(
            1,
            LedgerQuery {
                before_id: first.next_before_id,
                ..query
            },
        )
        .unwrap();
    assert_eq!(older.entries[0].entry_id, old);
    assert!(older.next_before_id.is_none());
}

#[test]
fn session_node_pages_search_past_the_old_500_item_preview() {
    let mut store = Store::open_in_memory().unwrap();
    let session = store.begin_session("Large install", None, None).unwrap();
    let other = store.begin_session("Separate install", None, None).unwrap();
    let nodes: Vec<_> = (0..530)
        .rev()
        .map(|i| {
            node(
                &format!(r"C:\Demo\Asset_{i:03}%.txt"),
                NodeType::File,
                Some(session),
            )
        })
        .collect();
    store.upsert_nodes(&nodes).unwrap();
    store
        .upsert_node(&node(
            r"D:\Demo\asset_529%.txt",
            NodeType::File,
            Some(other),
        ))
        .unwrap();
    let first = store.session_nodes_page(session, None, 0, 12).unwrap();
    let last = store.session_nodes_page(session, None, 528, 12).unwrap();
    assert_eq!(
        (first.total, first.matching, first.nodes.len()),
        (530, 530, 12)
    );
    assert_eq!(first.nodes[0].path, r"C:\Demo\Asset_000%.txt");
    assert_eq!(last.nodes.len(), 2);
    assert_eq!(last.nodes[1].path, r"C:\Demo\Asset_529%.txt");
    let found = store
        .session_nodes_page(session, Some(r"demo\asset_529%"), 0, 12)
        .unwrap();
    assert_eq!((found.total, found.matching), (530, 1));
    assert_eq!(found.nodes, last.nodes[1..]);
    assert!(
        store
            .session_nodes_page(session, Some("asset_5_%"), usize::MAX, 12)
            .unwrap()
            .nodes
            .is_empty()
    );
}
