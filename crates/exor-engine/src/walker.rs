//! Non-blocking, parallel, level-by-level recursive directory enumeration on `tokio::fs`.

use crate::cancel::Cancel;
use crate::error::{EngineError, Result};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

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
    /// Max directories read concurrently.
    pub max_concurrency: usize,
    /// Case-insensitive path substrings (with `\` normalised to `/`) to skip entirely.
    pub exclude_contains: Vec<String>,
}

impl Default for WalkOptions {
    fn default() -> Self {
        Self {
            max_concurrency: 16,
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
}

fn norm(p: &Path) -> String {
    p.to_string_lossy().replace('\\', "/").to_lowercase()
}

fn mtime_ns(md: &std::fs::Metadata) -> i64 {
    md.modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| i64::try_from(d.as_nanos()).unwrap_or(i64::MAX))
}

struct DirResult {
    entries: Vec<Entry>,
    subdirs: Vec<PathBuf>,
    skipped: Vec<(PathBuf, String)>,
}

async fn read_one(dir: PathBuf, excl: Arc<Vec<String>>) -> DirResult {
    let mut out = DirResult {
        entries: Vec::new(),
        subdirs: Vec::new(),
        skipped: Vec::new(),
    };
    let mut rd = match tokio::fs::read_dir(&dir).await {
        Ok(r) => r,
        Err(e) => {
            out.skipped.push((dir, e.to_string()));
            return out;
        }
    };
    loop {
        let de = match rd.next_entry().await {
            Ok(Some(de)) => de,
            Ok(None) => break,
            Err(e) => {
                out.skipped.push((dir.clone(), e.to_string()));
                break;
            }
        };
        let path = de.path();
        if !excl.is_empty() {
            let n = norm(&path);
            if excl.iter().any(|x| n.contains(x.as_str())) {
                continue;
            }
        }
        // symlink_metadata: never follow links (avoids loops and escaping the root)
        let md = match tokio::fs::symlink_metadata(&path).await {
            Ok(m) => m,
            Err(e) => {
                out.skipped.push((path, e.to_string()));
                continue;
            }
        };
        let ft = md.file_type();
        if ft.is_dir() {
            out.entries.push(Entry {
                path: path.clone(),
                kind: EntryKind::Dir,
                size: 0,
                mtime_ns: mtime_ns(&md),
            });
            out.subdirs.push(path);
        } else if ft.is_file() {
            out.entries.push(Entry {
                path,
                kind: EntryKind::File,
                size: md.len(),
                mtime_ns: mtime_ns(&md),
            });
        }
    }
    out
}

/// Enumerate everything below `root`, reading up to `max_concurrency` directories in parallel.
pub async fn walk(root: &Path, opts: &WalkOptions, cancel: &Cancel) -> Result<Walk> {
    let excl: Arc<Vec<String>> = Arc::new(
        opts.exclude_contains
            .iter()
            .map(|s| s.replace('\\', "/").to_lowercase())
            .collect(),
    );
    let sem = Arc::new(Semaphore::new(opts.max_concurrency.max(1)));
    let mut result = Walk::default();
    let mut level = vec![root.to_path_buf()];
    while !level.is_empty() {
        cancel.check()?;
        let mut set = JoinSet::new();
        for dir in level.drain(..) {
            let sem = Arc::clone(&sem);
            let excl = Arc::clone(&excl);
            set.spawn(async move {
                let _permit = sem.acquire_owned().await;
                read_one(dir, excl).await
            });
        }
        let mut next = Vec::new();
        while let Some(joined) = set.join_next().await {
            let r = joined.map_err(EngineError::from)?;
            result.entries.extend(r.entries);
            result.skipped.extend(r.skipped);
            next.extend(r.subdirs);
        }
        level = next;
    }
    result.entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(result)
}
