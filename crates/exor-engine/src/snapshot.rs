//! Snapshot capture: walk + incremental BLAKE3 hashing into a [`PathTrie`].

use crate::cancel::Cancel;
use crate::error::{EngineError, Result};
use crate::hashing::hash_file_blocking;
use crate::trie::{FileState, PathTrie};
use crate::walker::{EntryKind, WalkOptions, walk};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

/// Capture tuning.
#[derive(Debug, Clone)]
pub struct CaptureOptions {
    /// Walk options.
    pub walk: WalkOptions,
    /// Max files hashed concurrently.
    pub hash_concurrency: usize,
    /// Files larger than this are recorded without a content hash (size+mtime used instead).
    pub max_hash_bytes: Option<u64>,
}

impl Default for CaptureOptions {
    fn default() -> Self {
        Self {
            walk: WalkOptions::default(),
            hash_concurrency: 8,
            max_hash_bytes: None,
        }
    }
}

/// Capture phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Enumerating directories.
    Walking,
    /// Hashing file contents.
    Hashing,
    /// Finished.
    Done,
}

/// Live progress, safe to stream to a UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Progress {
    /// Current phase.
    pub phase: Phase,
    /// Entries found by the walk (0 until the walk ends).
    pub files_seen: u64,
    /// Files finished (hashed or reused).
    pub files_hashed: u64,
    /// Bytes covered by finished files.
    pub bytes_hashed: u64,
}

const PROGRESS_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);

/// A finalized trie plus anything that could not be read.
#[derive(Debug)]
pub struct Snapshot {
    /// Finalized trie.
    pub trie: PathTrie,
    /// Unreadable paths with the reason.
    pub skipped: Vec<(String, String)>,
}

/// Capture `root`. With `prev`, files whose size and mtime are unchanged reuse the old hash.
pub async fn capture(
    root: &Path,
    opts: &CaptureOptions,
    prev: Option<&PathTrie>,
    cancel: &Cancel,
) -> Result<Snapshot> {
    capture_with_progress(root, opts, prev, cancel, &|_| {}).await
}

/// Like [`capture`], reporting [`Progress`] at most every 100 ms (plus phase changes and the end).
pub async fn capture_with_progress(
    root: &Path,
    opts: &CaptureOptions,
    prev: Option<&PathTrie>,
    cancel: &Cancel,
    on_progress: &(dyn Fn(Progress) + Send + Sync),
) -> Result<Snapshot> {
    let mut p = Progress {
        phase: Phase::Walking,
        files_seen: 0,
        files_hashed: 0,
        bytes_hashed: 0,
    };
    on_progress(p);
    let w = walk(root, &opts.walk, cancel).await?;
    p.phase = Phase::Hashing;
    p.files_seen = w
        .entries
        .iter()
        .filter(|e| e.kind == EntryKind::File)
        .count() as u64;
    on_progress(p);
    let mut last = std::time::Instant::now();
    let mut trie = PathTrie::new();
    let mut skipped: Vec<(String, String)> = w
        .skipped
        .into_iter()
        .map(|(p, r)| (p.to_string_lossy().into_owned(), r))
        .collect();
    let sem = Arc::new(Semaphore::new(opts.hash_concurrency.max(1)));
    let mut set: JoinSet<(FileState, Option<String>)> = JoinSet::new();

    for e in w.entries {
        cancel.check()?;
        let path = e.path.to_string_lossy().into_owned();
        let mut state = FileState {
            path: path.clone(),
            kind: e.kind,
            size: e.size,
            mtime_ns: e.mtime_ns,
            hash: None,
        };
        if e.kind == EntryKind::Dir {
            trie.insert(state);
            continue;
        }
        if opts.max_hash_bytes.is_some_and(|m| e.size > m) {
            p.files_hashed += 1;
            p.bytes_hashed += e.size;
            trie.insert(state);
            continue;
        }
        if let Some(old) = prev.and_then(|p| p.get(&path))
            && old.size == e.size
            && old.mtime_ns == e.mtime_ns
            && old.hash.is_some()
        {
            state.hash = old.hash.clone();
            p.files_hashed += 1;
            p.bytes_hashed += e.size;
            trie.insert(state);
            continue;
        }
        let sem = Arc::clone(&sem);
        let p = e.path.clone();
        set.spawn(async move {
            let _permit = sem.acquire_owned().await;
            let res = tokio::task::spawn_blocking(move || hash_file_blocking(&p)).await;
            match res {
                Ok(Ok(h)) => {
                    let mut s = state;
                    s.hash = Some(h);
                    (s, None)
                }
                Ok(Err(e)) => (state, Some(e.to_string())),
                Err(e) => (state, Some(e.to_string())),
            }
        });
    }
    while let Some(j) = set.join_next().await {
        cancel.check()?;
        let (state, err) = j.map_err(EngineError::from)?;
        if let Some(reason) = err {
            skipped.push((state.path.clone(), reason));
        }
        p.files_hashed += 1;
        p.bytes_hashed += state.size;
        trie.insert(state);
        if last.elapsed() >= PROGRESS_INTERVAL {
            on_progress(p);
            last = std::time::Instant::now();
        }
    }
    trie.finalize();
    p.phase = Phase::Done;
    on_progress(p);
    Ok(Snapshot { trie, skipped })
}
