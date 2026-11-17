//! Evidence-preserving quarantine using compact JSON manifests (no VSS snapshots).
//!
//! Layout: `<root>/<id>/manifest.json` and `<root>/<id>/blobs/<index>`. Files are hashed
//! before anything moves; the manifest is written (atomically) as `in_progress` first, so an
//! interrupted run is recoverable. After restore or purge the manifest is kept as evidence.

use crate::error::{EngineError, Result, io_err};
use crate::hashing::hash_file_blocking;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Manifest lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestStatus {
    /// Moves under way (or interrupted).
    InProgress,
    /// All items held.
    Quarantined,
    /// Items returned to their original paths.
    Restored,
    /// Blobs deleted; manifest kept as evidence.
    Purged,
}

/// One quarantined item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QuarantineItem {
    /// A file moved into `blobs/`.
    File {
        /// Where it came from.
        original_path: String,
        /// Blob name under `blobs/`.
        blob: String,
        /// Size in bytes.
        size: u64,
        /// BLAKE3 hex of the content.
        blake3: String,
    },
    /// A registry export captured by the caller (restoring it is the caller's job).
    Registry {
        /// Key path.
        key_path: String,
        /// Serialised export (e.g. `.reg` text).
        export: String,
    },
}

/// Registry data to preserve alongside files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistryExport {
    /// Key path.
    pub key_path: String,
    /// Serialised export.
    pub export: String,
}

/// The on-disk manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuarantineManifest {
    /// Format version.
    pub version: u32,
    /// Opaque id (also the directory name).
    pub quarantine_id: String,
    /// Creation time, ms since epoch.
    pub created_unix_ms: u64,
    /// Lifecycle.
    pub status: ManifestStatus,
    /// Items.
    pub items: Vec<QuarantineItem>,
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0))
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let tmp = path.with_extension("json.tmp");
    let mut f = std::fs::File::create(&tmp).map_err(io_err(&tmp))?;
    f.write_all(bytes).map_err(io_err(&tmp))?;
    f.sync_all().map_err(io_err(&tmp))?;
    drop(f);
    std::fs::rename(&tmp, path).map_err(io_err(path))
}

fn write_manifest(path: &Path, m: &QuarantineManifest) -> Result<()> {
    atomic_write(path, &serde_json::to_vec_pretty(m)?)
}

/// Read a manifest.
pub fn load_manifest(path: &Path) -> Result<QuarantineManifest> {
    let bytes = std::fs::read(path).map_err(io_err(path))?;
    Ok(serde_json::from_slice(&bytes)?)
}

/// Move a file, falling back to copy + verify + delete across volumes.
fn move_file(src: &Path, dst: &Path, expect_hash: &str) -> Result<()> {
    if std::fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    std::fs::copy(src, dst).map_err(io_err(dst))?;
    if hash_file_blocking(dst)? != expect_hash {
        let _ = std::fs::remove_file(dst);
        return Err(EngineError::Quarantine(format!(
            "verification failed copying {}",
            src.display()
        )));
    }
    std::fs::remove_file(src).map_err(io_err(src))
}

/// Quarantine `files` (and keep `registry` exports) under `root`. Returns the manifest and its path.
/// All-or-nothing: on failure already moved files are returned to their places.
pub fn quarantine(
    root: &Path,
    files: &[PathBuf],
    registry: &[RegistryExport],
) -> Result<(QuarantineManifest, PathBuf)> {
    let mut items = Vec::new();
    for (i, f) in files.iter().enumerate() {
        let md = std::fs::metadata(f).map_err(io_err(f))?;
        if !md.is_file() {
            return Err(EngineError::Quarantine(format!(
                "{} is not a regular file",
                f.display()
            )));
        }
        items.push(QuarantineItem::File {
            original_path: f.to_string_lossy().into_owned(),
            blob: i.to_string(),
            size: md.len(),
            blake3: hash_file_blocking(f)?,
        });
    }
    for r in registry {
        items.push(QuarantineItem::Registry {
            key_path: r.key_path.clone(),
            export: r.export.clone(),
        });
    }
    let created = now_ms();
    let id = format!(
        "q{created:x}-{:04x}",
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let dir = root.join(&id);
    let blobs = dir.join("blobs");
    std::fs::create_dir_all(&blobs).map_err(io_err(&blobs))?;
    let manifest_path = dir.join("manifest.json");
    let mut m = QuarantineManifest {
        version: 1,
        quarantine_id: id,
        created_unix_ms: created,
        status: ManifestStatus::InProgress,
        items,
    };
    write_manifest(&manifest_path, &m)?;

    let mut moved: Vec<(PathBuf, PathBuf, String)> = Vec::new();
    let mut failure: Option<EngineError> = None;
    for it in &m.items {
        if let QuarantineItem::File {
            original_path,
            blob,
            blake3,
            ..
        } = it
        {
            let (src, dst) = (PathBuf::from(original_path), blobs.join(blob));
            match move_file(&src, &dst, blake3) {
                Ok(()) => moved.push((src, dst, blake3.clone())),
                Err(e) => {
                    failure = Some(e);
                    break;
                }
            }
        }
    }
    if let Some(e) = failure {
        for (src, dst, h) in moved.iter().rev() {
            if let Err(re) = move_file(dst, src, h) {
                return Err(EngineError::Quarantine(format!(
                    "{e}; rollback also failed: {re}; see {}",
                    manifest_path.display()
                )));
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
        return Err(e);
    }
    m.status = ManifestStatus::Quarantined;
    write_manifest(&manifest_path, &m)?;
    Ok((m, manifest_path))
}

/// Restore files to their original paths. Verifies every blob first and refuses to overwrite
/// existing files unless `overwrite`. Registry items are returned in the manifest for the caller.
pub fn restore(manifest_path: &Path, overwrite: bool) -> Result<QuarantineManifest> {
    let mut m = load_manifest(manifest_path)?;
    if m.status != ManifestStatus::Quarantined {
        return Err(EngineError::Quarantine(format!(
            "cannot restore a manifest in state {:?}",
            m.status
        )));
    }
    let blobs = manifest_path
        .parent()
        .map(|p| p.join("blobs"))
        .ok_or_else(|| EngineError::Quarantine("manifest has no parent".into()))?;
    for it in &m.items {
        if let QuarantineItem::File {
            original_path,
            blob,
            blake3,
            ..
        } = it
        {
            let b = blobs.join(blob);
            if hash_file_blocking(&b)? != *blake3 {
                return Err(EngineError::Quarantine(format!("blob {blob} is corrupt")));
            }
            if !overwrite && Path::new(original_path).exists() {
                return Err(EngineError::Quarantine(format!(
                    "{original_path} already exists"
                )));
            }
        }
    }
    for it in &m.items {
        if let QuarantineItem::File {
            original_path,
            blob,
            blake3,
            ..
        } = it
        {
            let dst = Path::new(original_path);
            if let Some(parent) = dst.parent() {
                std::fs::create_dir_all(parent).map_err(io_err(parent))?;
            }
            if overwrite && dst.exists() {
                std::fs::remove_file(dst).map_err(io_err(dst))?;
            }
            move_file(&blobs.join(blob), dst, blake3)?;
        }
    }
    m.status = ManifestStatus::Restored;
    write_manifest(manifest_path, &m)?;
    Ok(m)
}

/// Delete the blobs permanently; the manifest stays as evidence.
pub fn purge(manifest_path: &Path) -> Result<QuarantineManifest> {
    let mut m = load_manifest(manifest_path)?;
    if m.status != ManifestStatus::Quarantined {
        return Err(EngineError::Quarantine(format!(
            "cannot purge a manifest in state {:?}",
            m.status
        )));
    }
    if let Some(parent) = manifest_path.parent() {
        let blobs = parent.join("blobs");
        if blobs.exists() {
            std::fs::remove_dir_all(&blobs).map_err(io_err(&blobs))?;
        }
    }
    m.status = ManifestStatus::Purged;
    write_manifest(manifest_path, &m)?;
    Ok(m)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let a = d.path().join("app").join("a.dll");
        let b = d.path().join("app").join("b.dll");
        std::fs::create_dir_all(a.parent().unwrap()).unwrap();
        std::fs::write(&a, b"aaa").unwrap();
        std::fs::write(&b, b"bbbb").unwrap();
        (d, a, b)
    }

    #[test]
    fn quarantine_then_restore_roundtrip() {
        let (d, a, b) = setup();
        let q = d.path().join("q");
        let reg = [RegistryExport {
            key_path: "HKLM\\SOFTWARE\\X".into(),
            export: "[HKLM\\SOFTWARE\\X]".into(),
        }];
        let (m, mp) = quarantine(&q, &[a.clone(), b.clone()], &reg).unwrap();
        assert_eq!(m.status, ManifestStatus::Quarantined);
        assert_eq!(m.items.len(), 3);
        assert!(!a.exists() && !b.exists());
        let r = restore(&mp, false).unwrap();
        assert_eq!(r.status, ManifestStatus::Restored);
        assert_eq!(std::fs::read(&a).unwrap(), b"aaa");
        assert_eq!(std::fs::read(&b).unwrap(), b"bbbb");
        assert!(mp.exists(), "manifest kept as evidence");
        assert!(restore(&mp, false).is_err(), "cannot restore twice");
    }

    #[test]
    fn restore_refuses_overwrite_and_detects_corruption() {
        let (d, a, _b) = setup();
        let q = d.path().join("q");
        let (_m, mp) = quarantine(&q, std::slice::from_ref(&a), &[]).unwrap();
        std::fs::write(&a, b"new").unwrap();
        assert!(restore(&mp, false).is_err());
        assert_eq!(std::fs::read(&a).unwrap(), b"new");
        assert_eq!(restore(&mp, true).unwrap().status, ManifestStatus::Restored);
        assert_eq!(std::fs::read(&a).unwrap(), b"aaa");

        let (_m2, mp2) = quarantine(&q, std::slice::from_ref(&a), &[]).unwrap();
        std::fs::write(mp2.parent().unwrap().join("blobs").join("0"), b"tampered").unwrap();
        assert!(restore(&mp2, true).is_err());
    }

    #[test]
    fn failed_quarantine_rolls_back() {
        let (d, a, _b) = setup();
        let missing = d.path().join("app").join("gone.dll");
        let q = d.path().join("q");
        assert!(quarantine(&q, &[a.clone(), missing], &[]).is_err());
        assert!(a.exists(), "nothing moved when a file is unreadable");
    }

    #[test]
    fn purge_removes_blobs_keeps_manifest() {
        let (d, a, _b) = setup();
        let q = d.path().join("q");
        let (_m, mp) = quarantine(&q, std::slice::from_ref(&a), &[]).unwrap();
        let m = purge(&mp).unwrap();
        assert_eq!(m.status, ManifestStatus::Purged);
        assert!(!mp.parent().unwrap().join("blobs").exists());
        assert!(load_manifest(&mp).is_ok());
        assert!(restore(&mp, true).is_err());
    }
}
