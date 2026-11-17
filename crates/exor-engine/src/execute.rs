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
    store.record_quarantine(&QuarantineRecord {
        quarantine_id: manifest.quarantine_id.clone(),
        rule_id: None,
        session_id: Some(session_id),
        status: QuarantineStatus::Quarantined,
        manifest_path: manifest_path.to_string_lossy().into_owned(),
        manifest_json: serde_json::to_string(&manifest)?,
    })?;
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
