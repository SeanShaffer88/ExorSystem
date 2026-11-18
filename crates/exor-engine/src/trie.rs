//! Merkle-style path trie and differential diff.
//!
//! Every node stores a digest of its own state plus its children, so identical
//! subtrees are skipped in O(1) during [`diff`]. Path components are compared
//! case-insensitively by default, with a strict, spelling-preserving mode.

use crate::walker::EntryKind;
use std::collections::BTreeMap;

/// State recorded per path.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileState {
    /// Display path as observed.
    pub path: String,
    /// Kind.
    pub kind: EntryKind,
    /// Size in bytes.
    pub size: u64,
    /// Modified time (ns since epoch).
    pub mtime_ns: i64,
    /// BLAKE3 hex when hashed.
    pub hash: Option<String>,
}

impl FileState {
    fn fingerprint(&self, h: &mut blake3::Hasher) {
        h.update(&[match self.kind {
            EntryKind::File => 1,
            EntryKind::Dir => 2,
        }]);
        if self.kind == EntryKind::Dir {
            return;
        }
        h.update(&self.size.to_le_bytes());
        match &self.hash {
            Some(x) => {
                h.update(b"h");
                h.update(x.as_bytes());
            }
            None => {
                h.update(b"m");
                h.update(&self.mtime_ns.to_le_bytes());
            }
        }
    }
}

#[derive(Debug, Clone, Default)]
struct TrieNode {
    children: BTreeMap<String, TrieNode>,
    state: Option<FileState>,
    digest: [u8; 32],
}

impl TrieNode {
    fn finalize(&mut self) {
        let mut h = blake3::Hasher::new();
        match &self.state {
            Some(s) => s.fingerprint(&mut h),
            None => {
                h.update(&[0]);
            }
        }
        for (name, child) in &mut self.children {
            child.finalize();
            h.update(&(name.len() as u64).to_le_bytes());
            h.update(name.as_bytes());
            h.update(&child.digest);
        }
        self.digest = *h.finalize().as_bytes();
    }
}

/// Trie of path states.
#[derive(Debug, Clone, Default)]
pub struct PathTrie {
    root: TrieNode,
    len: usize,
    finalized: bool,
    strict: bool,
}

fn components(path: &str, strict: bool) -> impl Iterator<Item = String> + '_ {
    path.split(move |ch| ch == '/' || (ch == '\\' && (!strict || cfg!(windows))))
        .filter(|c| !c.is_empty())
        .map(move |part| {
            if strict {
                part.to_owned()
            } else {
                part.to_lowercase()
            }
        })
}

impl PathTrie {
    /// Empty trie.
    pub fn new() -> Self {
        Self::default()
    }

    /// Preserve case-only siblings rather than folding their path keys.
    pub fn new_strict() -> Self {
        Self {
            strict: true,
            ..Self::default()
        }
    }

    /// Insert or replace a state. Call [`PathTrie::finalize`] before diffing.
    pub fn insert(&mut self, state: FileState) {
        let mut node = &mut self.root;
        for c in components(&state.path, self.strict) {
            node = node.children.entry(c).or_default();
        }
        if node.state.is_none() {
            self.len += 1;
        }
        node.state = Some(state);
        self.finalized = false;
    }

    /// Look up a path.
    pub fn get(&self, path: &str) -> Option<&FileState> {
        let mut node = &self.root;
        for c in components(path, self.strict) {
            node = node.children.get(&c)?;
        }
        node.state.as_ref()
    }

    /// Number of recorded paths.
    pub fn len(&self) -> usize {
        self.len
    }

    /// True when empty.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Compute subtree digests.
    pub fn finalize(&mut self) {
        self.root.finalize();
        self.finalized = true;
    }

    /// Root digest (hex). Only meaningful after `finalize`.
    pub fn root_digest(&self) -> String {
        blake3::Hash::from_bytes(self.root.digest)
            .to_hex()
            .to_string()
    }

    /// All recorded states in path order.
    pub fn states(&self) -> Vec<&FileState> {
        fn walk<'a>(n: &'a TrieNode, out: &mut Vec<&'a FileState>) {
            if let Some(s) = &n.state {
                out.push(s);
            }
            for c in n.children.values() {
                walk(c, out);
            }
        }
        let mut v = Vec::with_capacity(self.len);
        walk(&self.root, &mut v);
        v
    }
}

/// Kind of difference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ChangeType {
    /// Present only in the new snapshot.
    Added,
    /// Present only in the old snapshot.
    Removed,
    /// Present in both with different content.
    Modified,
}

/// One difference between snapshots.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Change {
    /// Path as displayed in the newer (or, for removals, older) snapshot.
    pub path: String,
    /// Kind of change.
    pub change: ChangeType,
    /// Entry kind.
    pub kind: EntryKind,
    /// Hash before.
    pub old_hash: Option<String>,
    /// Hash after.
    pub new_hash: Option<String>,
}

fn state_fp(s: &FileState) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    s.fingerprint(&mut h);
    *h.finalize().as_bytes()
}

fn diff_node(a: Option<&TrieNode>, b: Option<&TrieNode>, out: &mut Vec<Change>) {
    if let (Some(x), Some(y)) = (a, b)
        && x.digest == y.digest
    {
        return;
    }
    let sa = a.and_then(|n| n.state.as_ref());
    let sb = b.and_then(|n| n.state.as_ref());
    match (sa, sb) {
        (None, Some(s)) => out.push(Change {
            path: s.path.clone(),
            change: ChangeType::Added,
            kind: s.kind,
            old_hash: None,
            new_hash: s.hash.clone(),
        }),
        (Some(s), None) => out.push(Change {
            path: s.path.clone(),
            change: ChangeType::Removed,
            kind: s.kind,
            old_hash: s.hash.clone(),
            new_hash: None,
        }),
        (Some(x), Some(y)) if state_fp(x) != state_fp(y) => out.push(Change {
            path: y.path.clone(),
            change: ChangeType::Modified,
            kind: y.kind,
            old_hash: x.hash.clone(),
            new_hash: y.hash.clone(),
        }),
        _ => {}
    }
    let empty = BTreeMap::new();
    let ca = a.map_or(&empty, |n| &n.children);
    let cb = b.map_or(&empty, |n| &n.children);
    let mut keys: Vec<&String> = ca.keys().chain(cb.keys()).collect();
    keys.sort();
    keys.dedup();
    for k in keys {
        diff_node(ca.get(k), cb.get(k), out);
    }
}

/// Differential comparison. Both tries must be finalized (done automatically by `capture`).
pub fn diff(old: &PathTrie, new: &PathTrie) -> Vec<Change> {
    let mut out = Vec::new();
    diff_node(Some(&old.root), Some(&new.root), &mut out);
    out
}
