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
    let w = walk(root, &opts.walk, cancel).await?;
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
            trie.insert(state);
            continue;
        }
        if let Some(old) = prev.and_then(|p| p.get(&path))
            && old.size == e.size
            && old.mtime_ns == e.mtime_ns
            && old.hash.is_some()
        {
            state.hash = old.hash.clone();
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
        trie.insert(state);
    }
    trie.finalize();
    Ok(Snapshot { trie, skipped })
}
