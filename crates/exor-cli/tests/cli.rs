//! End-to-end tests of the built `exor` binary: JSON envelope, exit codes, snapshot/diff round
//! trip and read-only profile access.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use serde_json::Value;
use std::path::Path;
use std::process::Command;

fn exor(args: &[&str]) -> (u8, Value) {
    let out = Command::new(env!("CARGO_BIN_EXE_exor"))
        .args(args)
        .output()
        .unwrap();
    let code = u8::try_from(out.status.code().unwrap()).unwrap();
    let json: Value = serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("not JSON ({e}): {}", String::from_utf8_lossy(&out.stdout)));
    (code, json)
}

fn s(p: &Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn version_and_usage_errors_use_the_envelope_and_exit_codes() {
    let (code, v) = exor(&["version"]);
    assert_eq!(code, 0);
    assert_eq!(v["ok"], true);
    assert_eq!(v["command"], "version");
    assert_eq!(v["data"]["profile_schema"], exor_storage::latest_version());

    let (code, v) = exor(&["bogus"]);
    assert_eq!(code, 2);
    assert_eq!(v["ok"], false);
    assert_eq!(v["error"]["kind"], "usage");

    let out = Command::new(env!("CARGO_BIN_EXE_exor"))
        .arg("help")
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("Exit codes"));
}

#[test]
fn snapshot_and_diff_round_trip_without_overwriting() {
    let d = tempfile::tempdir().unwrap();
    let app = d.path().join("app");
    std::fs::create_dir_all(app.join("sub")).unwrap();
    std::fs::write(app.join("keep.txt"), b"same").unwrap();
    std::fs::write(app.join("edit.txt"), b"before").unwrap();
    std::fs::write(app.join("sub/gone.txt"), b"bye").unwrap();
    let (before, after) = (d.path().join("before.json"), d.path().join("after.json"));

    let (code, v) = exor(&["snapshot", s(&app), "--out", s(&before)]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["data"]["files"], 3);
    assert_eq!(v["data"]["complete"], true);
    // never replaces an existing file
    let (code, v) = exor(&["snapshot", s(&app), "--out", s(&before)]);
    assert_eq!(code, 3, "{v}");
    assert_eq!(v["error"]["kind"], "refused");

    std::fs::write(app.join("edit.txt"), b"after, and longer").unwrap();
    std::fs::remove_file(app.join("sub/gone.txt")).unwrap();
    std::fs::write(app.join("new.txt"), b"hello").unwrap();
    assert_eq!(exor(&["snapshot", s(&app), "--out", s(&after)]).0, 0);

    let (code, v) = exor(&["diff", s(&before), s(&after)]);
    assert_eq!(code, 0, "{v}");
    let data = &v["data"];
    assert_eq!(
        (&data["added"], &data["removed"], &data["modified"]),
        (&Value::from(1), &Value::from(1), &Value::from(1)),
        "{data}"
    );
    assert_eq!(data["same_root"], true);
    let paths: Vec<&str> = data["changes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["path"].as_str().unwrap())
        .collect();
    assert!(paths.iter().any(|p| p.ends_with("new.txt")));
    assert!(paths.iter().any(|p| p.ends_with("gone.txt")));
    assert!(!paths.iter().any(|p| p.ends_with("keep.txt")));

    let (code, v) = exor(&["diff", s(&before), s(&d.path().join("missing.json"))]);
    assert_eq!((code, &v["error"]["kind"]), (4, &Value::from("not_found")));
    std::fs::write(d.path().join("junk.json"), b"{}").unwrap();
    assert_eq!(
        exor(&["diff", s(&before), s(&d.path().join("junk.json"))]).0,
        1
    );
    assert_eq!(exor(&["snapshot", s(&d.path().join("nope"))]).0, 4);
}

#[test]
fn reads_a_profile_without_changing_it() {
    let d = tempfile::tempdir().unwrap();
    let db = d.path().join("exor.db");
    {
        let store = exor_storage::Store::open(&db).unwrap();
        store.begin_session("Fixture App", None, None).unwrap();
    }
    let before = std::fs::read(&db).unwrap();
    let (code, v) = exor(&["sessions", "--profile", s(d.path())]);
    assert_eq!(code, 0, "{v}");
    assert_eq!(v["data"]["count"], 1);
    assert_eq!(v["data"]["sessions"][0]["app_name"], "Fixture App");
    let (code, v) = exor(&["history", "--profile", s(d.path()), "--limit", "5"]);
    assert_eq!(code, 0, "{v}");
    assert!(v["data"]["entries"].is_array());
    assert_eq!(
        std::fs::read(&db).unwrap(),
        before,
        "the profile database was modified"
    );

    let empty = tempfile::tempdir().unwrap();
    let (code, v) = exor(&["sessions", "--profile", s(empty.path())]);
    assert_eq!((code, &v["error"]["kind"]), (4, &Value::from("not_found")));
}
