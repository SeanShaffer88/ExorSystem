//! Install-session monitor: noise-filtered events become nodes, ledger entries and attribution edges.

use crate::cancel::Cancel;
use crate::collectors::{ServiceChange, TaskChange};
use crate::error::{EngineError, Result};
use crate::events::{FsReason, RawEvent, RegOp};
use crate::noise::{NoiseFilter, Verdict};
use crate::registry::RegistryChange;
use crate::trie::{Change, ChangeType};
use crate::walker::EntryKind;
use exor_storage::{
    ChangeKind, ChangeSource, Evidence, NewLedgerEntry, NewNode, NodeType, RelationshipType,
    SessionStatus, Store,
};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::mpsc;

/// Counters from a monitoring run.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MonitorStats {
    /// Events received.
    pub received: usize,
    /// Events that passed the noise filter.
    pub kept: usize,
    /// Events dropped as noise.
    pub dropped: usize,
}

/// A manifest entry (e.g. from an MSI table) that confirms an artifact belongs to the session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestEntry {
    /// Path or key.
    pub path: String,
    /// Kind.
    pub node_type: NodeType,
    /// Expected hash, if the manifest carries one.
    pub hash: Option<String>,
}

/// Monitors one installation session.
pub struct SessionMonitor {
    store: Arc<Mutex<Store>>,
    session_id: i64,
    installer_node: Option<i64>,
}

fn lock(m: &Mutex<Store>) -> Result<MutexGuard<'_, Store>> {
    m.lock().map_err(|_| EngineError::Poisoned)
}

impl SessionMonitor {
    /// Begin a session and register the installer as the root of attribution.
    pub fn start(
        store: Arc<Mutex<Store>>,
        app_name: &str,
        installer_path: Option<&str>,
        fingerprint_family: Option<&str>,
    ) -> Result<Self> {
        let (session_id, installer_node) = {
            let s = lock(&store)?;
            let id = s.begin_session(app_name, installer_path, fingerprint_family)?;
            let node = match installer_path {
                Some(p) => Some(s.upsert_node(&NewNode {
                    node_type: NodeType::File,
                    path: p.to_string(),
                    hash: None,
                    acl_fingerprint: None,
                    first_detected_in: Some(id),
                })?),
                None => None,
            };
            (id, node)
        };
        Ok(Self {
            store,
            session_id,
            installer_node,
        })
    }

    /// Session id.
    pub fn session_id(&self) -> i64 {
        self.session_id
    }

    /// Consume events until the source closes or `cancel` fires.
    pub async fn run(
        &self,
        mut rx: mpsc::Receiver<RawEvent>,
        mut noise: NoiseFilter,
        cancel: &Cancel,
    ) -> Result<MonitorStats> {
        let mut stats = MonitorStats::default();
        loop {
            let ev = tokio::select! {
                () = cancel.cancelled() => return Err(EngineError::Cancelled),
                ev = rx.recv() => match ev { Some(e) => e, None => break },
            };
            stats.received += 1;
            match noise.judge(&ev) {
                Verdict::Drop(_) => stats.dropped += 1,
                Verdict::Keep => {
                    stats.kept += 1;
                    let s = lock(&self.store)?;
                    self.record_event(&s, &ev)?;
                }
            }
        }
        Ok(stats)
    }

    fn link(&self, s: &Store, node: i64, kind: RelationshipType, ev: Evidence) -> Result<()> {
        if let Some(inst) = self.installer_node
            && inst != node
        {
            s.add_edge(inst, node, kind, ev)?;
        }
        Ok(())
    }

    fn record_event(&self, s: &Store, ev: &RawEvent) -> Result<()> {
        let (path, node_type, change, source) = match ev {
            RawEvent::Fs { path, reason, .. } => (
                path.clone(),
                NodeType::File,
                match reason {
                    FsReason::Create | FsReason::Rename => ChangeKind::Created,
                    FsReason::Delete => ChangeKind::Deleted,
                    FsReason::Modify => ChangeKind::Modified,
                },
                ChangeSource::UsnJournal,
            ),
            RawEvent::Registry {
                key,
                value_name,
                op,
                ..
            } => {
                let (p, t) = match value_name {
                    Some(v) => (format!("{key}::{v}"), NodeType::RegistryValue),
                    None => (key.clone(), NodeType::RegistryKey),
                };
                let existed = s.node_by_path(&p)?.is_some();
                let c = match op {
                    RegOp::CreateKey => ChangeKind::Created,
                    RegOp::DeleteKey | RegOp::DeleteValue => ChangeKind::Deleted,
                    RegOp::SetValue if existed => ChangeKind::Modified,
                    RegOp::SetValue => ChangeKind::Created,
                };
                (p, t, c, ChangeSource::RegistryCallback)
            }
        };
        let node_id = if change == ChangeKind::Deleted {
            s.node_by_path(&path)?.map(|n| n.node_id)
        } else {
            Some(s.upsert_node(&NewNode {
                node_type,
                path: path.clone(),
                hash: None,
                acl_fingerprint: None,
                first_detected_in: Some(self.session_id),
            })?)
        };
        if let Some(id) = node_id {
            match change {
                ChangeKind::Created => {
                    self.link(s, id, RelationshipType::CreatedBy, Evidence::Temporal)?
                }
                ChangeKind::Modified => {
                    self.link(s, id, RelationshipType::ModifiedBy, Evidence::Temporal)?
                }
                _ => {}
            }
        }
        s.append_ledger(&NewLedgerEntry {
            session_id: Some(self.session_id),
            node_id,
            path,
            change_kind: change,
            old_hash: None,
            new_hash: None,
            source,
        })?;
        Ok(())
    }

    /// Attribute snapshot-diff changes (no event evidence): heuristic confidence 0.3, which is
    /// raised automatically when events or a manifest back the same edge.
    pub fn attribute_diff(&self, changes: &[Change]) -> Result<usize> {
        let s = lock(&self.store)?;
        let mut n = 0;
        for c in changes.iter().filter(|c| c.kind == EntryKind::File) {
            let (kind, rel) = match c.change {
                ChangeType::Added => (ChangeKind::Created, Some(RelationshipType::CreatedBy)),
                ChangeType::Modified => (ChangeKind::Modified, Some(RelationshipType::ModifiedBy)),
                ChangeType::Removed => (ChangeKind::Deleted, None),
            };
            let node_id = if rel.is_some() {
                Some(s.upsert_node(&NewNode {
                    node_type: NodeType::File,
                    path: c.path.clone(),
                    hash: c.new_hash.clone(),
                    acl_fingerprint: None,
                    first_detected_in: Some(self.session_id),
                })?)
            } else {
                s.node_by_path(&c.path)?.map(|x| x.node_id)
            };
            if let (Some(id), Some(rel)) = (node_id, rel) {
                self.link(&s, id, rel, Evidence::Heuristic)?;
            }
            s.append_ledger(&NewLedgerEntry {
                session_id: Some(self.session_id),
                node_id,
                path: c.path.clone(),
                change_kind: kind,
                old_hash: c.old_hash.clone(),
                new_hash: c.new_hash.clone(),
                source: ChangeSource::SnapshotDiff,
            })?;
            n += 1;
        }
        Ok(n)
    }

    /// Attribute metadata-only registry changes as snapshot evidence, not driver observations.
    /// Removed keys contribute history but do not create new cleanup candidates.
    pub fn attribute_registry_keys(&self, changes: &[RegistryChange]) -> Result<usize> {
        let s = lock(&self.store)?;
        for c in changes {
            let (kind, relation) = match c.change {
                ChangeType::Added => (ChangeKind::Created, Some(RelationshipType::CreatedBy)),
                ChangeType::Modified => (ChangeKind::Modified, Some(RelationshipType::ModifiedBy)),
                ChangeType::Removed => (ChangeKind::Deleted, None),
            };
            let node_id = if let Some(relation) = relation {
                let id = s.upsert_node(&NewNode {
                    node_type: NodeType::RegistryKey,
                    path: c.path.clone(),
                    hash: None,
                    acl_fingerprint: None,
                    first_detected_in: Some(self.session_id),
                })?;
                self.link(&s, id, relation, Evidence::Heuristic)?;
                Some(id)
            } else {
                s.node_by_path(&c.path)?.map(|node| node.node_id)
            };
            s.append_ledger(&NewLedgerEntry {
                session_id: Some(self.session_id),
                node_id,
                path: c.path.clone(),
                change_kind: kind,
                old_hash: None,
                new_hash: None,
                source: ChangeSource::SnapshotDiff,
            })?;
        }
        Ok(changes.len())
    }

    /// Attribute service changes. Added/modified services become `service` nodes (heuristic 0.3);
    /// when the service's executable is already a known node, the service `depends_on` it.
    pub fn attribute_services(&self, changes: &[ServiceChange]) -> Result<usize> {
        let s = lock(&self.store)?;
        let mut n = 0;
        for c in changes {
            let path = format!("service:{}", c.service.name);
            let kind = match c.change {
                ChangeType::Added => ChangeKind::Created,
                ChangeType::Modified => ChangeKind::Modified,
                ChangeType::Removed => ChangeKind::Deleted,
            };
            let node_id = if c.change == ChangeType::Removed {
                s.node_by_path(&path)?.map(|x| x.node_id)
            } else {
                let id = s.upsert_node(&NewNode {
                    node_type: NodeType::Service,
                    path: path.clone(),
                    hash: None,
                    acl_fingerprint: None,
                    first_detected_in: Some(self.session_id),
                })?;
                let rel = if c.change == ChangeType::Added {
                    RelationshipType::CreatedBy
                } else {
                    RelationshipType::ModifiedBy
                };
                self.link(&s, id, rel, Evidence::Heuristic)?;
                if let Some(exe) = image_path(&c.service.binary_path)
                    && let Some(f) = s.node_by_path(&exe)?
                {
                    s.add_edge(
                        f.node_id,
                        id,
                        RelationshipType::DependsOn,
                        Evidence::Temporal,
                    )?;
                }
                Some(id)
            };
            s.append_ledger(&NewLedgerEntry {
                session_id: Some(self.session_id),
                node_id,
                path,
                change_kind: kind,
                old_hash: None,
                new_hash: None,
                source: ChangeSource::SnapshotDiff,
            })?;
            n += 1;
        }
        Ok(n)
    }

    /// Attribute scheduled-task changes (same rules as services).
    pub fn attribute_tasks(&self, changes: &[TaskChange]) -> Result<usize> {
        let s = lock(&self.store)?;
        let mut n = 0;
        for c in changes {
            let path = format!("task:{}", c.task.path);
            let kind = match c.change {
                ChangeType::Added => ChangeKind::Created,
                ChangeType::Modified => ChangeKind::Modified,
                ChangeType::Removed => ChangeKind::Deleted,
            };
            let node_id = if c.change == ChangeType::Removed {
                s.node_by_path(&path)?.map(|x| x.node_id)
            } else {
                let id = s.upsert_node(&NewNode {
                    node_type: NodeType::Task,
                    path: path.clone(),
                    hash: None,
                    acl_fingerprint: None,
                    first_detected_in: Some(self.session_id),
                })?;
                let rel = if c.change == ChangeType::Added {
                    RelationshipType::CreatedBy
                } else {
                    RelationshipType::ModifiedBy
                };
                self.link(&s, id, rel, Evidence::Heuristic)?;
                if let Some(exe) = c.task.command.as_deref().and_then(image_path)
                    && let Some(f) = s.node_by_path(&exe)?
                {
                    s.add_edge(
                        f.node_id,
                        id,
                        RelationshipType::DependsOn,
                        Evidence::Temporal,
                    )?;
                }
                Some(id)
            };
            s.append_ledger(&NewLedgerEntry {
                session_id: Some(self.session_id),
                node_id,
                path,
                change_kind: kind,
                old_hash: None,
                new_hash: None,
                source: ChangeSource::SnapshotDiff,
            })?;
            n += 1;
        }
        Ok(n)
    }

    /// Confirm artifacts via a manifest: confidence 1.0.
    pub fn ingest_manifest(&self, entries: &[ManifestEntry]) -> Result<usize> {
        let s = lock(&self.store)?;
        for e in entries {
            let id = s.upsert_node(&NewNode {
                node_type: e.node_type,
                path: e.path.clone(),
                hash: e.hash.clone(),
                acl_fingerprint: None,
                first_detected_in: Some(self.session_id),
            })?;
            self.link(&s, id, RelationshipType::CreatedBy, Evidence::Manifest)?;
            s.append_ledger(&NewLedgerEntry {
                session_id: Some(self.session_id),
                node_id: Some(id),
                path: e.path.clone(),
                change_kind: ChangeKind::Created,
                old_hash: None,
                new_hash: e.hash.clone(),
                source: ChangeSource::Manifest,
            })?;
        }
        Ok(entries.len())
    }

    /// Close the session.
    pub fn finish(&self, status: SessionStatus) -> Result<()> {
        Ok(lock(&self.store)?.end_session(self.session_id, status)?)
    }
}

/// Executable path from a command line: `"C:\\a b\\x.exe" /arg` or `C:\\a\\x.exe /arg`.
fn image_path(cmd: &str) -> Option<String> {
    let c = cmd.trim();
    if let Some(rest) = c.strip_prefix('"') {
        return rest
            .split('"')
            .next()
            .filter(|p| !p.is_empty())
            .map(str::to_string);
    }
    let lower = c.to_lowercase();
    let end = lower
        .find(".exe")
        .map(|i| i + 4)
        .or_else(|| lower.find(".sys").map(|i| i + 4));
    match end {
        Some(e) => Some(c[..e].to_string()),
        None => c.split_whitespace().next().map(str::to_string),
    }
}
