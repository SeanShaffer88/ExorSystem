//! Executing a simulated cleanup: quarantine decided files, record evidence, restore or purge.

use crate::error::{EngineError, Result};
use crate::policy::SimulationReport;
use crate::quarantine::{self, QuarantineItem, QuarantineManifest};
use exor_storage::{
    ChangeKind, ChangeSource, NewLedgerEntry, PolicyAction, QuarantineRecord, QuarantineStatus,
    Store,
};
use std::path::{Path, PathBuf};

/// Outcome of [`quarantine_decided`].
#[derive(Debug)]
pub struct QuarantineOutcome {
    /// Manifest written.
    pub manifest: QuarantineManifest,
    /// Where it lives.
    pub manifest_path: PathBuf,
    /// Decided paths that were not regular files on disk (already gone, registry, service...).
    pub skipped: Vec<String>,
}

/// Record a finished quarantine in the store. If that fails, the files go straight back to their
/// places, so a quarantine never exists on disk without a record the app can restore it from.
pub(crate) fn record_or_undo(
    store: &Store,
    manifest: &QuarantineManifest,
    manifest_path: &Path,
    session_id: Option<i64>,
) -> Result<()> {
    let recorded = serde_json::to_string(manifest)
        .map_err(EngineError::from)
        .and_then(|json| {
            store
                .record_quarantine(&QuarantineRecord {
                    quarantine_id: manifest.quarantine_id.clone(),
                    rule_id: None,
                    session_id,
                    status: QuarantineStatus::Quarantined,
                    manifest_path: manifest_path.to_string_lossy().into_owned(),
                    manifest_json: json,
                })
                .map_err(EngineError::from)
        });
    let Err(e) = recorded else {
        return Ok(());
    };
    match quarantine::restore(manifest_path, false) {
        Ok(_) => Err(EngineError::Quarantine(format!(
            "could not record the quarantine ({e}); every file was put back"
        ))),
        Err(re) => Err(EngineError::Quarantine(format!(
            "could not record the quarantine ({e}) and putting the files back failed ({re}); \
             they are kept in {} and will be listed the next time ExorSystem starts",
            manifest_path.display()
        ))),
    }
}

/// Add a store record for every `quarantined` manifest under `quarantine_root` that has none
/// (left by a crash between moving the files and recording them), so it can be restored from
/// the app. Returns the ids imported.
pub fn reconcile_unrecorded(store: &Store, quarantine_root: &Path) -> Result<Vec<String>> {
    let known: std::collections::HashSet<String> = store
        .quarantines()?
        .into_iter()
        .map(|q| q.quarantine_id)
        .collect();
    let mut imported = Vec::new();
    for (mp, m) in quarantine::list_manifests(quarantine_root)? {
        if m.status != quarantine::ManifestStatus::Quarantined || known.contains(&m.quarantine_id) {
            continue;
        }
        store.record_quarantine(&QuarantineRecord {
            quarantine_id: m.quarantine_id.clone(),
            rule_id: None,
            session_id: None,
            status: QuarantineStatus::Quarantined,
            manifest_path: mp.to_string_lossy().into_owned(),
            manifest_json: serde_json::to_string(&m)?,
        })?;
        imported.push(m.quarantine_id);
    }
    Ok(imported)
}

/// Quarantine every file the simulation marked `quarantine`, then record the manifest and
/// ledger entries. Only regular files that exist are moved; the rest are reported in `skipped`.
pub fn quarantine_decided(
    store: &Store,
    quarantine_root: &Path,
    session_id: i64,
    report: &SimulationReport,
) -> Result<QuarantineOutcome> {
    let mut files = Vec::new();
    let mut skipped = Vec::new();
    for d in report
        .decisions
        .iter()
        .filter(|d| d.action == PolicyAction::Quarantine)
    {
        let p = PathBuf::from(&d.path);
        if p.is_file() {
            files.push(p)
        } else {
            skipped.push(d.path.clone())
        }
    }
    if files.is_empty() {
        return Err(EngineError::Quarantine(
            "no existing files are marked for quarantine".into(),
        ));
    }
    let (manifest, manifest_path) = quarantine::quarantine(quarantine_root, &files, &[])?;
    record_or_undo(store, &manifest, &manifest_path, Some(session_id))?;
    for it in &manifest.items {
        if let QuarantineItem::File {
            original_path,
            blake3,
            ..
        } = it
        {
            store.append_ledger(&NewLedgerEntry {
                session_id: Some(session_id),
                node_id: store.node_by_path(original_path)?.map(|n| n.node_id),
                path: original_path.clone(),
                change_kind: ChangeKind::Quarantined,
                old_hash: Some(blake3.clone()),
                new_hash: None,
                source: ChangeSource::Policy,
            })?;
        }
    }
    Ok(QuarantineOutcome {
        manifest,
        manifest_path,
        skipped,
    })
}

fn record_path(store: &Store, quarantine_id: &str) -> Result<QuarantineRecord> {
    store
        .quarantines()?
        .into_iter()
        .find(|q| q.quarantine_id == quarantine_id)
        .ok_or_else(|| EngineError::Quarantine(format!("unknown quarantine {quarantine_id}")))
}

/// Restore a quarantine by id and log `restored` in the ledger.
pub fn restore_by_id(
    store: &Store,
    quarantine_id: &str,
    overwrite: bool,
) -> Result<QuarantineManifest> {
    let rec = record_path(store, quarantine_id)?;
    let m = quarantine::restore(Path::new(&rec.manifest_path), overwrite)?;
    store.set_quarantine_status(quarantine_id, QuarantineStatus::Restored)?;
    for it in &m.items {
        if let QuarantineItem::File {
            original_path,
            blake3,
            ..
        } = it
        {
            store.append_ledger(&NewLedgerEntry {
                session_id: rec.session_id,
                node_id: store.node_by_path(original_path)?.map(|n| n.node_id),
                path: original_path.clone(),
                change_kind: ChangeKind::Restored,
                old_hash: None,
                new_hash: Some(blake3.clone()),
                source: ChangeSource::Policy,
            })?;
        }
    }
    Ok(m)
}

/// Permanently delete a quarantine's blobs; the manifest stays as evidence.
pub fn purge_by_id(store: &Store, quarantine_id: &str) -> Result<QuarantineManifest> {
    let rec = record_path(store, quarantine_id)?;
    let m = quarantine::purge(Path::new(&rec.manifest_path))?;
    store.set_quarantine_status(quarantine_id, QuarantineStatus::Purged)?;
    Ok(m)
}

/// Startup recovery: undo any quarantine interrupted by a crash. Returns the manifests rolled back.
pub fn recover_interrupted(quarantine_root: &Path) -> Result<Vec<PathBuf>> {
    let mut done = Vec::new();
    for mp in quarantine::find_incomplete(quarantine_root)? {
        quarantine::recover(&mp)?;
        done.push(mp);
    }
    Ok(done)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn file(d: &Path, name: &str, body: &[u8]) -> PathBuf {
        let p = d.join("app").join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn failed_record_puts_files_back() {
        let d = tempfile::tempdir().unwrap();
        let a = file(d.path(), "a.dll", b"aaa");
        let store = Store::open_in_memory().unwrap();
        let (m, mp) =
            quarantine::quarantine(&d.path().join("q"), std::slice::from_ref(&a), &[]).unwrap();
        assert!(!a.exists());
        record_or_undo(&store, &m, &mp, None).unwrap();
        // the same id again violates the primary key: the files must come back
        let err = record_or_undo(&store, &m, &mp, None).unwrap_err();
        assert!(err.to_string().contains("put back"), "{err}");
        assert_eq!(std::fs::read(&a).unwrap(), b"aaa");
        assert_eq!(
            quarantine::load_manifest(&mp).unwrap().status,
            quarantine::ManifestStatus::Restored
        );
    }

    #[test]
    fn unrecorded_manifests_are_imported_once() {
        let d = tempfile::tempdir().unwrap();
        let a = file(d.path(), "a.dll", b"aaa");
        let b = file(d.path(), "b.dll", b"bb");
        let q = d.path().join("q");
        let store = Store::open_in_memory().unwrap();
        // a crash after the move but before the record: manifest on disk, nothing in the store
        let (m, _) = quarantine::quarantine(&q, std::slice::from_ref(&a), &[]).unwrap();
        // a restored one must not be imported
        let (_, mp2) = quarantine::quarantine(&q, std::slice::from_ref(&b), &[]).unwrap();
        quarantine::restore(&mp2, false).unwrap();
        assert_eq!(
            reconcile_unrecorded(&store, &q).unwrap(),
            vec![m.quarantine_id.clone()]
        );
        assert!(reconcile_unrecorded(&store, &q).unwrap().is_empty());
        restore_by_id(&store, &m.quarantine_id, false).unwrap();
        assert_eq!(std::fs::read(&a).unwrap(), b"aaa");
    }
}
