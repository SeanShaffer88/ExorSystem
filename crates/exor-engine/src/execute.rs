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
    let record = QuarantineRecord {
        quarantine_id: manifest.quarantine_id.clone(),
        rule_id: None,
        session_id: Some(session_id),
        status: QuarantineStatus::Quarantined,
        manifest_path: manifest_path.to_string_lossy().into_owned(),
        manifest_json: serde_json::to_string(&manifest)?,
    };
    persist_record_or_restore(&manifest_path, || store.record_quarantine(&record))?;
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

/// Resolve a quarantine by its opaque id inside the trusted quarantine root.
///
/// The database and its stored paths live beside a portable executable and may be user-writable.
/// Never use the database's manifest path to locate restore or purge inputs.
fn trusted_manifest_path(root: &Path, quarantine_id: &str) -> Result<PathBuf> {
    let id_path = Path::new(quarantine_id);
    if id_path.components().count() != 1
        || !matches!(
            id_path.components().next(),
            Some(std::path::Component::Normal(_))
        )
    {
        return Err(EngineError::Quarantine("invalid quarantine id".to_string()));
    }
    let root = root.canonicalize().map_err(crate::error::io_err(root))?;
    let manifest_path = root.join(quarantine_id).join("manifest.json");
    if !manifest_path.is_file() {
        return Err(EngineError::Quarantine(
            "quarantine batch is missing from protected storage; refusing an untrusted manifest"
                .into(),
        ));
    }
    let manifest_path = manifest_path
        .canonicalize()
        .map_err(crate::error::io_err(&manifest_path))?;
    if !manifest_path.starts_with(&root) {
        return Err(EngineError::Quarantine(
            "manifest is outside protected quarantine storage".into(),
        ));
    }
    let manifest = quarantine::load_manifest(&manifest_path)?;
    if manifest.quarantine_id != quarantine_id {
        return Err(EngineError::Quarantine(
            "manifest id does not match the requested quarantine".into(),
        ));
    }
    Ok(manifest_path)
}

fn persist_record_or_restore(
    manifest_path: &Path,
    persist: impl FnOnce() -> exor_storage::Result<()>,
) -> Result<()> {
    if let Err(record_error) = persist() {
        return match quarantine::restore(manifest_path, false) {
            Ok(_) => Err(record_error.into()),
            Err(restore_error) => Err(EngineError::Quarantine(format!(
                "could not save quarantine record ({record_error}) and rollback failed ({restore_error}); data remains at {}",
                manifest_path.display()
            ))),
        };
    }
    Ok(())
}

/// Restore a quarantine by id and log `restored` in the ledger.
pub fn restore_by_id(
    store: &Store,
    quarantine_root: &Path,
    quarantine_id: &str,
    overwrite: bool,
) -> Result<QuarantineManifest> {
    let rec = record_path(store, quarantine_id)?;
    let manifest_path = trusted_manifest_path(quarantine_root, quarantine_id)?;
    let m = quarantine::restore(&manifest_path, overwrite)?;
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
pub fn purge_by_id(
    store: &Store,
    quarantine_root: &Path,
    quarantine_id: &str,
) -> Result<QuarantineManifest> {
    let rec = record_path(store, quarantine_id)?;
    let manifest_path = trusted_manifest_path(quarantine_root, quarantine_id)?;
    let m = quarantine::purge(&manifest_path)?;
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

    fn record_for(manifest: &QuarantineManifest, manifest_path: &Path) -> QuarantineRecord {
        QuarantineRecord {
            quarantine_id: manifest.quarantine_id.clone(),
            rule_id: None,
            session_id: None,
            status: QuarantineStatus::Quarantined,
            manifest_path: manifest_path.to_string_lossy().into_owned(),
            manifest_json: serde_json::to_string(manifest).unwrap(),
        }
    }

    #[test]
    fn restore_and_purge_reject_manifests_outside_the_trusted_root() {
        let dir = tempfile::tempdir().unwrap();
        let trusted_root = dir.path().join("protected");
        let untrusted_root = dir.path().join("portable-data");
        std::fs::create_dir_all(&trusted_root).unwrap();
        let original = dir.path().join("app").join("file.dll");
        std::fs::create_dir_all(original.parent().unwrap()).unwrap();
        std::fs::write(&original, b"content").unwrap();
        let (manifest, manifest_path) =
            quarantine::quarantine(&untrusted_root, &[original.clone()], &[]).unwrap();
        let store = Store::open_in_memory().unwrap();
        store
            .record_quarantine(&record_for(&manifest, &manifest_path))
            .unwrap();

        assert!(restore_by_id(&store, &trusted_root, &manifest.quarantine_id, false).is_err());
        assert!(purge_by_id(&store, &trusted_root, &manifest.quarantine_id).is_err());
        assert!(!original.exists(), "rejected batch remains quarantined");
        assert!(
            manifest_path
                .parent()
                .unwrap()
                .join("blobs")
                .join("0")
                .exists(),
            "rejected batch data is not deleted"
        );
    }

    #[test]
    fn failed_database_record_restores_quarantined_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("protected");
        let original = dir.path().join("app").join("file.dll");
        std::fs::create_dir_all(original.parent().unwrap()).unwrap();
        std::fs::write(&original, b"content").unwrap();
        let (manifest, manifest_path) =
            quarantine::quarantine(&root, std::slice::from_ref(&original), &[]).unwrap();

        let result = persist_record_or_restore(&manifest_path, || {
            Err(exor_storage::StorageError::NotFound(
                "simulated database failure".into(),
            ))
        });

        assert!(result.is_err());
        assert_eq!(std::fs::read(&original).unwrap(), b"content");
        assert_eq!(
            quarantine::load_manifest(&manifest_path).unwrap().status,
            crate::quarantine::ManifestStatus::Restored
        );
        assert_eq!(manifest.items.len(), 1);
    }

    #[test]
    fn restore_rejects_manifest_whose_id_does_not_match_the_record() {
        let dir = tempfile::tempdir().unwrap();
        let trusted_root = dir.path().join("protected");
        std::fs::create_dir_all(&trusted_root).unwrap();
        let original = dir.path().join("app").join("file.dll");
        std::fs::create_dir_all(original.parent().unwrap()).unwrap();
        std::fs::write(&original, b"content").unwrap();
        let (manifest, actual_manifest_path) =
            quarantine::quarantine(&trusted_root, &[original], &[]).unwrap();

        let forged_id = format!("x{}", manifest.quarantine_id);
        let forged_dir = trusted_root.join(&forged_id);
        std::fs::create_dir_all(&forged_dir).unwrap();
        let forged_manifest_path = forged_dir.join("manifest.json");
        std::fs::copy(&actual_manifest_path, &forged_manifest_path).unwrap();
        let mut forged_record = record_for(&manifest, &forged_manifest_path);
        forged_record.quarantine_id = forged_id.clone();
        let store = Store::open_in_memory().unwrap();
        store.record_quarantine(&forged_record).unwrap();

        assert!(restore_by_id(&store, &trusted_root, &forged_id, false).is_err());
        assert!(purge_by_id(&store, &trusted_root, &forged_id).is_err());
    }

    #[test]
    fn manifest_lookup_rejects_path_traversal_ids() {
        assert!(trusted_manifest_path(Path::new("."), "../outside").is_err());
    }
}
