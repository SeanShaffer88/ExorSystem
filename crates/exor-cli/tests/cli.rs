#![allow(clippy::unwrap_used, clippy::expect_used)]

use exor_storage::{
    ChangeKind, ChangeSource, NewLedgerEntry, ScopeCoverage, SessionStatus, StorageError, Store,
    WatchCoverage, WatchSummary, latest_version, migrate_to,
};
use serde_json::Value;
use std::path::Path;
use std::process::{Command, Output};

fn exor(command: &str, db: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_exor"))
        .args([command, "--db"])
        .arg(db)
        .args(args)
        .output()
        .unwrap()
}

fn json(output: Output) -> Value {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    serde_json::from_slice(&output.stdout).unwrap()
}

fn fails(output: Output, status: i32, reason: &str) {
    assert_eq!(
        output.status.code(),
        Some(status),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains(reason));
}

#[test]
fn archive_reads_the_existing_profile_and_preserves_comparison_states() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("test profile Ω.db");
    let writer = Store::open(&db).unwrap();
    let old = writer
        .begin_session("Editor%_ legacy", Some("private.exe"), None)
        .unwrap();
    writer.end_session(old, SessionStatus::Completed).unwrap();
    writer
        .begin_session("EditorXX unrelated", None, None)
        .unwrap();
    let zero = writer
        .begin_session("Editor%_ compared", None, None)
        .unwrap();
    writer
        .record_watch_comparison::<StorageError>(|_| {
            Ok(WatchSummary {
                session_id: zero,
                coverage: WatchCoverage {
                    folders: ScopeCoverage {
                        requested: 1,
                        usable: 1,
                    },
                    ..Default::default()
                },
                files_changed: 0,
                registry_keys_changed: 0,
                services_changed: 0,
                tasks_changed: 0,
                warnings: vec![],
            })
        })
        .unwrap();

    let filter = [
        "--search",
        "editor%_",
        "--status",
        "completed",
        "--limit",
        "1",
    ];
    let page = json(exor("archive", &db, &filter));
    assert_eq!(page["kind"], "watch_archive");
    assert_eq!(page["schema_version"], 1);
    assert_eq!(page["rows"][0]["session_id"], zero);
    assert_eq!(page["rows"][0]["comparison"]["recorded_changes"], 0);
    let cursor = page["next_before_id"].as_i64().unwrap().to_string();
    let older = json(exor(
        "archive",
        &db,
        &[filter.as_slice(), &["--before-id", &cursor]].concat(),
    ));
    assert_eq!(older["rows"][0]["session_id"], old);
    assert!(older["rows"][0]["comparison"].is_null());
    assert!(older["next_before_id"].is_null());
    assert!(!older.to_string().contains("private.exe"));
    let exact = json(exor("archive", &db, &["--search", &format!("#{old}")]));
    assert_eq!(exact["rows"], older["rows"]);
    let empty = json(exor("archive", &db, &["--search", "not present"]));
    assert!(empty["rows"].as_array().unwrap().is_empty());
    assert!(empty["next_before_id"].is_null());
    assert!(writer.watch_comparison(old).unwrap().is_none());
    assert_eq!(writer.schema_version().unwrap(), latest_version());
}

#[test]
fn history_uses_a_session_and_filters_before_paging() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("exor.db");
    let writer = Store::open(&db).unwrap();
    let target = writer.begin_session("Target", None, None).unwrap();
    let other = writer.begin_session("Other", None, None).unwrap();
    for (session_id, path, change_kind) in [
        (target, "old%_fixture.txt", ChangeKind::Modified),
        (target, "new%_fixture.txt", ChangeKind::Modified),
        (target, "newxxfixture.txt", ChangeKind::Modified),
        (target, r"HKCU\Software\new%_fixture", ChangeKind::Modified),
        (other, "other%_fixture.txt", ChangeKind::Modified),
        (target, "gone%_fixture.txt", ChangeKind::Deleted),
    ] {
        writer
            .append_ledger(&NewLedgerEntry {
                session_id: Some(session_id),
                node_id: None,
                path: path.into(),
                change_kind,
                old_hash: None,
                new_hash: None,
                source: ChangeSource::SnapshotDiff,
            })
            .unwrap();
    }
    let session = target.to_string();
    let filters = [
        "--session",
        &session,
        "--path",
        "%_FIXTURE",
        "--domain",
        "file",
        "--kind",
        "modified",
        "--limit",
        "1",
    ];
    let first = json(exor("history", &db, &filters));
    assert_eq!(first["schema_version"], 1);
    assert_eq!(first["kind"], "session_history");
    assert_eq!(first["session_id"], target);
    assert_eq!(first["rows"][0]["entry"]["path"], "new%_fixture.txt");
    let cursor = first["next_before_id"].as_i64().unwrap().to_string();
    let older = json(exor(
        "history",
        &db,
        &[filters.as_slice(), &["--before-id", &cursor]].concat(),
    ));
    assert_eq!(older["rows"][0]["entry"]["path"], "old%_fixture.txt");
    assert!(older["next_before_id"].is_null());
    fails(
        exor("history", &db, &["--session", "123"]),
        1,
        "no recorded session #123",
    );
    assert_eq!(writer.ledger_for_session(target).unwrap().len(), 5);
}

#[test]
fn bad_arguments_and_missing_profiles_do_not_create_a_database() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("not-created.db");
    fails(
        exor("archive", &db, &[]),
        1,
        "existing profile database file",
    );
    fails(
        exor("archive", &db, &["--limit", "51"]),
        2,
        "--limit must be between",
    );
    fails(
        exor("archive", &db, &["--before-id", "0"]),
        2,
        "positive ID",
    );
    fails(
        exor("archive", &db, &["--search", "--status", "completed"]),
        2,
        "missing value",
    );
    fails(
        exor("archive", &db, &["--limit", "1", "--limit", "2"]),
        2,
        "only once",
    );
    fails(exor("history", &db, &[]), 2, "--session ID is required");
    fails(
        exor("uninstall", &db, &[]),
        2,
        "expected the archive or history command",
    );
    let help = Command::new(env!("CARGO_BIN_EXE_exor"))
        .args(["archive", "--help"])
        .output()
        .unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("exor archive --db PATH"));
    let extras = Command::new(env!("CARGO_BIN_EXE_exor"))
        .args(["archive", "--help", "--db"])
        .arg(&db)
        .output()
        .unwrap();
    fails(extras, 2, "--help does not accept other options");
    assert!(!db.exists());
}

#[test]
fn schema_mismatch_is_not_migrated_or_modified() {
    let dir = tempfile::tempdir().unwrap();
    for version in [latest_version() - 1, latest_version() + 1] {
        let db = dir.path().join(format!("schema-{version}.db"));
        let mut conn = rusqlite::Connection::open(&db).unwrap();
        migrate_to(&mut conn, version.min(latest_version())).unwrap();
        if version > latest_version() {
            conn.execute(
                "INSERT INTO schema_migrations(version, name) VALUES (?1, 'future')",
                [version],
            )
            .unwrap();
        }
        drop(conn);
        let before = std::fs::read(&db).unwrap();
        fails(
            exor("archive", &db, &[]),
            1,
            "this CLI does not migrate profiles",
        );
        assert_eq!(std::fs::read(&db).unwrap(), before);
    }
}
