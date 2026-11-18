//! Execute a cleanup with one recoverable protocol for policy and leftover batches.
//!
//! Stage the files with an in-progress manifest, atomically record the index and ledger in
//! SQLite, then finalize the manifest. Startup uses the presence of a matching database
//! record to distinguish finishing a commit from rolling an uncommitted batch back.

use crate::error::{EngineError, Result};
use crate::policy::SimulationReport;
use crate::quarantine::{self, ManifestStatus, QuarantineItem, QuarantineManifest};
use exor_storage::{
    ChangeKind, ChangeSource, NewLedgerEntry, NodeType, PolicyAction, QuarantineRecord,
    QuarantineStatus, Store,
};
use serde::Serialize;
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

/// Quarantine every existing file marked `quarantine`. Other decisions are reported as skipped.
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
        // A registry key is not a filesystem path, even if a file has the same spelling.
        if d.node_type == NodeType::File && Path::new(&d.path).is_file() {
            files.push(PathBuf::from(&d.path));
        } else {
            skipped.push(d.path.clone());
        }
    }
    if files.is_empty() {
        return Err(EngineError::Quarantine(
            "no existing files are marked for quarantine".into(),
        ));
    }
    let (manifest, manifest_path) = record_files(store, quarantine_root, Some(session_id), &files)?;
    Ok(QuarantineOutcome {
        manifest,
        manifest_path,
        skipped,
    })
}

fn ledger(
    store: &Store,
    session_id: Option<i64>,
    manifest: &QuarantineManifest,
    restoring: bool,
) -> Result<Vec<NewLedgerEntry>> {
    let mut entries = Vec::new();
    for item in &manifest.items {
        if let QuarantineItem::File {
            original_path,
            blake3,
            ..
        } = item
        {
            entries.push(NewLedgerEntry {
                session_id,
                node_id: store.node_by_path(original_path)?.map(|n| n.node_id),
                path: original_path.clone(),
                change_kind: if restoring {
                    ChangeKind::Restored
                } else {
                    ChangeKind::Quarantined
                },
                old_hash: (!restoring).then(|| blake3.clone()),
                new_hash: restoring.then(|| blake3.clone()),
                source: ChangeSource::Policy,
            });
        }
    }
    Ok(entries)
}

fn commit_record(
    store: &Store,
    session_id: Option<i64>,
    m: &QuarantineManifest,
    path: &Path,
) -> Result<()> {
    let mut committed = m.clone();
    committed.status = ManifestStatus::Quarantined;
    let report = QuarantineRecord {
        quarantine_id: m.quarantine_id.clone(),
        rule_id: None,
        session_id,
        status: QuarantineStatus::Quarantined,
        manifest_path: path.to_string_lossy().into_owned(),
        manifest_json: serde_json::to_string(&committed)?,
    };
    store.record_quarantine_with_ledger(&report, &ledger(store, session_id, m, false)?)?;
    Ok(())
}

pub(crate) fn record_files(
    store: &Store,
    root: &Path,
    session_id: Option<i64>,
    files: &[PathBuf],
) -> Result<(QuarantineManifest, PathBuf)> {
    let (mut m, path) = quarantine::stage_files(root, files, &[])?;
    if let Err(e) = commit_record(store, session_id, &m, &path) {
        return Err(quarantine::rollback_after(&path, &e));
    }
    if let Err(e) = quarantine::finish_stage(&path, &mut m) {
        return Err(EngineError::Quarantine(format!(
            "Batch {} and its history were saved, but the manifest could not be finalized: {e}. Retry from Quarantine or restart ExorSystem; held files were kept.",
            m.quarantine_id
        )));
    }
    Ok((m, path))
}

fn check_record(rec: &QuarantineRecord, m: &QuarantineManifest) -> Result<()> {
    let mut expected: QuarantineManifest = serde_json::from_str(&rec.manifest_json)?;
    expected.status = m.status;
    if expected != *m {
        return Err(EngineError::Quarantine(format!(
            "batch {} does not match its recorded manifest; held files were kept",
            rec.quarantine_id
        )));
    }
    Ok(())
}

fn checked_batch(
    store: &Store,
    root: &Path,
    id: &str,
) -> Result<(QuarantineRecord, QuarantineManifest, PathBuf)> {
    let path = quarantine::batch_path(root, id)?;
    let rec = store
        .quarantine_record(id)?
        .ok_or_else(|| EngineError::Quarantine(format!("unknown quarantine {id}")))?;
    let mut m = quarantine::load_manifest(&path)?;
    check_record(&rec, &m)?;
    if rec.status == QuarantineStatus::Quarantined && m.status == ManifestStatus::InProgress {
        quarantine::finish_stage(&path, &mut m)?;
    }
    Ok((rec, m, path))
}

/// Original path and logical file size from the recorded manifest, not a live file probe.
#[derive(Debug, Serialize)]
pub struct BatchFile {
    /// Where the file originally lived.
    pub original_path: String,
    /// Logical bytes at move time.
    pub size: u64,
}

/// One page of the local database's batch record. No quarantined blobs are read or modified.
#[derive(Debug, Serialize)]
pub struct BatchPreview {
    /// Database lifecycle state (not a live restore eligibility check).
    pub status: QuarantineStatus,
    /// Creation time stored in the manifest.
    pub created_unix_ms: u64,
    /// Number of files recorded in the batch.
    pub total_files: usize,
    /// Requested page of recorded file paths and lengths; no registry export data.
    pub files: Vec<BatchFile>,
    /// Offset of the next page if another exists.
    pub next_offset: Option<usize>,
}

/// Inspect the manifest copy in SQLite. Do not resolve, validate against disk or finalize the
/// active manifest here; only the restore and purge commands may perform those operations.
pub fn preview_batch(store: &Store, id: &str, offset: usize) -> Result<BatchPreview> {
    const PAGE_SIZE: usize = 24;
    let record = store
        .quarantine_record(id)?
        .ok_or_else(|| EngineError::Quarantine("unknown quarantine batch".into()))?;
    let manifest: QuarantineManifest = serde_json::from_str(&record.manifest_json)?;
    if manifest.quarantine_id != record.quarantine_id || manifest.version != 1 {
        return Err(EngineError::Quarantine(
            "unsupported or mismatched recorded manifest".into(),
        ));
    }
    let total_files = manifest
        .items
        .iter()
        .filter(|item| matches!(item, QuarantineItem::File { .. }))
        .count();
    let files = manifest
        .items
        .into_iter()
        .filter_map(|item| match item {
            QuarantineItem::File {
                original_path,
                size,
                ..
            } => Some(BatchFile {
                original_path,
                size,
            }),
            QuarantineItem::Registry { .. } => None,
        })
        .skip(offset)
        .take(PAGE_SIZE)
        .collect::<Vec<_>>();
    let next = offset.saturating_add(files.len());
    Ok(BatchPreview {
        status: record.status,
        created_unix_ms: manifest.created_unix_ms,
        total_files,
        files,
        next_offset: (next < total_files).then_some(next),
    })
}

/// Restore a named batch without overwrite and commit its status and ledger together. A retry
/// after a database write failure can reconcile an already finished disk restore.
pub fn restore_by_id(store: &Store, root: &Path, id: &str) -> Result<QuarantineManifest> {
    let (rec, mut m, path) = checked_batch(store, root, id)?;
    if rec.status == QuarantineStatus::Purged {
        return Err(EngineError::Quarantine(
            "cannot restore a purged batch".into(),
        ));
    }
    if rec.status != QuarantineStatus::Restored {
        let entries = ledger(store, rec.session_id, &m, true)?;
        if m.status != ManifestStatus::Restored {
            m = quarantine::restore(&path)?;
        }
        store.set_quarantine_status_with_ledger(id, QuarantineStatus::Restored, &entries)?;
    }
    Ok(m)
}

/// Permanently delete a named batch's blobs. The manifest stays as evidence.
pub fn purge_by_id(store: &Store, root: &Path, id: &str) -> Result<QuarantineManifest> {
    let (rec, mut m, path) = checked_batch(store, root, id)?;
    if rec.status == QuarantineStatus::Restored {
        return Err(EngineError::Quarantine(
            "cannot purge a restored batch".into(),
        ));
    }
    if rec.status != QuarantineStatus::Purged {
        if m.status != ManifestStatus::Purged {
            m = quarantine::purge(&path)?;
        }
        store.set_quarantine_status(id, QuarantineStatus::Purged)?;
    }
    Ok(m)
}

/// Startup recovery reports problems per batch; a corrupt batch cannot block unrelated batches.
#[derive(Debug, Default)]
pub struct RecoveryReport {
    /// Previously uncommitted batches put back.
    pub rolled_back: usize,
    /// Batches whose SQLite commit was already complete and whose manifest was finalized.
    pub finalized: usize,
    /// Manifests that could not be safely processed, with an explanation.
    pub warnings: Vec<String>,
}

/// Recover only validated batches under the current profile's quarantine root.
pub fn recover_interrupted(store: &Store, root: &Path) -> Result<RecoveryReport> {
    let mut report = RecoveryReport::default();
    for id in quarantine::batch_ids(root)? {
        let result = (|| {
            let path = quarantine::batch_path(root, &id)?;
            let mut m = quarantine::load_manifest(&path)?;
            if m.status != ManifestStatus::InProgress {
                return Ok::<_, EngineError>(());
            }
            match store.quarantine_record(&id)? {
                None => {
                    quarantine::recover(&path)?;
                    report.rolled_back += 1;
                }
                Some(rec) if rec.status == QuarantineStatus::Quarantined => {
                    check_record(&rec, &m)?;
                    quarantine::finish_stage(&path, &mut m)?;
                    report.finalized += 1;
                }
                Some(_) => {
                    return Err(EngineError::Quarantine(
                        "unexpected database state; held files were kept".into(),
                    ));
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            report.warnings.push(format!("Batch {id}: {e}"));
        }
    }
    Ok(report)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn startup_reconciles_both_sides_of_an_interrupted_commit() {
        let d = tempfile::tempdir().unwrap();
        let root = d.path().join("q");
        let a = d.path().join("a.txt");
        let b = d.path().join("b.txt");
        fs::write(&a, b"uncommitted").unwrap();
        fs::write(&b, b"committed").unwrap();
        let store = Store::open_in_memory().unwrap();
        let (uncommitted, _) =
            quarantine::stage_files(&root, std::slice::from_ref(&a), &[]).unwrap();
        let (committed, mp) =
            quarantine::stage_files(&root, std::slice::from_ref(&b), &[]).unwrap();
        commit_record(&store, None, &committed, &mp).unwrap();
        assert!(!a.exists() && !b.exists());

        let recovery = recover_interrupted(&store, &root).unwrap();
        assert_eq!((recovery.rolled_back, recovery.finalized), (1, 1));
        assert!(recovery.warnings.is_empty());
        assert_eq!(fs::read(&a).unwrap(), b"uncommitted");
        assert!(!b.exists());
        assert!(
            store
                .quarantine_record(&uncommitted.quarantine_id)
                .unwrap()
                .is_none()
        );
        assert_eq!(
            quarantine::load_manifest(&mp).unwrap().status,
            ManifestStatus::Quarantined
        );
        restore_by_id(&store, &root, &committed.quarantine_id).unwrap();
        assert_eq!(fs::read(&b).unwrap(), b"committed");
    }

    #[test]
    fn database_paths_and_unknown_batches_are_not_restore_authority() {
        let d = tempfile::tempdir().unwrap();
        let outside = d.path().join("outside");
        let root = d.path().join("q");
        fs::create_dir(&root).unwrap();
        let file = d.path().join("personal.txt");
        fs::write(&file, b"personal").unwrap();
        let store = Store::open_in_memory().unwrap();
        let (m, mp) = quarantine::quarantine(&outside, std::slice::from_ref(&file), &[]).unwrap();
        commit_record(&store, None, &m, &mp).unwrap();
        assert!(restore_by_id(&store, &root, &m.quarantine_id).is_err());
        assert!(purge_by_id(&store, &root, &m.quarantine_id).is_err());
        assert!(restore_by_id(&store, &root, "../outside/q1-0000").is_err());
        assert!(!file.exists());
        assert!(mp.with_file_name("blobs").join("0").is_file());

        fs::rename(mp.parent().unwrap(), root.join(&m.quarantine_id)).unwrap();
        let unrelated_store = Store::open_in_memory().unwrap();
        assert!(restore_by_id(&unrelated_store, &root, &m.quarantine_id).is_err());
        assert!(!file.exists());
        restore_by_id(&store, &root, &m.quarantine_id).unwrap();
        assert_eq!(fs::read(&file).unwrap(), b"personal");
    }

    #[cfg(windows)]
    fn link_dir(src: &Path, link: &Path) {
        let output = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", "$ErrorActionPreference='Stop'; New-Item -ItemType Junction -Path $env:EXOR_TEST_LINK -Target $env:EXOR_TEST_TARGET | Out-Null"])
            .env("EXOR_TEST_LINK", link)
            .env("EXOR_TEST_TARGET", src)
            .output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[cfg(unix)]
    fn link_dir(src: &Path, link: &Path) {
        std::os::unix::fs::symlink(src, link).unwrap();
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn restore_purge_and_startup_refuse_redirected_batches_and_storage() {
        let d = tempfile::tempdir().unwrap();
        let outside = d.path().join("outside");
        let profile = d.path().join("q");
        let profile_alias = d.path().join("linked-profile");
        let a = d.path().join("fixture.txt");
        fs::write(&a, b"fixture").unwrap();
        let store = Store::open_in_memory().unwrap();
        let (staged, mp) =
            quarantine::stage_files(&outside, std::slice::from_ref(&a), &[]).unwrap();
        commit_record(&store, None, &staged, &mp).unwrap();
        fs::create_dir(&profile).unwrap();
        link_dir(mp.parent().unwrap(), &profile.join(&staged.quarantine_id));
        assert!(restore_by_id(&store, &profile, &staged.quarantine_id).is_err());
        assert!(purge_by_id(&store, &profile, &staged.quarantine_id).is_err());
        let recovery = recover_interrupted(&store, &profile).unwrap();
        assert_eq!(recovery.warnings.len(), 1);
        assert_eq!(recovery.finalized, 0);
        assert!(!a.exists());
        assert_eq!(
            quarantine::load_manifest(&mp).unwrap().status,
            ManifestStatus::InProgress
        );
        assert!(mp.with_file_name("blobs").join("0").is_file());
        link_dir(&profile, &profile_alias);
        assert!(recover_interrupted(&store, &profile_alias).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn a_sharing_violation_rolls_back_earlier_files_in_the_batch() {
        use std::os::windows::fs::OpenOptionsExt;
        let d = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let a = d.path().join("a.txt");
        let b = d.path().join("locked.txt");
        fs::write(&a, b"first").unwrap();
        fs::write(&b, b"locked").unwrap();
        // Allow reads for hashing but not deletion while the handle is open.
        let lock = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&b)
            .unwrap();
        assert!(record_files(&store, &d.path().join("q"), None, &[a.clone(), b.clone()]).is_err());
        drop(lock);
        assert_eq!(fs::read(a).unwrap(), b"first");
        assert_eq!(fs::read(b).unwrap(), b"locked");
        assert!(store.quarantines().unwrap().is_empty());
        assert!(store.ledger_recent(10, None).unwrap().is_empty());
    }
}
