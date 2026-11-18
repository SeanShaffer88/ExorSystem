//! Cancellable, budgeted filesystem enumeration on a single blocking worker.

use crate::cancel::Cancel;
use crate::error::{EngineError, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Entry kind. Symlinks and special files are never followed or recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum EntryKind {
    /// Regular file.
    File,
    /// Directory.
    Dir,
}

/// One enumerated filesystem entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// Full path.
    pub path: PathBuf,
    /// Kind.
    pub kind: EntryKind,
    /// Size in bytes (0 for directories).
    pub size: u64,
    /// Modified time, ns since Unix epoch (0 if unavailable).
    pub mtime_ns: i64,
}

/// Walk tuning.
#[derive(Debug, Clone)]
pub struct WalkOptions {
    /// Maximum filesystem entries inspected, including exclusions and redirects.
    pub max_entries: usize,
    /// Maximum bytes retained in path names (conservative text bound).
    pub max_path_bytes: usize,
    /// Case-insensitive path substrings (with `\` normalised to `/`) to skip entirely.
    pub exclude_contains: Vec<String>,
}

impl Default for WalkOptions {
    fn default() -> Self {
        Self {
            max_entries: 24_000,
            max_path_bytes: 8 * 1024 * 1024,
            exclude_contains: Vec::new(),
        }
    }
}

/// Result of a walk.
#[derive(Debug, Default)]
pub struct Walk {
    /// Entries, root excluded.
    pub entries: Vec<Entry>,
    /// Directories/files that could not be read, with the reason.
    pub skipped: Vec<(PathBuf, String)>,
    /// Total path bytes charged against the budget.
    pub path_bytes: usize,
}

fn norm(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/").to_lowercase()
}

pub(crate) fn mtime_ns(md: &std::fs::Metadata) -> i64 {
    md.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX))
}

pub(crate) fn redirected(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    meta.is_symlink()
}

fn limit(root: &Path, detail: &str) -> EngineError {
    EngineError::Watch(format!(
        "{}: {detail} exceeded the capture budget. Choose a smaller scope.",
        root.display()
    ))
}

fn walk_blocking(
    root: &Path,
    opts: &WalkOptions,
    cancel: &Cancel,
    progress: &dyn Fn(u64),
) -> Result<Walk> {
    let excl: Vec<_> = opts
        .exclude_contains
        .iter()
        .map(|s| s.replace('\\', "/").to_lowercase())
        .collect();
    let mut pending = vec![(root.to_path_buf(), 0)];
    let mut result = Walk::default();
    let mut inspected = 0_usize;
    let mut files = 0_u64;
    let mut last = std::time::Instant::now();
    while let Some((dir, depth)) = pending.pop() {
        cancel.check()?;
        if depth > 64 {
            return Err(limit(root, "Directory depth"));
        }
        // Recheck ancestors rather than descending a redirect discovered after enumeration.
        if let Err(e) = crate::quarantine::plain_dirs(&dir).and_then(|plain| {
            if plain {
                Ok(())
            } else {
                Err(EngineError::Watch(
                    "directory disappeared during capture".into(),
                ))
            }
        }) {
            result.skipped.push((dir, e.to_string()));
            continue;
        }
        let reader = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(e) => {
                result.skipped.push((dir, e.to_string()));
                continue;
            }
        };
        for entry in reader {
            cancel.check()?;
            inspected += 1;
            if inspected > opts.max_entries {
                return Err(limit(root, "Entry count"));
            }
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    result.skipped.push((dir.clone(), e.to_string()));
                    continue;
                }
            };
            let path = entry.path();
            let Some(text) = path.to_str() else {
                result
                    .skipped
                    .push((path, "not a representable path".into()));
                continue;
            };
            if text.len() > 4_096 {
                return Err(limit(root, "Individual path length"));
            }
            result.path_bytes = result.path_bytes.saturating_add(text.len());
            if result.path_bytes > opts.max_path_bytes {
                return Err(limit(root, "Path storage"));
            }
            if !excl.is_empty() && excl.iter().any(|x| norm(&path).contains(x.as_str())) {
                continue;
            }
            let meta = match std::fs::symlink_metadata(&path) {
                Ok(m) => m,
                Err(e) => {
                    result.skipped.push((path, e.to_string()));
                    continue;
                }
            };
            if redirected(&meta) {
                continue;
            }
            if meta.is_dir() {
                pending.push((path.clone(), depth + 1));
                result.entries.push(Entry {
                    path,
                    kind: EntryKind::Dir,
                    size: 0,
                    mtime_ns: mtime_ns(&meta),
                });
            } else if meta.is_file() {
                result.entries.push(Entry {
                    path,
                    kind: EntryKind::File,
                    size: meta.len(),
                    mtime_ns: mtime_ns(&meta),
                });
                files += 1;
            }
            if last.elapsed() >= std::time::Duration::from_millis(125) {
                progress(files);
                last = std::time::Instant::now();
            }
        }
    }
    result.entries.sort_unstable_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
}

/// Walk `root` without following redirects. Exceeding a budget makes the whole scope incomplete.
pub async fn walk(root: &Path, opts: &WalkOptions, cancel: &Cancel) -> Result<Walk> {
    walk_progress(root, opts, cancel, Arc::new(|_| {})).await
}

pub(crate) async fn walk_progress(
    root: &Path,
    opts: &WalkOptions,
    cancel: &Cancel,
    progress: Arc<dyn Fn(u64) + Send + Sync>,
) -> Result<Walk> {
    let (root, opts, cancel) = (root.to_path_buf(), opts.clone(), cancel.clone());
    tokio::task::spawn_blocking(move || walk_blocking(&root, &opts, &cancel, &*progress)).await?
}
