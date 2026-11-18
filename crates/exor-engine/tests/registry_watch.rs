#![allow(clippy::unwrap_used)]

use exor_engine::collectors::{ServiceInfo, TaskInfo};
use exor_engine::policy::{candidates_for_session, simulate};
use exor_engine::registry::{RegistryKeyState, RegistryScope, RegistryScopes};
use exor_engine::watch::{InstallWatch, SystemProbe};
use exor_engine::{Cancel, report, templates};
use exor_storage::{
    ChangeKind, ChangeSource, Evidence, NodeType, PolicyAction, RelationshipType, SessionStatus,
    Store,
};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

const COMPLETE: &str = r"HKCU\Software\Example";
const APPEARS: &str = r"HKLM32\Software\Example";
const NO_BASELINE: &str = r"HKCU\Software\NoBaseline";
const NO_AFTER: &str = r"HKLM\Software\NoAfter";

#[derive(Default)]
struct MockProbe {
    calls: Mutex<BTreeMap<String, usize>>,
}

impl SystemProbe for MockProbe {
    fn services(&self) -> Result<Vec<ServiceInfo>, String> {
        Ok(Vec::new())
    }

    fn tasks(&self) -> Result<Vec<TaskInfo>, String> {
        Ok(Vec::new())
    }

    fn registry_keys(&self, scope: &RegistryScope) -> Result<Vec<RegistryKeyState>, String> {
        let scope = scope.to_string();
        let mut calls = self.calls.lock().unwrap();
        let count = calls.entry(scope.clone()).or_default();
        *count += 1;
        let key = |suffix, last_write| RegistryKeyState {
            path: format!("{scope}{suffix}"),
            last_write,
        };
        match (scope.as_str(), *count) {
            (COMPLETE, 1) => Ok(vec![
                key(r"\Updated", 1),
                key(r"\Gone", 1),
                key(r"\Same", 1),
            ]),
            (COMPLETE, 2) => Ok(vec![key(r"\Updated", 2), key(r"\New", 1), key(r"\same", 1)]),
            (APPEARS, 1) => Ok(Vec::new()),
            (APPEARS, 2) => Ok(vec![key("", 1)]),
            (NO_BASELINE, 1) => Err("access denied".into()),
            (NO_AFTER, 1) => Ok(vec![key("", 1)]),
            (NO_AFTER, 2) => Err("capture limit exceeded".into()),
            _ => Err(format!("unexpected snapshot for {scope}")),
        }
    }
}

#[tokio::test]
async fn watch_records_only_complete_registry_pairs_as_advisory_evidence() {
    let scopes =
        RegistryScopes::parse(&[COMPLETE, APPEARS, NO_BASELINE, NO_AFTER].join("\n")).unwrap();
    let store = Arc::new(Mutex::new(Store::open_in_memory().unwrap()));
    let probe = Arc::new(MockProbe::default());
    let cancel = Cancel::new();
    let watch = InstallWatch::begin(
        Arc::clone(&store),
        "Example",
        Some("setup.exe"),
        &[],
        &scopes,
        probe.clone(),
        &cancel,
    )
    .await
    .unwrap();
    let sid = watch.session_id();
    assert_eq!(watch.warnings().len(), 1);
    assert!(watch.warnings()[0].contains(NO_BASELINE));
    assert!(watch.warnings()[0].contains("before"));

    let summary = watch.finish(&cancel).await.unwrap();
    assert_eq!(summary.session_id, sid);
    assert_eq!(summary.registry_keys_changed, 4);
    assert_eq!(summary.registry_scopes_compared, 2);
    assert_eq!(summary.warnings.len(), 2);
    assert!(summary.warnings[1].contains(NO_AFTER));
    assert!(summary.warnings[1].contains("after"));
    assert_eq!(probe.calls.lock().unwrap().get(NO_BASELINE), Some(&1));

    let store = store.lock().unwrap();
    assert_eq!(
        store.session(sid).unwrap().unwrap().status,
        Some(SessionStatus::Completed)
    );
    let ledger = store.ledger_for_session(sid).unwrap();
    let mut observed = BTreeMap::new();
    for row in &ledger {
        let event = &row.entry;
        assert_eq!(event.source, ChangeSource::SnapshotDiff);
        assert!(event.old_hash.is_none() && event.new_hash.is_none());
        assert_eq!(
            event.node_id.is_none(),
            event.change_kind == ChangeKind::Deleted
        );
        observed.insert(event.path.as_str(), event.change_kind);
    }
    assert_eq!(
        observed,
        BTreeMap::from([
            (r"HKCU\Software\Example\New", ChangeKind::Created),
            (r"HKCU\Software\Example\Updated", ChangeKind::Modified),
            (r"HKCU\Software\Example\Gone", ChangeKind::Deleted),
            (APPEARS, ChangeKind::Created),
        ])
    );
    for (path, relation) in [
        (r"hkcu\software\example\new", RelationshipType::CreatedBy),
        (
            r"hkcu\software\example\updated",
            RelationshipType::ModifiedBy,
        ),
    ] {
        let trace = store.provenance_for_path(path).unwrap().unwrap();
        assert_eq!(trace.node.node_type, NodeType::RegistryKey);
        assert_eq!(trace.origins.len(), 1);
        assert_eq!(trace.origins[0].relationship_type, relation);
        assert_eq!(
            trace.origins[0].confidence_score,
            Evidence::Heuristic.score()
        );
        assert!(report::describe_provenance(&trace).contains("snapshot cannot tell which process"));
    }
    let candidates = candidates_for_session(&store, sid).unwrap();
    let preview = simulate(&templates::rules_for("thorough").unwrap(), &candidates).unwrap();
    let system_items: Vec<_> = preview
        .decisions
        .iter()
        .filter(|d| d.node_type == NodeType::RegistryKey)
        .collect();
    assert_eq!(system_items.len(), 3);
    assert!(
        system_items
            .iter()
            .all(|d| d.action == PolicyAction::FlagForReview)
    );
}
