//! Recoverable, file-only quarantine. A database record is an index, not path authority.
//! Manifests and blobs are local files below the configured root; no action follows a link.

use crate::error::{EngineError, Result, io_err};
use crate::hashing::hash_file_blocking;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::{self, Metadata, OpenOptions};
use std::io::{ErrorKind, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

/// Quarantine manifest lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestStatus {
    /// Written before moving anything; the SQLite record decides how startup reconciles it.
    InProgress,
    /// Files are in quarantine and can be restored or purged.
    Quarantined,
    /// Files were restored.
    Restored,
    /// Blobs were permanently deleted.
    Purged,
    /// An uncommitted batch was rolled back.
    RolledBack,
}

/// One manifest item. Registry exports are evidence only and are never applied.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum QuarantineItem {
    /// A file held in the batch's blobs directory.
    File {
        /// Original absolute path.
        original_path: String,
        /// Numeric name within `blobs/`, never a path.
        blob: String,
        /// File length when recorded.
        size: u64,
        /// BLAKE3 of the default data stream.
        blake3: String,
    },
    /// Read-only registry export.
    Registry {
        /// Key that was observed.
        key_path: String,
        /// Export text, not an instruction to apply it.
        export: String,
    },
}

/// An optional registry export to preserve as evidence.
#[derive(Debug, Clone)]
pub struct RegistryExport {
    /// Key path.
    pub key_path: String,
    /// Export text.
    pub export: String,
}

/// Disk manifest, retained after restore or purge as evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuarantineManifest {
    /// Disk format version.
    pub version: u32,
    /// Batch identifier, also the batch directory name.
    pub quarantine_id: String,
    /// Creation timestamp in Unix milliseconds.
    pub created_unix_ms: u64,
    /// Current disk state.
    pub status: ManifestStatus,
    /// Files and optional advisory evidence.
    pub items: Vec<QuarantineItem>,
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(0))
}

fn next_name(created: u64) -> String {
    format!(
        "q{created:x}-{:04x}",
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

fn valid_id(id: &str) -> bool {
    let lower_hex = |s: &str| {
        !s.is_empty() && s.len() <= 16 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
    };
    id.strip_prefix('q')
        .and_then(|s| s.split_once('-'))
        .is_some_and(|(time, counter)| lower_hex(time) && counter.len() >= 4 && lower_hex(counter))
}

fn blocked(path: &Path, why: &str) -> EngineError {
    EngineError::Quarantine(format!("{}: {why}", path.display()))
}

fn is_link(meta: &Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.is_symlink()
    }
}

fn local_path(path: &Path) -> Result<()> {
    let mut parts = path.components();
    #[cfg(windows)]
    if !matches!(parts.next(), Some(Component::Prefix(p)) if matches!(p.kind(), std::path::Prefix::Disk(_) | std::path::Prefix::VerbatimDisk(_)))
    {
        return Err(blocked(path, "expected a local drive path"));
    }
    if !matches!(parts.next(), Some(Component::RootDir))
        || !parts.all(|part| match part {
            Component::Normal(name) => name.to_str().is_some_and(|name| {
                !name.contains('\0')
                    && (!cfg!(windows)
                        || (!name.contains([':', '/', '\\'])
                            && !name.ends_with(['.', ' '])
                            && name != ".."))
            }),
            _ => false,
        })
    {
        return Err(blocked(path, "expected an absolute, unambiguous path"));
    }
    Ok(())
}

/// Missing is handled separately; a reparse point, symlink or wrong type is always an error.
fn plain(path: &Path, dir: bool) -> Result<Option<Metadata>> {
    match fs::symlink_metadata(path) {
        Ok(meta) if !is_link(&meta) && if dir { meta.is_dir() } else { meta.is_file() } => {
            Ok(Some(meta))
        }
        Ok(_) => Err(blocked(
            path,
            if dir {
                "not a plain directory"
            } else {
                "not a plain file"
            },
        )),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_err(path)(e)),
    }
}

fn plain_dirs(path: &Path) -> Result<bool> {
    local_path(path)?;
    let mut ancestors: Vec<_> = path.ancestors().collect();
    while let Some(dir) = ancestors.pop() {
        if plain(dir, true)?.is_none() {
            return Ok(false);
        }
    }
    Ok(true)
}

fn path_key(path: &Path) -> String {
    let s = path.to_string_lossy();
    if cfg!(windows) {
        s.strip_prefix(r"\\?\")
            .unwrap_or(&s)
            .replace('/', "\\")
            .to_ascii_lowercase()
    } else {
        s.into_owned()
    }
}

fn original_path(root: &Path, original: &Path) -> Result<()> {
    local_path(original)?;
    let parent = original
        .parent()
        .ok_or_else(|| blocked(original, "missing parent directory"))?;
    // Check existing parents before canonicalizing. Reject aliases into the quarantine itself,
    // including alternate Windows spellings of a directory that already exists.
    plain_dirs(parent)?;
    let key = path_key(root);
    if original.ancestors().any(|p| path_key(p) == key)
        || root.canonicalize().ok().is_some_and(|real_root| {
            let key = path_key(&real_root);
            parent
                .ancestors()
                .find_map(|p| p.canonicalize().ok())
                .is_some_and(|p| p.ancestors().any(|p| path_key(p) == key))
        })
    {
        return Err(blocked(
            original,
            "original path is inside quarantine storage",
        ));
    }
    Ok(())
}

/// Derive the manifest location from the configured root, never from the database.
pub(crate) fn batch_path(root: &Path, id: &str) -> Result<PathBuf> {
    if !valid_id(id) {
        return Err(EngineError::Quarantine("invalid quarantine ID".into()));
    }
    if !plain_dirs(root)? {
        return Err(blocked(root, "quarantine directory is missing"));
    }
    let batch = root.join(id);
    if plain(&batch, true)?.is_none() {
        return Err(blocked(&batch, "quarantine batch is missing"));
    }
    let mp = batch.join("manifest.json");
    if plain(&mp, false)?.is_none() {
        return Err(blocked(&mp, "manifest is missing"));
    }
    plain(&batch.join("blobs"), true)?;
    Ok(mp)
}

fn root_and_id(path: &Path) -> Result<(&Path, &str)> {
    if path.file_name().is_none_or(|name| name != "manifest.json") {
        return Err(blocked(path, "expected manifest.json"));
    }
    let batch = path
        .parent()
        .ok_or_else(|| blocked(path, "manifest has no batch"))?;
    let id = batch
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| blocked(batch, "missing batch ID"))?;
    let root = batch
        .parent()
        .ok_or_else(|| blocked(batch, "missing quarantine root"))?;
    Ok((root, id))
}

/// Load a batch after checking its location, ID, and contained blob names. Original destinations
/// are checked separately immediately before any restore or rollback writes.
pub fn load_manifest(path: &Path) -> Result<QuarantineManifest> {
    let (root, id) = root_and_id(path)?;
    let path = batch_path(root, id)?;
    let bytes = fs::read(&path).map_err(io_err(&path))?;
    let m: QuarantineManifest = serde_json::from_slice(&bytes)?;
    if m.version != 1 || m.quarantine_id != id || m.items.is_empty() {
        return Err(blocked(
            &path,
            "unexpected manifest version, batch ID or item list",
        ));
    }
    let mut seen = BTreeSet::new();
    for (index, item) in m.items.iter().enumerate() {
        if let QuarantineItem::File {
            original_path,
            blob,
            blake3,
            ..
        } = item
        {
            let dst = Path::new(original_path);
            local_path(dst)?;
            if blob != &index.to_string()
                || !seen.insert(path_key(dst))
                || blake3.len() != 64
                || !blake3.bytes().all(|b| b.is_ascii_hexdigit())
            {
                return Err(blocked(
                    &path,
                    "invalid blob name, duplicate original or invalid hash",
                ));
            }
        }
    }
    Ok(m)
}

fn write_manifest(path: &Path, m: &QuarantineManifest) -> Result<()> {
    let mut temp = path.to_path_buf();
    temp.set_file_name(format!("{}.tmp", next_name(now_ms())));
    let bytes = serde_json::to_vec(m)?;
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(io_err(&temp))?;
    let result = f.write_all(&bytes).and_then(|()| f.sync_all());
    drop(f);
    let result = result.and_then(|()| fs::rename(&temp, path));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(io_err(path))
}

/// Publish with no replacement. A hard link gives create-if-absent semantics and preserves
/// the file on NTFS. Across volumes, verify a copy in a newly reserved scratch directory
/// before publishing that copy with the same semantics. Filesystems without hard links fail
/// closed. After a crash between publishing and unlinking, both copies may be present.
fn move_file(src: &Path, dst: &Path, expected: &str) -> Result<()> {
    if plain(src, false)?.is_none() || hash_file_blocking(src)? != expected {
        return Err(blocked(src, "missing file or content changed"));
    }
    match fs::hard_link(src, dst) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::CrossesDevices => {
            let dir = dst
                .parent()
                .ok_or_else(|| blocked(dst, "no destination directory"))?;
            let scratch = dir.join(format!(".exor-copy-{}", next_name(now_ms())));
            fs::create_dir(&scratch).map_err(io_err(&scratch))?;
            let copy = scratch.join("copy");
            let result = (|| {
                fs::copy(src, &copy).map_err(io_err(&copy))?;
                if hash_file_blocking(&copy)? != expected {
                    return Err(blocked(&copy, "copy verification failed"));
                }
                fs::hard_link(&copy, dst).map_err(io_err(dst))
            })();
            let _ = fs::remove_file(&copy);
            let _ = fs::remove_dir(&scratch);
            result?;
        }
        Err(e) => return Err(io_err(dst)(e)),
    }
    if let Err(e) = fs::remove_file(src) {
        // Keep the just-published copy if cleanup fails; recovery will report the conflict
        // rather than deleting a copy or overwriting the original.
        let _ = fs::remove_file(dst);
        return Err(io_err(src)(e));
    }
    Ok(())
}

/// Stage a batch with an in-progress manifest written before any file moves.
pub(crate) fn stage_files(
    root: &Path,
    files: &[PathBuf],
    registry: &[RegistryExport],
) -> Result<(QuarantineManifest, PathBuf)> {
    if files.is_empty() && registry.is_empty() {
        return Err(blocked(root, "nothing to quarantine"));
    }
    if !plain_dirs(root)? {
        fs::create_dir_all(root).map_err(io_err(root))?;
    }
    plain_dirs(root)?;
    let mut items = Vec::new();
    let mut seen = BTreeSet::new();
    for (i, f) in files.iter().enumerate() {
        original_path(root, f)?;
        let path = f.to_str().ok_or_else(|| blocked(f, "invalid path text"))?;
        if !seen.insert(path_key(f)) {
            return Err(blocked(f, "duplicate quarantine candidate"));
        }
        let meta = plain(f, false)?.ok_or_else(|| blocked(f, "file is missing"))?;
        items.push(QuarantineItem::File {
            original_path: path.into(),
            blob: i.to_string(),
            size: meta.len(),
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
    let id = next_name(created);
    let dir = root.join(&id);
    let blobs = dir.join("blobs");
    fs::create_dir(&dir).map_err(io_err(&dir))?;
    fs::create_dir(&blobs).map_err(io_err(&blobs))?;
    let path = dir.join("manifest.json");
    let m = QuarantineManifest {
        version: 1,
        quarantine_id: id,
        created_unix_ms: created,
        status: ManifestStatus::InProgress,
        items,
    };
    write_manifest(&path, &m)?;
    for item in &m.items {
        if let QuarantineItem::File {
            original_path: source,
            blob,
            blake3,
            ..
        } = item
        {
            let src = Path::new(source);
            if let Err(e) =
                original_path(root, src).and_then(|()| move_file(src, &blobs.join(blob), blake3))
            {
                return Err(rollback_after(&path, &e));
            }
        }
    }
    Ok((m, path))
}

/// Try to put a staged batch back; retain any conflicting or corrupt blobs for manual review.
pub(crate) fn rollback_after(path: &Path, cause: &dyn std::fmt::Display) -> EngineError {
    match recover(path) {
        Ok(_) => EngineError::Quarantine(format!("{cause}; files put back")),
        Err(e) => blocked(
            path,
            &format!("{cause}; rollback incomplete: {e}; held files were kept"),
        ),
    }
}

pub(crate) fn finish_stage(path: &Path, m: &mut QuarantineManifest) -> Result<()> {
    m.status = ManifestStatus::Quarantined;
    write_manifest(path, m)
}

/// Quarantine without a database, for standalone engine callers. The desktop uses a staged
/// batch and records the related ledger entries before finalizing it.
pub fn quarantine(
    root: &Path,
    files: &[PathBuf],
    registry: &[RegistryExport],
) -> Result<(QuarantineManifest, PathBuf)> {
    let (mut m, path) = stage_files(root, files, registry)?;
    if let Err(e) = finish_stage(&path, &mut m) {
        return Err(rollback_after(&path, &e));
    }
    Ok((m, path))
}

fn verified(blob: &Path, expected: &str) -> Result<()> {
    if hash_file_blocking(blob)? != expected {
        return Err(blocked(blob, "hash mismatch"));
    }
    Ok(())
}

fn pending_writes<'a>(
    path: &Path,
    m: &'a QuarantineManifest,
    rollback: bool,
) -> Result<Vec<(PathBuf, &'a Path, &'a str)>> {
    let (root, _) = root_and_id(path)?;
    let blobs = path.with_file_name("blobs");
    let mut pending = Vec::new();
    let mut conflict = None;
    for item in &m.items {
        if let QuarantineItem::File {
            original_path: original,
            blob,
            blake3,
            ..
        } = item
        {
            let dst = Path::new(original);
            let src = blobs.join(blob);
            let held = plain(&src, false)?.is_some();
            if rollback && !held {
                continue;
            }
            let check = (|| {
                original_path(root, dst)?;
                let exists = plain(dst, false)?.is_some();
                if held {
                    verified(&src, blake3)?;
                    if exists {
                        return Err(blocked(
                            dst,
                            "already exists; the held file was not overwritten",
                        ));
                    }
                    pending.push((src, dst, blake3.as_str()));
                } else if exists {
                    verified(dst, blake3)?;
                } else {
                    return Err(blocked(&src, "missing blob and original"));
                }
                Ok(())
            })();
            if let Err(e) = check {
                if !rollback {
                    return Err(e);
                }
                conflict.get_or_insert(e);
            }
        }
    }
    // A normal restore preflights all items. During rollback, put back every independent
    // unblocked item before reporting any conflicts, and keep the in-progress manifest.
    if let Some(e) = conflict {
        put_back(root, pending)?;
        return Err(e);
    }
    Ok(pending)
}

fn put_back(root: &Path, pending: Vec<(PathBuf, &Path, &str)>) -> Result<()> {
    for (src, dst, hash) in pending {
        original_path(root, dst)?;
        let parent = dst
            .parent()
            .ok_or_else(|| blocked(dst, "missing parent directory"))?;
        fs::create_dir_all(parent).map_err(io_err(parent))?;
        original_path(root, dst)?;
        move_file(&src, dst, hash)?;
    }
    Ok(())
}

/// Restore without overwriting anything. If interrupted, a subsequent retry skips any verified
/// originals that already went back, and the in-progress source files remain recoverable.
pub fn restore(path: &Path) -> Result<QuarantineManifest> {
    let mut m = load_manifest(path)?;
    if m.status != ManifestStatus::Quarantined {
        return Err(blocked(
            path,
            &format!("cannot restore batch in state {:?}", m.status),
        ));
    }
    let (root, _) = root_and_id(path)?;
    put_back(root, pending_writes(path, &m, false)?)?;
    m.status = ManifestStatus::Restored;
    write_manifest(path, &m)?;
    Ok(m)
}

/// List syntactically valid batch IDs. Callers still have to validate each batch before use.
pub(crate) fn batch_ids(root: &Path) -> Result<Vec<String>> {
    if !plain_dirs(root)? {
        return Ok(Vec::new());
    }
    let mut ids = Vec::new();
    for entry in fs::read_dir(root).map_err(io_err(root))? {
        let entry = entry.map_err(io_err(root))?;
        if let Some(id) = entry.file_name().to_str().filter(|name| valid_id(name)) {
            ids.push(id.to_string());
        }
    }
    ids.sort_unstable();
    Ok(ids)
}

/// Manifests under `root` left in progress. Invalid batches are never followed.
pub fn find_incomplete(root: &Path) -> Result<Vec<PathBuf>> {
    Ok(batch_ids(root)?
        .into_iter()
        .filter_map(|id| {
            let path = batch_path(root, &id).ok()?;
            (load_manifest(&path).ok()?.status == ManifestStatus::InProgress).then_some(path)
        })
        .collect())
}

/// Undo an uncommitted batch. A missing blob is an untouched or already recovered item;
/// a conflicting original or corrupt blob keeps the batch in progress for review.
pub fn recover(path: &Path) -> Result<QuarantineManifest> {
    let mut m = load_manifest(path)?;
    if m.status != ManifestStatus::InProgress {
        return Err(blocked(
            path,
            &format!("nothing to recover from state {:?}", m.status),
        ));
    }
    let (root, _) = root_and_id(path)?;
    put_back(root, pending_writes(path, &m, true)?)?;
    m.status = ManifestStatus::RolledBack;
    write_manifest(path, &m)?;
    Ok(m)
}

/// Delete only this batch's listed, regular blobs. A retry can finish a partial purge.
pub fn purge(path: &Path) -> Result<QuarantineManifest> {
    let mut m = load_manifest(path)?;
    if m.status != ManifestStatus::Quarantined {
        return Err(blocked(
            path,
            &format!("cannot purge batch in state {:?}", m.status),
        ));
    }
    let blobs = path.with_file_name("blobs");
    let mut pending = Vec::new();
    for item in &m.items {
        if let QuarantineItem::File { blob, .. } = item {
            let src = blobs.join(blob);
            if plain(&src, false)?.is_some() {
                pending.push(src);
            }
        }
    }
    for src in pending {
        fs::remove_file(&src).map_err(io_err(&src))?;
    }
    // Remove only an empty directory; unexpected entries are not purge targets.
    let _ = fs::remove_dir(&blobs);
    m.status = ManifestStatus::Purged;
    write_manifest(path, &m)?;
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
        let r = restore(&mp).unwrap();
        assert_eq!(r.status, ManifestStatus::Restored);
        assert_eq!(std::fs::read(&a).unwrap(), b"aaa");
        assert_eq!(std::fs::read(&b).unwrap(), b"bbbb");
        assert!(mp.exists(), "manifest kept as evidence");
        assert!(restore(&mp).is_err(), "cannot restore twice");
    }

    #[test]
    fn restore_refuses_overwrite_and_detects_corruption() {
        let (d, a, _b) = setup();
        let q = d.path().join("q");
        let (_m, mp) = quarantine(&q, std::slice::from_ref(&a), &[]).unwrap();
        std::fs::write(&a, b"new").unwrap();
        assert!(restore(&mp).is_err());
        assert_eq!(std::fs::read(&a).unwrap(), b"new");
        std::fs::remove_file(&a).unwrap();
        assert_eq!(restore(&mp).unwrap().status, ManifestStatus::Restored);
        assert_eq!(std::fs::read(&a).unwrap(), b"aaa");

        let (_m2, mp2) = quarantine(&q, std::slice::from_ref(&a), &[]).unwrap();
        std::fs::write(mp2.parent().unwrap().join("blobs").join("0"), b"tampered").unwrap();
        assert!(restore(&mp2).is_err());
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
    fn recover_rolls_back_an_interrupted_run() {
        let (d, a, b) = setup();
        let q = d.path().join("q");
        let (m, mp) = quarantine(&q, &[a.clone(), b.clone()], &[]).unwrap();
        // simulate a crash after moving only the first file: b back in place, manifest in_progress
        let blobs = mp.parent().unwrap().join("blobs");
        std::fs::rename(blobs.join("1"), &b).unwrap();
        let mut crashed = m.clone();
        crashed.status = ManifestStatus::InProgress;
        write_manifest(&mp, &crashed).unwrap();
        assert_eq!(find_incomplete(&q).unwrap(), vec![mp.clone()]);
        let r = recover(&mp).unwrap();
        assert_eq!(r.status, ManifestStatus::RolledBack);
        assert_eq!(std::fs::read(&a).unwrap(), b"aaa");
        assert_eq!(std::fs::read(&b).unwrap(), b"bbbb");
        assert!(find_incomplete(&q).unwrap().is_empty());
        assert!(recover(&mp).is_err());
    }

    // ---- failure drills ----

    #[test]
    fn restore_resumes_after_a_partial_restore() {
        let (d, a, b) = setup();
        let q = d.path().join("q");
        let (_, mp) = quarantine(&q, &[a.clone(), b.clone()], &[]).unwrap();
        // an earlier restore got one file back, then died before finishing
        std::fs::rename(mp.parent().unwrap().join("blobs").join("0"), &a).unwrap();
        let m = restore(&mp).unwrap();
        assert_eq!(m.status, ManifestStatus::Restored);
        assert_eq!(std::fs::read(&a).unwrap(), b"aaa");
        assert_eq!(std::fs::read(&b).unwrap(), b"bbbb");
    }

    #[test]
    fn restore_does_not_hide_a_different_file_at_the_original_path() {
        let (d, a, b) = setup();
        let q = d.path().join("q");
        let (_, mp) = quarantine(&q, &[a.clone(), b.clone()], &[]).unwrap();
        std::fs::remove_file(mp.parent().unwrap().join("blobs").join("0")).unwrap();
        std::fs::write(&a, b"someone else's file").unwrap();
        assert!(
            restore(&mp).is_err(),
            "missing blob + different content must not pass as restored"
        );
        assert_eq!(std::fs::read(&a).unwrap(), b"someone else's file");
        assert!(
            !b.exists(),
            "nothing was moved because verification failed first"
        );
    }

    #[test]
    fn restore_recreates_deleted_parent_directory() {
        let (d, a, b) = setup();
        let q = d.path().join("q");
        let (_, mp) = quarantine(&q, &[a.clone(), b.clone()], &[]).unwrap();
        let _ = std::fs::remove_dir_all(a.parent().unwrap());
        restore(&mp).unwrap();
        assert_eq!(std::fs::read(&a).unwrap(), b"aaa");
    }

    #[test]
    fn recover_never_overwrites_a_file_that_reappeared() {
        let (d, a, b) = setup();
        let q = d.path().join("q");
        let (m, mp) = quarantine(&q, &[a.clone(), b.clone()], &[]).unwrap();
        let mut crashed = m.clone();
        crashed.status = ManifestStatus::InProgress;
        write_manifest(&mp, &crashed).unwrap();
        std::fs::write(&a, b"reinstalled").unwrap();
        assert!(recover(&mp).is_err());
        assert_eq!(
            load_manifest(&mp).unwrap().status,
            ManifestStatus::InProgress
        );
        assert_eq!(std::fs::read(&a).unwrap(), b"reinstalled");
        assert!(
            mp.parent().unwrap().join("blobs").join("0").exists(),
            "blob kept, not lost"
        );
        assert_eq!(std::fs::read(&b).unwrap(), b"bbbb");
    }

    #[test]
    fn missing_file_aborts_before_anything_moves() {
        let (d, a, b) = setup();
        let q = d.path().join("q");
        let gone = d.path().join("app").join("gone.dll");
        assert!(quarantine(&q, &[a.clone(), gone, b.clone()], &[]).is_err());
        assert_eq!(std::fs::read(&a).unwrap(), b"aaa");
        assert_eq!(std::fs::read(&b).unwrap(), b"bbbb");
        assert!(find_incomplete(&q).unwrap().is_empty());
    }

    #[test]
    fn moving_cannot_replace_an_existing_destination() {
        let (_d, a, b) = setup();
        let hash = hash_file_blocking(&a).unwrap();
        assert!(move_file(&a, &b, &hash).is_err());
        assert_eq!(fs::read(&a).unwrap(), b"aaa");
        assert_eq!(fs::read(&b).unwrap(), b"bbbb");
    }

    /// A quarantine folder on another filesystem forces the copy + verify + delete path.
    #[cfg(target_os = "linux")]
    #[test]
    fn cross_filesystem_quarantine_and_restore() {
        let shm = Path::new("/dev/shm");
        if !shm.is_dir() {
            return;
        }
        let (d, a, b) = setup();
        let q = tempfile::tempdir_in(shm).unwrap();
        let (_, mp) = quarantine(q.path(), &[a.clone(), b.clone()], &[]).unwrap();
        assert!(!a.exists() && !b.exists());
        restore(&mp).unwrap();
        assert_eq!(std::fs::read(&a).unwrap(), b"aaa");
        assert_eq!(std::fs::read(&b).unwrap(), b"bbbb");
        drop(d);
    }

    #[test]
    fn malformed_paths_are_rejected_before_any_writes() {
        let (d, a, b) = setup();
        let q = d.path().join("q");
        let (mut m, mp) = stage_files(&q, &[a.clone(), b.clone()], &[]).unwrap();
        if let QuarantineItem::File { blob, .. } = &mut m.items[1] {
            *blob = "../manifest.json".into();
        }
        write_manifest(&mp, &m).unwrap();
        assert!(recover(&mp).is_err());
        assert!(!a.exists() && !b.exists());
        assert_eq!(
            fs::read(mp.with_file_name("blobs").join("0")).unwrap(),
            b"aaa"
        );
    }

    #[test]
    fn corrupt_blob_stays_held_for_review() {
        let (d, a, b) = setup();
        let q = d.path().join("q");
        let (_, mp) = stage_files(&q, &[a.clone(), b.clone()], &[]).unwrap();
        let blob = mp.with_file_name("blobs").join("1");
        fs::write(&blob, b"damaged").unwrap();
        assert!(recover(&mp).is_err());
        assert_eq!(fs::read(a).unwrap(), b"aaa");
        assert!(!b.exists());
        assert_eq!(fs::read(blob).unwrap(), b"damaged");
        assert_eq!(
            load_manifest(&mp).unwrap().status,
            ManifestStatus::InProgress
        );
    }

    #[test]
    fn purge_does_not_delete_unlisted_files() {
        let (d, a, _) = setup();
        let (_, mp) = quarantine(&d.path().join("q"), &[a], &[]).unwrap();
        let note = mp.with_file_name("blobs").join("unexpected.txt");
        fs::write(&note, b"keep this").unwrap();
        purge(&mp).unwrap();
        assert_eq!(fs::read(note).unwrap(), b"keep this");
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
        assert!(restore(&mp).is_err());
    }
}
