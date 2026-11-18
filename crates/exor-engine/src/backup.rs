//! Versioned, content-verified profile backups. A backup is not mounted as an active profile
//! and rotated batches are never removed before a verified copy has been published.

use crate::error::{EngineError, Result, io_err};
use crate::hashing::hash_file_blocking;
use crate::publication::{Staging, sync_dir};
use crate::quarantine::{self, ManifestStatus, QuarantineItem};
use exor_storage::Store;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const FORMAT: &str = "exorsystem.profile-backup";
const MAX_FILES: usize = 40_000;
const MAX_BYTES: u64 = 64 * 1024 * 1024 * 1024;
const INDEX_LIMIT: u64 = 16 * 1024 * 1024;
const INDEX: &str = "exor-backup.json";

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Artifact {
    path: String,
    size: u64,
    blake3: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Index {
    format: String,
    version: u32,
    files: Vec<Artifact>,
}

/// Result for an independently verified backup or restored copy.
#[derive(Debug, Serialize)]
pub struct VerifiedCopy {
    /// Where the complete copy was written.
    pub path: String,
    /// Number of files included (SQLite and any recorded quarantine artifacts).
    pub files: usize,
    /// Logical bytes included, excluding the backup index.
    pub bytes: u64,
}

fn fail(message: impl Into<String>) -> EngineError {
    EngineError::Quarantine(message.into())
}

fn artifact_path(name: &str) -> Result<PathBuf> {
    let parts: Vec<_> = name.split('/').collect();
    match parts.as_slice() {
        ["exor.db"] => {}
        ["quarantine", id, "manifest.json"] if quarantine::valid_id(id) => {}
        ["quarantine", id, "blobs", blob]
            if quarantine::valid_id(id)
                && !blob.is_empty()
                && blob.len() <= 16
                && blob.bytes().all(|c| c.is_ascii_digit()) => {}
        _ => {
            return Err(fail(
                "Backup contains a path outside the supported profile layout.",
            ));
        }
    }
    Ok(parts.iter().collect())
}

fn open_artifact(path: &Path) -> Result<File> {
    if !path
        .parent()
        .is_some_and(|p| matches!(quarantine::plain_dirs(p), Ok(true)))
        || quarantine::plain(path, false)?.is_none()
    {
        return Err(fail(format!("{} is missing or redirected", path.display())));
    }
    #[cfg(windows)]
    {
        let _dirs = exor_fs::pin_parents(
            path.parent()
                .ok_or_else(|| fail("Missing artifact folder"))?,
        )
        .map_err(io_err(path))?;
        let file = exor_fs::open_locked(path, false).map_err(io_err(path))?;
        exor_fs::single_data_stream(&file).map_err(io_err(path))?;
        Ok(file)
    }
    #[cfg(not(windows))]
    File::open(path).map_err(io_err(path))
}

fn copy(src: &Path, dest: &Path, max: u64) -> Result<u64> {
    let input = open_artifact(src)?;
    let len = input.metadata().map_err(io_err(src))?.len();
    if len > max {
        return Err(fail(
            "Backup exceeds its byte limit. Archive a smaller set of held files first.",
        ));
    }
    let parent = dest
        .parent()
        .ok_or_else(|| fail("Missing backup directory"))?;
    fs::create_dir_all(parent).map_err(io_err(parent))?;
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(dest)
        .map_err(io_err(dest))?;
    let copied =
        std::io::copy(&mut input.take(len.saturating_add(1)), &mut output).map_err(io_err(dest))?;
    if copied != len {
        return Err(fail("A profile artifact changed while it was copied."));
    }
    output.sync_all().map_err(io_err(dest))?;
    sync_dir(parent)?;
    Ok(len)
}

fn checked_dir(dir: &Path) -> Result<PathBuf> {
    if !quarantine::plain_dirs(dir)? {
        return Err(fail(
            "Choose an existing local folder that is not redirected.",
        ));
    }
    dir.canonicalize().map_err(io_err(dir))
}

fn outside(profile: &Path, directory: &Path) -> Result<PathBuf> {
    let p = checked_dir(profile)?;
    let d = checked_dir(directory)?;
    if d.starts_with(p) {
        return Err(fail(
            "Choose a folder outside the active ExorSystem profile.",
        ));
    }
    Ok(d)
}

fn write_index(dir: &Path, files: Vec<Artifact>) -> Result<()> {
    let path = dir.join(INDEX);
    let index = Index {
        format: FORMAT.into(),
        version: 1,
        files,
    };
    let bytes = serde_json::to_vec_pretty(&index)?;
    if bytes.len() as u64 > INDEX_LIMIT {
        return Err(fail("Backup index exceeds its size limit."));
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .map_err(io_err(&path))?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(io_err(&path))?;
    Ok(())
}

fn verify(dir: &Path) -> Result<(Index, VerifiedCopy)> {
    let dir = checked_dir(dir)?;
    let path = dir.join(INDEX);
    let mut bytes = Vec::new();
    open_artifact(&path)?
        .take(INDEX_LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(io_err(&path))?;
    if bytes.len() as u64 > INDEX_LIMIT {
        return Err(fail("Backup index is too big."));
    }
    let index: Index = serde_json::from_slice(&bytes)?;
    if index.format != FORMAT || index.version != 1 || index.files.len() > MAX_FILES {
        return Err(fail("Unsupported backup format, version or file count."));
    }
    let mut names = BTreeMap::new();
    let mut total = 0_u64;
    for item in &index.files {
        let path = dir.join(artifact_path(&item.path)?);
        total = total.saturating_add(item.size);
        if total > MAX_BYTES || names.insert(item.path.as_str(), item).is_some() {
            return Err(fail(
                "Backup exceeds its verification budget or has duplicate paths.",
            ));
        }
        let mut input = open_artifact(&path)?;
        if input.metadata().map_err(io_err(&path))?.len() != item.size
            || crate::hashing::hash_reader(
                &mut (&mut input).take(item.size.saturating_add(1)),
                &path,
                None,
            )? != item.blake3
        {
            return Err(fail(format!(
                "Backup verification failed for {}.",
                item.path
            )));
        }
    }
    let db = dir.join("exor.db");
    if !names.contains_key("exor.db") {
        return Err(fail("Backup does not contain its database."));
    }
    for sidecar in ["exor.db-wal", "exor.db-shm", "exor.db-journal"] {
        if quarantine::plain(&dir.join(sidecar), false)?.is_some() {
            return Err(fail("Backup database has unindexed SQLite sidecars."));
        }
    }
    Store::verify_backup(&db)?;
    // A database cannot be accepted with a truncated held-file set, even if listed hashes pass.
    let store = Store::open_reader(&db)?;
    let root = dir.join("quarantine");
    for offset in (0..=MAX_FILES).step_by(100) {
        let batch = store.batch_summaries(100, offset)?;
        if batch.is_empty() {
            break;
        }
        for (i, row) in batch.into_iter().enumerate() {
            if offset + i >= MAX_FILES {
                return Err(fail(
                    "Backup database exceeds the batch verification budget.",
                ));
            }
            if matches!(row.status.as_str(), "quarantined" | "restoring" | "purging") {
                let id = row.quarantine_id;
                if !names.contains_key(format!("quarantine/{id}/manifest.json").as_str()) {
                    return Err(fail(format!("Backup is missing held batch {id}.")));
                }
                let manifest = crate::execute::verified_held_manifest(&store, &root, &id)?;
                for item in manifest.items {
                    if let QuarantineItem::File {
                        blob, blake3, size, ..
                    } = item
                    {
                        let expected = format!("quarantine/{id}/blobs/{blob}");
                        let check = names.get(expected.as_str());
                        if !check.is_some_and(|a| a.blake3 == blake3 && a.size == size) {
                            return Err(fail(format!(
                                "Backup is missing a verified held file in batch {id}."
                            )));
                        }
                    }
                }
            }
        }
    }
    let count = names.len();
    Ok((
        index,
        VerifiedCopy {
            path: dir.display().to_string(),
            files: count,
            bytes: total,
        },
    ))
}

/// Create a consistent SQLite snapshot with matching held files. Caller holds exclusive app job
/// access. Recovery conflicts must be reviewed before a backup can be certified complete.
pub fn create(store: &Store, profile: &Path, directory: &Path) -> Result<VerifiedCopy> {
    let directory = outside(profile, directory)?;
    let stage = Staging::new(&directory)?;
    let dest = stage.path();
    let db = dest.join("exor.db");
    store.backup_to(&db)?;
    let mut total = fs::metadata(&db).map_err(io_err(&db))?.len();
    if total > MAX_BYTES {
        return Err(fail("Database exceeds the backup budget."));
    }
    let mut files = vec![Artifact {
        path: "exor.db".into(),
        size: total,
        blake3: hash_file_blocking(&db)?,
    }];
    let root = profile.join("quarantine");
    // Refuse silent loss of interrupted/unindexed batches.
    for id in quarantine::batch_ids(&root)? {
        let path = quarantine::batch_path(&root, &id)?;
        let manifest = quarantine::load_manifest(&path)?;
        if matches!(manifest.status, ManifestStatus::RolledBack) {
            continue;
        }
        if store.quarantine_record(&id)?.is_none()
            || matches!(
                manifest.status,
                ManifestStatus::InProgress | ManifestStatus::Restoring | ManifestStatus::Purging
            )
        {
            return Err(fail(format!(
                "Batch {id} needs recovery before a complete backup can be verified."
            )));
        }
        let mut add = |rel: String, expected: Option<&str>| -> Result<()> {
            if files.len() >= MAX_FILES {
                return Err(fail("Backup reached its file count limit."));
            }
            let path = artifact_path(&rel)?;
            let target = dest.join(&path);
            let size = copy(
                &profile.join(path),
                &target,
                MAX_BYTES.saturating_sub(total),
            )?;
            let blake3 = hash_file_blocking(&target)?;
            if expected.is_some_and(|h| h != blake3) {
                return Err(fail("A held file did not match its quarantine manifest."));
            }
            total += size;
            files.push(Artifact {
                path: rel,
                size,
                blake3,
            });
            Ok(())
        };
        add(format!("quarantine/{id}/manifest.json"), None)?;
        if manifest.status == ManifestStatus::Quarantined {
            for item in manifest.items {
                if let QuarantineItem::File { blob, blake3, .. } = item {
                    add(format!("quarantine/{id}/blobs/{blob}"), Some(&blake3))?;
                }
            }
        }
    }
    write_index(dest, files)?;
    let (_, mut result) = verify(dest)?;
    result.path = stage
        .publish_directory("ExorSystem profile backup")?
        .display()
        .to_string();
    Ok(result)
}

/// Reverify the backup, make a new independent copy, and verify again before publication.
/// Never replaces the open database or any existing destination.
pub fn restore_copy(profile: &Path, backup: &Path, destination: &Path) -> Result<VerifiedCopy> {
    let backup = outside(profile, backup)?;
    let dest = outside(profile, destination)?;
    if dest.starts_with(&backup) {
        return Err(fail("Choose a destination outside the backup directory."));
    }
    let (index, _) = verify(&backup)?;
    let stage = Staging::new(&dest)?;
    for item in &index.files {
        let path = artifact_path(&item.path)?;
        copy(&backup.join(&path), &stage.path().join(path), item.size)?;
    }
    write_index(stage.path(), index.files)?;
    let (_, mut result) = verify(stage.path())?;
    result.path = stage
        .publish_directory("ExorSystem restored profile copy")?
        .display()
        .to_string();
    Ok(result)
}

/// Confirm a backup contains this exact (still held) manifest and every associated blob before
/// local retention is changed. This does not delete anything on its own.
pub fn verify_archived_batch(store: &Store, root: &Path, backup: &Path, id: &str) -> Result<()> {
    verify_archived_manifest(store, root, backup, id).map(|_| ())
}

pub(crate) fn verify_archived_manifest(
    store: &Store,
    root: &Path,
    backup: &Path,
    id: &str,
) -> Result<quarantine::QuarantineManifest> {
    verify(backup)?;
    let expected = crate::execute::verified_held_manifest(store, root, id)?;
    let actual =
        quarantine::load_manifest(&quarantine::batch_path(&backup.join("quarantine"), id)?)?;
    if expected != actual {
        return Err(fail(
            "The verified backup does not contain this exact held batch.",
        ));
    }
    let archived_store = Store::open_reader(&backup.join("exor.db"))?;
    if crate::execute::verified_held_manifest(&archived_store, &backup.join("quarantine"), id)?
        != expected
    {
        return Err(fail("Archived database does not confirm the batch."));
    }
    Ok(expected)
}
