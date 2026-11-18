//! Recoverable, file-only quarantine. A database record is an index, not path authority.
//! Manifests and blobs are local files below the configured root; no action follows a link.

use crate::Cancel;
use crate::error::{EngineError, Result, io_err};
use crate::hashing::{hash_file_blocking, hash_reader};
use crate::publication::sync_dir;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{ErrorKind, Read, Write};
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
    /// The user authorized restore; completed originals can be recognized on retry.
    Restoring,
    /// The user authorized purge. Never resume an incomplete purge automatically.
    Purging,
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

const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;
const MAX_MANIFEST_ITEMS: usize = 16_384;

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

pub(crate) fn valid_id(id: &str) -> bool {
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

pub(crate) fn local_path(path: &Path) -> Result<()> {
    let mut parts = path.components();
    #[cfg(windows)]
    if !exor_fs::is_local_drive(path) || !matches!(parts.next(), Some(Component::Prefix(_))) {
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
pub(crate) fn plain(path: &Path, dir: bool) -> Result<Option<Metadata>> {
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

pub(crate) fn plain_dirs(path: &Path) -> Result<bool> {
    local_path(path)?;
    let mut ancestors: Vec<_> = path.ancestors().collect();
    while let Some(dir) = ancestors.pop() {
        if !dir.has_root() {
            continue;
        }
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

/// The exact file read and shown in a quarantine proposal. This is not deserialized from IPC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewedFile {
    path: PathBuf,
    canonical: PathBuf,
    stamp: FileStamp,
    hash: String,
    #[cfg(windows)]
    identity: exor_fs::Identity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileStamp {
    size: u64,
    modified: Option<SystemTime>,
    created: Option<SystemTime>,
    #[cfg(unix)]
    identity: (u64, u64),
}

impl FileStamp {
    fn of(meta: &Metadata) -> Self {
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Self {
            size: meta.len(),
            modified: meta.modified().ok(),
            created: meta.created().ok(),
            #[cfg(unix)]
            identity: (meta.dev(), meta.ino()),
        }
    }
}

impl ReviewedFile {
    /// Original path used to restore this file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Hash shown by the recorded evidence and verified at review time.
    pub fn hash(&self) -> &str {
        &self.hash
    }

    /// Logical bytes checked in the proposal.
    pub fn size(&self) -> u64 {
        self.stamp.size
    }
}

// Keep the read handle live from verification across the move. On Windows, refuse reparse
// points at open and deny sharing for writers or deletion.
fn open_plain(path: &Path) -> Result<File> {
    let expected = plain(path, false)?.ok_or_else(|| blocked(path, "file is missing"))?;
    #[cfg(windows)]
    let file = exor_fs::open_locked(path, false).map_err(io_err(path))?;
    #[cfg(not(windows))]
    let file = File::open(path).map_err(io_err(path))?;
    let actual = file.metadata().map_err(io_err(path))?;
    if !actual.is_file() || is_link(&actual) || FileStamp::of(&expected) != FileStamp::of(&actual) {
        return Err(blocked(path, "file changed while it was opened"));
    }
    Ok(file)
}

fn inspect_file(path: &Path, max_bytes: u64, cancel: &Cancel) -> Result<ReviewedFile> {
    cancel.check()?;
    local_path(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| blocked(path, "missing parent"))?;
    if !plain_dirs(parent)? {
        return Err(blocked(parent, "file parent is missing"));
    }
    #[cfg(windows)]
    let _parents = exor_fs::pin_parents(parent).map_err(io_err(parent))?;
    let canonical = path.canonicalize().map_err(io_err(path))?;
    let mut file = open_plain(path)?;
    let stamp = FileStamp::of(&file.metadata().map_err(io_err(path))?);
    if stamp.size > max_bytes {
        return Err(blocked(
            path,
            "file exceeds the remaining review byte budget",
        ));
    }
    let hash = hash_reader(
        &mut (&mut file).take(stamp.size.saturating_add(1)),
        path,
        Some(cancel),
    )?;
    if stamp != FileStamp::of(&file.metadata().map_err(io_err(path))?)
        || path.canonicalize().map_err(io_err(path))? != canonical
        || !plain(path, false)?.is_some_and(|meta| FileStamp::of(&meta) == stamp)
    {
        return Err(blocked(path, "file changed during review"));
    }
    #[cfg(windows)]
    let identity = exor_fs::identity(&file).map_err(io_err(path))?;
    Ok(ReviewedFile {
        path: path.to_path_buf(),
        canonical,
        stamp,
        hash,
        #[cfg(windows)]
        identity,
    })
}

/// Validate the active file against a bounded set of application roots and an optional recorded
/// content fingerprint. This is separate from policy: a policy decision alone is never a plan.
pub fn review_file(
    path: &Path,
    approved_roots: &[PathBuf],
    protected: &Path,
    evidence_hash: Option<&str>,
    cancel: &Cancel,
) -> Result<ReviewedFile> {
    review_file_bounded(
        path,
        approved_roots,
        protected,
        evidence_hash,
        u64::MAX,
        cancel,
    )
}

pub(crate) fn review_file_bounded(
    path: &Path,
    approved_roots: &[PathBuf],
    protected: &Path,
    evidence_hash: Option<&str>,
    max_bytes: u64,
    cancel: &Cancel,
) -> Result<ReviewedFile> {
    cancel.check()?;
    local_path(path)?;
    if !plain_dirs(
        path.parent()
            .ok_or_else(|| blocked(path, "missing parent"))?,
    )? || plain(path, false)?.is_none()
    {
        return Err(blocked(path, "file or parent is not a plain existing path"));
    }
    let resolved = path.canonicalize().map_err(io_err(path))?;
    let within = |root: &Path| resolved != root && resolved.starts_with(root);
    let protected = protected.canonicalize().map_err(io_err(protected))?;
    if within(&protected) {
        return Err(blocked(path, "the application profile is not eligible"));
    }
    if !approved_roots.iter().any(|root| {
        matches!(plain_dirs(root), Ok(true)) && root.canonicalize().is_ok_and(|r| within(&r))
    }) {
        return Err(blocked(path, "not under an approved application root"));
    }
    let item = inspect_file(path, max_bytes, cancel)?;
    if item.canonical != resolved || evidence_hash.is_some_and(|hash| hash != item.hash) {
        return Err(blocked(
            path,
            "file no longer matches its recorded evidence",
        ));
    }
    #[cfg(windows)]
    if item.identity.0 != exor_fs::directory_volume(&protected).map_err(io_err(&protected))? {
        return Err(blocked(
            path,
            "the file and quarantine profile are on different volumes",
        ));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if item.stamp.identity.0 != fs::metadata(&protected).map_err(io_err(&protected))?.dev() {
            return Err(blocked(
                path,
                "the file and quarantine profile are on different volumes",
            ));
        }
    }
    Ok(item)
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
    #[cfg(windows)]
    let _dirs = exor_fs::pin_parents(path.parent().unwrap_or(root)).map_err(io_err(&path))?;
    let mut bytes = Vec::new();
    open_plain(&path)?
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(io_err(&path))?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(blocked(&path, "manifest exceeds its read limit"));
    }
    let m: QuarantineManifest = serde_json::from_slice(&bytes)?;
    if m.version != 1
        || m.quarantine_id != id
        || m.items.is_empty()
        || m.items.len() > MAX_MANIFEST_ITEMS
    {
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
    if bytes.len() as u64 > MAX_MANIFEST_BYTES || m.items.len() > MAX_MANIFEST_ITEMS {
        return Err(blocked(path, "manifest exceeds its write limit"));
    }
    #[cfg(windows)]
    let _dirs = exor_fs::pin_parents(
        path.parent()
            .ok_or_else(|| blocked(path, "missing batch folder"))?,
    )
    .map_err(io_err(path))?;
    let mut f = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(io_err(&temp))?;
    let result = f.write_all(&bytes).and_then(|()| f.sync_all());
    drop(f);
    #[cfg(windows)]
    let result = result.and_then(|()| exor_fs::publish(&temp, path, true));
    #[cfg(not(windows))]
    let result = result.and_then(|()| fs::rename(&temp, path));
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(io_err(path))?;
    if let Some(dir) = path.parent() {
        sync_dir(dir)?;
    }
    Ok(())
}

/// Move an exact file without replacement. On Windows use a handle-bound same-volume rename.
#[cfg(windows)]
fn move_file(
    src: &Path,
    dst: &Path,
    expected: &str,
    reviewed: Option<&ReviewedFile>,
    cancel: Option<&Cancel>,
) -> Result<()> {
    let src_parent = src
        .parent()
        .ok_or_else(|| blocked(src, "missing source parent"))?;
    let dst_parent = dst
        .parent()
        .ok_or_else(|| blocked(dst, "missing destination parent"))?;
    let _source_dirs = exor_fs::pin_parents(src_parent).map_err(io_err(src_parent))?;
    let _dest_dirs = exor_fs::pin_parents(dst_parent).map_err(io_err(dst_parent))?;
    let mut input = exor_fs::open_locked(src, true).map_err(io_err(src))?;
    let stamp = FileStamp::of(&input.metadata().map_err(io_err(src))?);
    let identity = exor_fs::identity(&input).map_err(io_err(src))?;
    if let Some(r) = reviewed
        && (r.stamp != stamp
            || r.identity != identity
            || src.canonicalize().map_err(io_err(src))? != r.canonical)
    {
        return Err(blocked(src, "the reviewed file changed; rebuild the plan"));
    }
    if hash_reader(
        &mut (&mut input).take(stamp.size.saturating_add(1)),
        src,
        cancel,
    )? != expected
    {
        return Err(blocked(src, "file contents changed"));
    }
    let dest = dst_parent.canonicalize().map_err(io_err(dst_parent))?.join(
        dst.file_name()
            .ok_or_else(|| blocked(dst, "missing destination name"))?,
    );
    exor_fs::rename_open(&input, &dest).map_err(|e| {
        if e.kind() == ErrorKind::CrossesDevices {
            blocked(
                src,
                "cross-volume quarantine is disabled on Windows to preserve metadata and streams",
            )
        } else {
            io_err(dst)(e)
        }
    })
}

/// Unix development fallback: publish a matching file without replacement, then unlink.
#[cfg(not(windows))]
fn move_file(
    src: &Path,
    dst: &Path,
    expected: &str,
    reviewed: Option<&ReviewedFile>,
    cancel: Option<&Cancel>,
) -> Result<()> {
    let mut input = open_plain(src)?;
    let actual = FileStamp::of(&input.metadata().map_err(io_err(src))?);
    if reviewed.is_some_and(|r| {
        r.stamp != actual || src.canonicalize().ok().as_ref() != Some(&r.canonical)
    }) || hash_reader(
        &mut (&mut input).take(actual.size.saturating_add(1)),
        src,
        cancel,
    )? != expected
        || !plain(src, false)?.is_some_and(|meta| FileStamp::of(&meta) == actual)
    {
        return Err(blocked(src, "missing file or content changed"));
    }
    // Fail closed across volumes on all platforms; the previous copy path could not retain all metadata.
    match fs::hard_link(src, dst) {
        Ok(()) => {}
        Err(e) if e.kind() == ErrorKind::CrossesDevices => {
            return Err(blocked(src, "cross-volume quarantine is disabled"));
        }
        Err(e) => return Err(io_err(dst)(e)),
    }
    if let Some(dir) = dst.parent() {
        sync_dir(dir)?;
    }
    if let Err(e) = fs::remove_file(src) {
        // The original still exists. If we cannot remove the duplicate, recovery reports it.
        let _ = fs::remove_file(dst);
        return Err(io_err(src)(e));
    }
    if let Some(dir) = src.parent() {
        sync_dir(dir)?;
    }
    Ok(())
}

/// Stage a batch with an in-progress manifest written before any file moves.
pub(crate) fn stage_files(
    root: &Path,
    files: &[PathBuf],
    registry: &[RegistryExport],
) -> Result<(QuarantineManifest, PathBuf)> {
    let inspected = files
        .iter()
        .map(|f| inspect_file(f, u64::MAX, &Cancel::new()))
        .collect::<Result<Vec<_>>>()?;
    stage_reviewed(root, &inspected, registry, &Cancel::new())
}

/// Verify every reviewed file again before staging, then compare the open handle before each move.
pub(crate) fn stage_reviewed(
    root: &Path,
    files: &[ReviewedFile],
    registry: &[RegistryExport],
    cancel: &Cancel,
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
    for (i, item) in files.iter().enumerate() {
        let f = item.path();
        original_path(root, f)?;
        if &inspect_file(f, item.size(), cancel)? != item {
            return Err(blocked(f, "the reviewed file changed; rebuild the plan"));
        }
        let path = f.to_str().ok_or_else(|| blocked(f, "invalid path text"))?;
        if !seen.insert(path_key(f)) {
            return Err(blocked(f, "duplicate quarantine candidate"));
        }
        items.push(QuarantineItem::File {
            original_path: path.into(),
            blob: i.to_string(),
            size: item.stamp.size,
            blake3: item.hash.clone(),
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
    sync_dir(root)?;
    let path = dir.join("manifest.json");
    let m = QuarantineManifest {
        version: 1,
        quarantine_id: id,
        created_unix_ms: created,
        status: ManifestStatus::InProgress,
        items,
    };
    write_manifest(&path, &m)?;
    for (i, item) in m.items.iter().enumerate() {
        if let QuarantineItem::File {
            original_path: source,
            blob,
            blake3,
            ..
        } = item
        {
            let src = Path::new(source);
            let moved = cancel
                .check()
                .and_then(|()| original_path(root, src))
                .and_then(|()| {
                    move_file(src, &blobs.join(blob), blake3, files.get(i), Some(cancel))
                });
            if let Err(e) = moved {
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
    items: &'a [QuarantineItem],
    allow_already_restored: bool,
    rollback: bool,
) -> Result<Vec<(PathBuf, &'a Path, &'a str)>> {
    let (root, _) = root_and_id(path)?;
    let blobs = path.with_file_name("blobs");
    let mut pending = Vec::new();
    let mut conflict = None;
    for item in items {
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
                } else if exists && allow_already_restored {
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
        move_file(&src, dst, hash, None, None)?;
    }
    Ok(())
}

/// Restore without overwriting anything. If interrupted, a subsequent retry skips any verified
/// originals that already went back, and the in-progress source files remain recoverable.
pub fn restore(path: &Path) -> Result<QuarantineManifest> {
    let mut m = load_manifest(path)?;
    if !matches!(
        m.status,
        ManifestStatus::Quarantined | ManifestStatus::Restoring
    ) {
        return Err(blocked(
            path,
            &format!("cannot restore batch in state {:?}", m.status),
        ));
    }
    let (root, _) = root_and_id(path)?;
    let pending = pending_writes(path, &m.items, m.status == ManifestStatus::Restoring, false)?;
    if m.status != ManifestStatus::Restoring {
        m.status = ManifestStatus::Restoring;
        write_manifest(path, &m)?;
    }
    put_back(root, pending)?;
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
    for (index, entry) in fs::read_dir(root).map_err(io_err(root))?.enumerate() {
        if index >= 40_000 {
            return Err(blocked(
                root,
                "too many quarantine directories to enumerate safely",
            ));
        }
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
    put_back(root, pending_writes(path, &m.items, true, true)?)?;
    m.status = ManifestStatus::RolledBack;
    write_manifest(path, &m)?;
    Ok(m)
}

fn check_held(input: &mut File, src: &Path, size: u64, expected: &str) -> Result<()> {
    if input.metadata().map_err(io_err(src))?.len() != size
        || hash_reader(&mut input.take(size.saturating_add(1)), src, None)? != expected
    {
        return Err(blocked(src, "held bytes do not match the manifest"));
    }
    Ok(())
}

/// Before sealing a disk-only operation in SQLite, check that it actually finished.
pub(crate) fn check_finished(path: &Path, m: &QuarantineManifest) -> Result<()> {
    let blobs = path.with_file_name("blobs");
    let (root, _) = root_and_id(path)?;
    for item in &m.items {
        if let QuarantineItem::File {
            blob,
            original_path: original,
            size,
            blake3,
        } = item
        {
            let src = blobs.join(blob);
            if plain(&src, false)?.is_some() {
                return Err(blocked(
                    &src,
                    "finished manifest still has a held file; no evidence was changed",
                ));
            }
            if m.status == ManifestStatus::Restored {
                let dest = Path::new(original);
                original_path(root, dest)?;
                check_held(&mut open_plain(dest)?, dest, *size, blake3)?;
            }
        }
    }
    Ok(())
}

/// Delete only verified, listed files. A retry can finish a partial purge, but a missing or
/// changed blob in a fresh held batch blocks the entire delete before any bytes are removed.
pub fn purge(path: &Path) -> Result<QuarantineManifest> {
    purge_matching(path, false, None)
}

pub(crate) fn purge_matching(
    path: &Path,
    archive: bool,
    expected: Option<&QuarantineManifest>,
) -> Result<QuarantineManifest> {
    let mut m = load_manifest(path)?;
    if !matches!(
        m.status,
        ManifestStatus::Quarantined | ManifestStatus::Purging
    ) || expected.is_some_and(|record| *record != m)
    {
        return Err(blocked(
            path,
            "batch changed or cannot be deleted in its current state",
        ));
    }
    let blobs = path.with_file_name("blobs");
    let mut pending = Vec::new();
    for item in &m.items {
        if let QuarantineItem::File {
            blob, size, blake3, ..
        } = item
        {
            let src = blobs.join(blob);
            if plain(&src, false)?.is_some() {
                #[cfg(windows)]
                let _dirs = exor_fs::pin_parents(&blobs).map_err(io_err(&blobs))?;
                let mut input = open_plain(&src)?;
                check_held(&mut input, &src, *size, blake3)?;
                #[cfg(windows)]
                if archive {
                    exor_fs::single_data_stream(&input).map_err(io_err(&src))?;
                }
                pending.push((src, *size, blake3.clone()));
            } else if m.status == ManifestStatus::Quarantined {
                return Err(blocked(&src, "held file is missing; nothing was deleted"));
            }
        }
    }
    if m.status != ManifestStatus::Purging {
        m.status = ManifestStatus::Purging;
        write_manifest(path, &m)?;
    }
    for (src, size, expected) in pending {
        #[cfg(windows)]
        {
            let _dirs = exor_fs::pin_parents(&blobs).map_err(io_err(&blobs))?;
            let mut input = exor_fs::open_locked(&src, true).map_err(io_err(&src))?;
            exor_fs::identity(&input).map_err(io_err(&src))?;
            if archive {
                exor_fs::single_data_stream(&input).map_err(io_err(&src))?;
            }
            check_held(&mut input, &src, size, &expected)?;
            exor_fs::delete_open(&input).map_err(io_err(&src))?;
        }
        #[cfg(not(windows))]
        {
            let _ = archive;
            if !plain_dirs(&blobs)? {
                return Err(blocked(&blobs, "held folder disappeared"));
            }
            check_held(&mut open_plain(&src)?, &src, size, &expected)?;
            fs::remove_file(&src).map_err(io_err(&src))?;
        }
    }
    if blobs.is_dir() {
        sync_dir(&blobs)?;
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
        let (mut manifest, mp) = quarantine(&q, &[a.clone(), b.clone()], &[]).unwrap();
        manifest.status = ManifestStatus::Restoring;
        write_manifest(&mp, &manifest).unwrap();
        // an earlier restore persisted intent and got one file back, then stopped
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
        assert!(move_file(&a, &b, &hash, None, None).is_err());
        assert_eq!(fs::read(&a).unwrap(), b"aaa");
        assert_eq!(fs::read(&b).unwrap(), b"bbbb");
    }

    /// Cross-filesystem movement fails closed rather than copying only the default stream.
    #[cfg(target_os = "linux")]
    #[test]
    fn cross_filesystem_quarantine_leaves_originals_alone() {
        let shm = Path::new("/dev/shm");
        if !shm.is_dir() {
            return;
        }
        let (d, a, b) = setup();
        let q = tempfile::tempdir_in(shm).unwrap();
        assert!(quarantine(q.path(), &[a.clone(), b.clone()], &[]).is_err());
        assert!(find_incomplete(q.path()).unwrap().is_empty());
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
