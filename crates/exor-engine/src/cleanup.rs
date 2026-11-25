//! Read-only preparation of exact, bounded cleanup plans. Filesystem paths, scope and
//! fingerprints are retained server-side; applying a plan never runs another recursive scan.

use crate::quarantine::{self, ReviewedFile};
use crate::{Cancel, EngineError, Result, policy};
use exor_storage::{PolicyAction, Store};
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Maximum findings (including unchanged policy decisions) to preflight in one review.
pub const MAX_FINDINGS: usize = 4_096;
/// Maximum files and logical bytes in one mutation.
const MAX_FILES: usize = 512;
const MAX_BYTES: u64 = 2 * 1024 * 1024 * 1024;
const MAX_PATH_BYTES: usize = 4_096;
/// Maximum number of application folders in a single approval.
pub const MAX_ROOTS: usize = 8;
/// Number of paths sent in one review page.
pub const PAGE_SIZE: usize = 40;

/// An excluded file, folder, or system item and the reason it will not be moved.
#[derive(Debug, Clone, Serialize)]
pub struct Exclusion {
    /// Recorded or discovered path.
    pub path: String,
    /// What made this item ineligible.
    pub reason: String,
}

/// Read-only review data plus the exact identities held by the worker.
#[derive(Debug)]
pub struct CleanupPlan {
    /// Approved canonical roots, displayed to the user before applying.
    pub roots: Vec<PathBuf>,
    /// Associations for the append-only change ledger.
    pub session_id: Option<i64>,
    /// Eligible immutable set. A later discovery is never appended to this set.
    pub files: Vec<ReviewedFile>,
    /// Candidates which will be left alone.
    pub exclusions: Vec<Exclusion>,
    /// Total logical byte length of the files.
    pub bytes: u64,
    remaining_read_bytes: u64,
}

fn refused(message: impl Into<String>) -> EngineError {
    EngineError::Quarantine(message.into())
}

fn canonical_roots(roots: &[PathBuf], protected: &Path) -> Result<Vec<PathBuf>> {
    if roots.is_empty() || roots.len() > MAX_ROOTS {
        return Err(refused(format!(
            "Select one to {MAX_ROOTS} eligible application folders."
        )));
    }
    let protected = protected
        .canonicalize()
        .map_err(crate::error::io_err(protected))?;
    let mut canonical = BTreeSet::new();
    for root in roots {
        if root.to_str().is_none_or(|s| s.len() > MAX_PATH_BYTES) || !quarantine::plain_dirs(root)?
        {
            return Err(refused(
                "Application folder is missing, redirected or too long.",
            ));
        }
        let full = root.canonicalize().map_err(crate::error::io_err(root))?;
        if protected.starts_with(&full) || full.starts_with(&protected) {
            return Err(refused("ExorSystem profile folders are not eligible."));
        }
        canonical.insert(full);
    }
    Ok(canonical.into_iter().collect())
}

impl CleanupPlan {
    fn new(roots: &[PathBuf], protected: &Path, session_id: Option<i64>) -> Result<Self> {
        Ok(Self {
            roots: canonical_roots(roots, protected)?,
            session_id,
            files: vec![],
            exclusions: vec![],
            bytes: 0,
            remaining_read_bytes: MAX_BYTES,
        })
    }

    fn exclude(&mut self, path: &Path, reason: impl Into<String>) {
        let text = path.to_string_lossy();
        let mut len = text.len().min(MAX_PATH_BYTES);
        while !text.is_char_boundary(len) {
            len -= 1;
        }
        self.exclusions.push(Exclusion {
            path: text[..len].to_string(),
            reason: reason.into(),
        });
    }

    fn include(
        &mut self,
        path: &Path,
        protected: &Path,
        hash: Option<&str>,
        cancel: &Cancel,
    ) -> Result<()> {
        if path.to_str().is_none_or(|s| s.len() > MAX_PATH_BYTES) {
            self.exclude(path, "Path cannot be represented within the review budget.");
            return Ok(());
        }
        // Size alone is never identity. Bound the read even if the path is swapped or grows.
        let max = match quarantine::plain(path, false) {
            Ok(Some(meta)) if meta.len() <= self.remaining_read_bytes => meta.len(),
            Ok(Some(_)) => {
                self.exclude(path, "File exceeds the remaining review byte budget.");
                return Ok(());
            }
            Ok(None) => {
                self.exclude(path, "File is no longer present.");
                return Ok(());
            }
            Err(e) => {
                self.exclude(path, e.to_string());
                return Ok(());
            }
        };
        self.remaining_read_bytes -= max;
        let reviewed =
            quarantine::review_file_bounded(path, &self.roots, protected, hash, max, cancel);
        match reviewed {
            Ok(file) => {
                if self.files.len() >= MAX_FILES {
                    return Err(refused(format!(
                        "More than {MAX_FILES} eligible files; choose a smaller scope."
                    )));
                }
                self.bytes += file.size();
                self.files.push(file);
            }
            Err(EngineError::Cancelled) => return Err(EngineError::Cancelled),
            Err(e) => self.exclude(path, e.to_string()),
        }
        Ok(())
    }

    /// Recheck that the current eligible user selections still resolve to the reviewed roots.
    pub fn check_roots(&self, roots: &[PathBuf], protected: &Path) -> Result<()> {
        if self.roots != canonical_roots(roots, protected)? {
            return Err(refused(
                "The approved application folders changed. Build a new plan.",
            ));
        }
        Ok(())
    }
}

/// Prepare policy-selected files with evidence that this session created them, not merely
/// observed or modified them. A prior installation's files and system entries are review-only.
pub fn prepare_policy(
    store: &Store,
    session_id: i64,
    rules: &[exor_storage::PolicyRule],
    roots: &[PathBuf],
    protected: &Path,
    cancel: &Cancel,
) -> Result<CleanupPlan> {
    let mut plan = CleanupPlan::new(roots, protected, Some(session_id))?;
    let mut offset = 0;
    loop {
        cancel.check()?;
        let page = store.session_nodes_page(session_id, None, offset, 100)?;
        if usize::try_from(page.total).unwrap_or(usize::MAX) > MAX_FINDINGS {
            return Err(refused(
                "Too many recorded candidates to review safely; no cleanup plan was created.",
            ));
        }
        if page.nodes.is_empty() {
            break;
        }
        offset += page.nodes.len();
        let candidates = page
            .nodes
            .iter()
            .map(|n| policy::candidate_from_node(store, n))
            .collect::<Result<Vec<_>>>()?;
        for decision in policy::simulate(rules, &candidates)?.decisions {
            cancel.check()?;
            let path = Path::new(&decision.path);
            if decision.action != PolicyAction::Quarantine {
                plan.exclude(
                    path,
                    format!("{}: {}", decision.action.as_str(), decision.reason),
                );
            } else if let Some(hash) = store.cleanup_hash(session_id, &decision.path)? {
                plan.include(path, protected, Some(&hash), cancel)?;
            } else {
                plan.exclude(
                    path,
                    "No current recorded fingerprint for a file created by this session.",
                );
            }
        }
    }
    Ok(plan)
}

/// Prepare the current files of explicitly selected app folders. Unreadable or redirected
/// locations are listed as excluded. Directory entries themselves are never cleanup targets.
pub fn prepare_leftovers(
    roots: &[PathBuf],
    protected: &Path,
    session_id: Option<i64>,
    cancel: &Cancel,
) -> Result<CleanupPlan> {
    let mut plan = CleanupPlan::new(roots, protected, session_id)?;
    let mut pending: Vec<_> = roots.iter().cloned().map(|p| (p, 0)).collect();
    let mut seen = BTreeSet::new();
    let mut visited = 0;
    while let Some((dir, depth)) = pending.pop() {
        cancel.check()?;
        if depth > 64 || !matches!(quarantine::plain_dirs(&dir), Ok(true)) {
            plan.exclude(
                &dir,
                "Folder is redirected, unreadable or beyond the depth limit.",
            );
            continue;
        }
        let entries = match std::fs::read_dir(&dir) {
            Ok(rd) => rd,
            Err(e) => {
                plan.exclude(&dir, format!("Unable to list folder: {e}"));
                continue;
            }
        };
        for entry in entries {
            cancel.check()?;
            visited += 1;
            if visited > MAX_FINDINGS {
                return Err(refused(
                    "Folder exceeds the review entry budget. Choose a smaller scope.",
                ));
            }
            let entry = match entry {
                Ok(entry) => entry,
                Err(e) => {
                    plan.exclude(&dir, format!("Unable to read an entry: {e}"));
                    continue;
                }
            };
            let path = entry.path();
            if path.to_str().is_none_or(|s| s.len() > MAX_PATH_BYTES) {
                plan.exclude(&path, "Path exceeds the review budget.");
                continue;
            }
            match quarantine::plain(&path, true) {
                Ok(Some(_)) => pending.push((path, depth + 1)),
                _ if seen.insert(path.clone()) => plan.include(&path, protected, None, cancel)?,
                _ => {}
            }
        }
    }
    plan.files.sort_by(|a, b| a.path().cmp(b.path()));
    plan.exclusions.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(plan)
}

/// A bounded review that is not yet authorized for mutation.
#[derive(Serialize)]
pub struct ReviewHeader {
    /// Process-local, single-use review ID.
    pub id: u64,
    /// Resolved application folders.
    pub roots: Vec<PathBuf>,
    /// Number of files that must be explicitly approved.
    pub files: usize,
    /// Logical size of included files.
    pub bytes: u64,
    /// Findings that will be left alone.
    pub excluded: usize,
    /// Lifetime from preparation, not extended by paging.
    pub expires_in_seconds: u64,
}

/// A file in a review page, without internal identity or mutable authorization.
#[derive(Serialize)]
pub struct ReviewFile {
    /// Exact original path.
    pub path: PathBuf,
    /// Logical size at preflight time.
    pub size: u64,
}

/// One bounded page of either included or excluded paths.
#[derive(Serialize)]
pub struct ReviewPage {
    /// Position in the selected list.
    pub offset: usize,
    /// Total entries in the selected list.
    pub total: usize,
    /// Continuation, if more paths are available.
    pub next_offset: Option<usize>,
    /// Included paths on this page.
    pub files: Vec<ReviewFile>,
    /// Excluded paths and reasons on this page.
    pub exclusions: Vec<Exclusion>,
}

struct PendingReview<C> {
    context: C,
    plan: CleanupPlan,
    created: std::time::Instant,
    viewed: BTreeSet<usize>,
}

struct Drafts<C> {
    next: u64,
    pending: std::collections::BTreeMap<u64, PendingReview<C>>,
}

/// Process-local review store. Two expiring drafts, small pages, and atomic consumption;
/// the caller still has to recheck the context and hold the desktop's mutation gate.
pub struct Reviews<C>(std::sync::Mutex<Drafts<C>>);

impl<C> Default for Reviews<C> {
    fn default() -> Self {
        Self(std::sync::Mutex::new(Drafts {
            next: 0,
            pending: Default::default(),
        }))
    }
}

const REVIEW_LIFETIME: std::time::Duration = std::time::Duration::from_secs(600);

fn expired_review() -> EngineError {
    refused("This review expired or was already used. Build a new cleanup plan.")
}

impl<C> Reviews<C> {
    /// Retain an immutable review and contextual data chosen by the native controller.
    pub fn insert(&self, context: C, plan: CleanupPlan) -> Result<ReviewHeader> {
        let mut drafts = self.0.lock().map_err(|_| EngineError::Poisoned)?;
        drafts
            .pending
            .retain(|_, d| d.created.elapsed() < REVIEW_LIFETIME);
        if drafts.pending.len() >= 2 {
            drafts.pending.pop_first();
        }
        drafts.next = drafts.next.checked_add(1).ok_or_else(expired_review)?;
        let id = drafts.next;
        let header = ReviewHeader {
            id,
            roots: plan.roots.clone(),
            files: plan.files.len(),
            bytes: plan.bytes,
            excluded: plan.exclusions.len(),
            expires_in_seconds: REVIEW_LIFETIME.as_secs(),
        };
        drafts.pending.insert(
            id,
            PendingReview {
                context,
                plan,
                created: std::time::Instant::now(),
                viewed: BTreeSet::new(),
            },
        );
        Ok(header)
    }

    /// Read a page by exact offset; only nonempty included pages count toward approval.
    pub fn page(&self, id: u64, offset: usize, excluded: bool) -> Result<ReviewPage> {
        let mut drafts = self.0.lock().map_err(|_| EngineError::Poisoned)?;
        let draft = drafts
            .pending
            .get_mut(&id)
            .filter(|d| d.created.elapsed() < REVIEW_LIFETIME)
            .ok_or_else(expired_review)?;
        let total = if excluded {
            draft.plan.exclusions.len()
        } else {
            draft.plan.files.len()
        };
        if !offset.is_multiple_of(PAGE_SIZE) || (offset != 0 && offset >= total) {
            return Err(refused("Invalid review page."));
        }
        let end = total.min(offset + PAGE_SIZE);
        let (files, exclusions) = if excluded {
            (vec![], draft.plan.exclusions[offset..end].to_vec())
        } else {
            if total > 0 {
                draft.viewed.insert(offset / PAGE_SIZE);
            }
            (
                draft.plan.files[offset..end]
                    .iter()
                    .map(|f| ReviewFile {
                        path: f.path().to_path_buf(),
                        size: f.size(),
                    })
                    .collect(),
                vec![],
            )
        };
        Ok(ReviewPage {
            offset,
            total,
            next_offset: (end < total).then_some(end),
            files,
            exclusions,
        })
    }

    /// Consume exactly one approval. Wrong count, expiry or unviewed pages never returns paths.
    pub fn take_approved(&self, id: u64, count: usize) -> Result<(C, CleanupPlan)> {
        let mut drafts = self.0.lock().map_err(|_| EngineError::Poisoned)?;
        let draft = drafts
            .pending
            .get(&id)
            .filter(|d| d.created.elapsed() < REVIEW_LIFETIME)
            .ok_or_else(expired_review)?;
        if count == 0
            || count != draft.plan.files.len()
            || draft.viewed.len() != count.div_ceil(PAGE_SIZE)
        {
            return Err(refused(
                "Review every included file page and approve the exact count before moving files.",
            ));
        }
        let draft = drafts.pending.remove(&id).ok_or_else(expired_review)?;
        Ok((draft.context, draft.plan))
    }

    /// The included paths of a live review, for read-only checks before applying (such as which
    /// programs hold them open). Doesn't count as viewing a page or consume the approval.
    pub fn included_paths(&self, id: u64) -> Result<Vec<PathBuf>> {
        let drafts = self.0.lock().map_err(|_| EngineError::Poisoned)?;
        let draft = drafts
            .pending
            .get(&id)
            .filter(|d| d.created.elapsed() < REVIEW_LIFETIME)
            .ok_or_else(expired_review)?;
        Ok(draft
            .plan
            .files
            .iter()
            .map(|f| f.path().to_path_buf())
            .collect())
    }

    /// Forget a review without touching any files.
    pub fn discard(&self, id: u64) {
        if let Ok(mut drafts) = self.0.lock() {
            drafts.pending.remove(&id);
        }
    }
}

/// Re-evaluate just the approved files after other observations may have arrived. Newly
/// discovered paths are never added and the retained fingerprint must still match the ledger.
pub fn recheck_policy(
    store: &Store,
    plan: &CleanupPlan,
    session: i64,
    rules: &[exor_storage::PolicyRule],
) -> Result<()> {
    let changed =
        || refused("Recorded eligibility changed after review. Build a new cleanup plan.");
    for file in &plan.files {
        let path = file.path().to_str().ok_or_else(changed)?;
        let node = store
            .node_by_path(path)?
            .filter(|n| n.first_detected_in == Some(session))
            .ok_or_else(changed)?;
        let decision = policy::simulate_nodes(store, rules, &[node])?;
        if decision.quarantine != 1
            || store.cleanup_hash(session, path)?.as_deref() != Some(file.hash())
        {
            return Err(changed());
        }
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn approval_requires_every_included_page_and_cannot_be_replayed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("owned");
        let profile = dir.path().join("profile");
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(&profile).unwrap();
        for n in 0..PAGE_SIZE + 1 {
            std::fs::write(root.join(n.to_string()), b"fixture").unwrap();
        }
        let cancel = Cancel::new();
        let prep = || prepare_leftovers(std::slice::from_ref(&root), &profile, None, &cancel);
        let reviews = Reviews::default();
        let plan = reviews.insert("context", prep().unwrap()).unwrap();
        assert!(reviews.take_approved(plan.id, plan.files).is_err());
        assert!(reviews.page(plan.id, usize::MAX, false).is_err());
        assert_eq!(
            reviews.page(plan.id, 0, false).unwrap().next_offset,
            Some(PAGE_SIZE)
        );
        reviews.page(plan.id, 0, true).unwrap();
        assert!(reviews.take_approved(plan.id, plan.files).is_err());
        reviews.page(plan.id, PAGE_SIZE, false).unwrap();
        assert!(reviews.take_approved(plan.id, plan.files - 1).is_err());
        std::thread::scope(|threads| {
            let a = threads.spawn(|| reviews.take_approved(plan.id, plan.files).is_ok());
            let b = threads.spawn(|| reviews.take_approved(plan.id, plan.files).is_ok());
            assert_ne!(a.join().unwrap(), b.join().unwrap());
        });
        let replaced = reviews.insert("old", prep().unwrap()).unwrap();
        let removed = reviews.insert("discard", prep().unwrap()).unwrap();
        let newer = reviews.insert("new", prep().unwrap()).unwrap();
        assert!(reviews.page(replaced.id, 0, false).is_err());
        reviews.discard(removed.id);
        assert!(reviews.page(removed.id, 0, false).is_err());
        reviews
            .0
            .lock()
            .unwrap()
            .pending
            .get_mut(&newer.id)
            .unwrap()
            .created -= REVIEW_LIFETIME;
        assert!(reviews.page(newer.id, 0, false).is_err());
        assert!(reviews.take_approved(newer.id, newer.files).is_err());
        cancel.cancel();
        assert!(matches!(prep(), Err(EngineError::Cancelled)));
        assert_eq!(std::fs::read_dir(root).unwrap().count(), PAGE_SIZE + 1);
    }
}
