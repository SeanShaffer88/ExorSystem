//! Capture a bounded file snapshot, with at most N hashing jobs in flight.

use crate::cancel::Cancel;
use crate::error::{EngineError, Result};
use crate::hashing::hash_capture;
use crate::trie::{FileState, PathTrie};
use crate::walker::{Entry, EntryKind, WalkOptions, walk_progress};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use tokio::task::JoinSet;

/// Capture tuning.
#[derive(Debug, Clone)]
pub struct CaptureOptions {
    /// Walk and path budgets.
    pub walk: WalkOptions,
    /// Max files hashed concurrently (clamped to 1–8).
    pub hash_concurrency: usize,
    /// Optional per-file content read limit.
    pub max_hash_bytes: Option<u64>,
    /// Maximum logical bytes scheduled for content reads per scope.
    pub total_hash_bytes: u64,
    /// Rehash equal size/mtime files and preserve case-only siblings.
    pub strict: bool,
}

impl Default for CaptureOptions {
    fn default() -> Self {
        Self {
            walk: WalkOptions::default(),
            hash_concurrency: 4,
            max_hash_bytes: None,
            total_hash_bytes: 2 * 1024 * 1024 * 1024,
            strict: false,
        }
    }
}

/// Capture phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Enumerating directories.
    Walking,
    /// Reading file contents.
    Hashing,
    /// Finished.
    Done,
}

/// Throttled progress. Reused and unreadable files do not count as successful hashes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Progress {
    /// Current phase.
    pub phase: Phase,
    /// Files discovered (growing during the walk).
    pub files_seen: u64,
    /// File paths processed, including reuse and failures.
    pub files_processed: u64,
    /// Successfully hashed files.
    pub files_hashed: u64,
    /// Files using their previous snapshot's hash.
    pub files_reused: u64,
    /// Files not hashed due to access, change or byte limit.
    pub files_unhashed: u64,
    /// Actual bytes read for hashing (including reads that did not yield a usable hash).
    pub bytes_hashed: u64,
}

impl Progress {
    fn initial() -> Self {
        Self {
            phase: Phase::Walking,
            files_seen: 0,
            files_processed: 0,
            files_hashed: 0,
            files_reused: 0,
            files_unhashed: 0,
            bytes_hashed: 0,
        }
    }
}

const INTERVAL: std::time::Duration = std::time::Duration::from_millis(125);

/// A finalized trie plus reasons for incomplete content checks.
#[derive(Debug)]
pub struct Snapshot {
    /// Finalized trie.
    pub trie: PathTrie,
    /// Unreadable paths or missing content hashes, with the reason.
    pub skipped: Vec<(String, String)>,
    /// Number of regular files (not directory entries).
    pub files: usize,
    /// Path bytes charged by the walk.
    pub path_bytes: usize,
    /// Logical bytes scheduled for reads, including failed reads.
    pub read_budget_used: u64,
}

/// Capture `root` without progress notifications.
pub async fn capture(
    root: &Path,
    opts: &CaptureOptions,
    prev: Option<&PathTrie>,
    cancel: &Cancel,
) -> Result<Snapshot> {
    capture_with_progress(root, opts, prev, cancel, Arc::new(|_| {})).await
}

/// Capture with phase changes and at most eight additional progress events per second.
pub async fn capture_with_progress(
    root: &Path,
    opts: &CaptureOptions,
    prev: Option<&PathTrie>,
    cancel: &Cancel,
    on_progress: Arc<dyn Fn(Progress) + Send + Sync>,
) -> Result<Snapshot> {
    let mut p = Progress::initial();
    on_progress(p);
    let walking = on_progress.clone();
    let w = walk_progress(
        root,
        &opts.walk,
        cancel,
        Arc::new(move |files_seen| {
            walking(Progress {
                files_seen,
                ..Progress::initial()
            })
        }),
    )
    .await?;
    cancel.check()?;
    let files = w
        .entries
        .iter()
        .filter(|e| e.kind == EntryKind::File)
        .count();
    p.phase = Phase::Hashing;
    p.files_seen = files as u64;
    on_progress(p);
    let mut trie = if opts.strict {
        PathTrie::new_strict()
    } else {
        PathTrie::new()
    };
    let mut skipped: Vec<_> = w
        .skipped
        .into_iter()
        .map(|(p, r)| (p.to_string_lossy().into_owned(), r))
        .collect();
    // Detect folding collisions for the fast path; do not silently drop a case-only sibling.
    if !opts.strict {
        let mut seen = std::collections::HashSet::with_capacity(w.entries.len());
        for entry in &w.entries {
            let key = entry
                .path
                .to_string_lossy()
                .replace('\\', "/")
                .to_lowercase();
            if !seen.insert(key) {
                return Err(EngineError::Watch(
                    "Case-only path collision; enable strict comparison or use a smaller scope."
                        .into(),
                ));
            }
        }
    }
    let mut jobs = JoinSet::<(FileState, Result<String>)>::new();
    let mut todo = w.entries.into_iter();
    let mut exhausted = false;
    let mut budget = opts.total_hash_bytes;
    let bytes = Arc::new(AtomicU64::new(0));
    let mut last = std::time::Instant::now();
    let mut timer = tokio::time::interval(INTERVAL);
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let max_jobs = opts.hash_concurrency.clamp(1, 8);
    while !exhausted || !jobs.is_empty() {
        while !exhausted && !cancel.is_cancelled() && jobs.len() < max_jobs {
            let Some(Entry {
                path,
                kind,
                size,
                mtime_ns,
            }) = todo.next()
            else {
                exhausted = true;
                break;
            };
            let mut state = FileState {
                path: path.to_string_lossy().into_owned(),
                kind,
                size,
                mtime_ns,
                hash: None,
            };
            if kind == EntryKind::Dir {
                trie.insert(state);
                continue;
            }
            if !opts.strict
                && let Some(old) = prev.and_then(|base| base.get(&state.path))
                && old.size == size
                && old.mtime_ns == mtime_ns
                && mtime_ns != 0
                && old.hash.is_some()
            {
                state.hash = old.hash.clone();
                p.files_reused += 1;
                p.files_processed += 1;
                trie.insert(state);
            } else if opts.max_hash_bytes.is_some_and(|limit| size > limit) || size > budget {
                skipped.push((state.path.clone(), "content read budget exceeded".into()));
                p.files_unhashed += 1;
                p.files_processed += 1;
                trie.insert(state);
            } else {
                budget -= size;
                let token = cancel.clone();
                let bytes = bytes.clone();
                jobs.spawn_blocking(move || {
                    (state, hash_capture(&path, size, mtime_ns, &token, &bytes))
                });
            }
            tick(&mut p, &bytes, &on_progress, &mut last);
        }
        if cancel.is_cancelled() {
            // `spawn_blocking` work can't be aborted. Join the bounded set before allowing a new job.
            while jobs.join_next().await.is_some() {}
            return Err(EngineError::Cancelled);
        }
        if jobs.is_empty() {
            break;
        }
        tokio::select! {
            result = jobs.join_next() => {
                if let Some(result) = result {
                    let (mut state, hash) = match result {
                        Ok(result) => result,
                        Err(e) => {
                            while jobs.join_next().await.is_some() {}
                            return Err(e.into());
                        }
                    };
                    match hash {
                        Ok(hash) => { state.hash = Some(hash); p.files_hashed += 1; }
                        Err(e) => { skipped.push((state.path.clone(), e.to_string())); p.files_unhashed += 1; }
                    }
                    p.files_processed += 1;
                    trie.insert(state);
                }
            }
            _ = timer.tick() => {}
        }
        tick(&mut p, &bytes, &on_progress, &mut last);
    }
    cancel.check()?;
    trie.finalize();
    p.phase = Phase::Done;
    p.bytes_hashed = bytes.load(Relaxed);
    on_progress(p);
    Ok(Snapshot {
        trie,
        skipped,
        files,
        path_bytes: w.path_bytes,
        read_budget_used: opts.total_hash_bytes - budget,
    })
}

fn tick(
    p: &mut Progress,
    bytes: &AtomicU64,
    notify: &Arc<dyn Fn(Progress) + Send + Sync>,
    last: &mut std::time::Instant,
) {
    if last.elapsed() >= INTERVAL {
        let read = bytes.load(Relaxed);
        if p.bytes_hashed != read || p.files_processed < p.files_seen {
            p.bytes_hashed = read;
            notify(*p);
        }
        *last = std::time::Instant::now();
    }
}
