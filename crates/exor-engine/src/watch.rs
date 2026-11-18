//! Install watch: baseline before an installer runs, diff after, and record the evidence.
//!
//! Files and selected registry, service, and scheduled-task metadata are observed independently.
//! Unreadable scopes on either side are skipped, not mistaken for empty snapshots.

use crate::cancel::Cancel;
use crate::collectors::{ServiceInfo, TaskInfo, diff_services, diff_tasks};
use crate::error::{EngineError, Result, io_err};
use crate::monitor::{SessionMonitor, WatchChanges};
use crate::registry::{RegistryKeyState, RegistryScope, RegistryScopes, diff_registry_keys};
use crate::snapshot::{CaptureOptions, capture};
use crate::trie::{PathTrie, diff};
pub use exor_storage::{ScopeCoverage, WatchCoverage, WatchSummary};
use exor_storage::{SessionStatus, Store};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

type ProbeResult<T> = std::result::Result<Vec<T>, String>;

/// Source of system state that is not a file.
pub trait SystemProbe: Send + Sync {
    /// Service listings and available configuration metadata.
    fn services(&self) -> ProbeResult<ServiceInfo>;
    /// Scheduled-task metadata from readable definitions, or an error for an incomplete scope.
    fn tasks(&self) -> ProbeResult<TaskInfo>;
    /// Complete set of key metadata for one scope. A missing subtree is empty; an incomplete
    /// snapshot is an error, never a partial set.
    fn registry_keys(&self, _scope: &RegistryScope) -> ProbeResult<RegistryKeyState> {
        Err("registry snapshots are not available on this platform".into())
    }
}

/// A probe that reports nothing (non-Windows development).
pub struct NullProbe;

impl SystemProbe for NullProbe {
    fn services(&self) -> ProbeResult<ServiceInfo> {
        Err("service listing is not available on this platform".into())
    }
    fn tasks(&self) -> ProbeResult<TaskInfo> {
        Err("task listing is not available on this platform".into())
    }
}

/// Explicit, read-only scopes for both ends of a watch.
pub struct WatchPlan<'a> {
    /// Absolute folder paths. A missing folder can be watched for later creation.
    pub folders: &'a [PathBuf],
    /// Validated registry subtrees.
    pub registry: &'a RegistryScopes,
    /// Include service metadata.
    pub services: bool,
    /// Include scheduled-task metadata.
    pub tasks: bool,
}

/// An installation being watched.
pub struct InstallWatch {
    monitor: SessionMonitor,
    roots: Vec<(PathBuf, PathTrie)>,
    services: Option<Vec<ServiceInfo>>,
    tasks: Option<Vec<TaskInfo>>,
    registry: Vec<(RegistryScope, Vec<RegistryKeyState>)>,
    probe: Arc<dyn SystemProbe>,
    coverage: WatchCoverage,
    warnings: Vec<String>,
}

async fn probe_services(p: &Arc<dyn SystemProbe>) -> ProbeResult<ServiceInfo> {
    let p = Arc::clone(p);
    tokio::task::spawn_blocking(move || p.services())
        .await
        .map_err(|e| e.to_string())?
}

async fn probe_tasks(p: &Arc<dyn SystemProbe>) -> ProbeResult<TaskInfo> {
    let p = Arc::clone(p);
    tokio::task::spawn_blocking(move || p.tasks())
        .await
        .map_err(|e| e.to_string())?
}

async fn probe_registry(
    p: &Arc<dyn SystemProbe>,
    scope: &RegistryScope,
) -> ProbeResult<RegistryKeyState> {
    let p = Arc::clone(p);
    let scope = scope.clone();
    tokio::task::spawn_blocking(move || p.registry_keys(&scope))
        .await
        .map_err(|e| e.to_string())?
}

fn plain_folder(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        // FILE_ATTRIBUTE_REPARSE_POINT: do not interpret a redirected root as the watched folder.
        if meta.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    meta.is_dir() && !meta.is_symlink()
}

async fn folder_snapshot(
    root: &Path,
    previous: Option<&PathTrie>,
    cancel: &Cancel,
) -> Result<PathTrie> {
    cancel.check()?;
    match tokio::fs::symlink_metadata(root).await {
        Ok(meta) if plain_folder(&meta) => {}
        Ok(_) => {
            return Err(EngineError::Watch(
                "expected a folder, not a file or link".into(),
            ));
        }
        Err(e) if e.kind() == ErrorKind::NotFound => {
            // A genuinely absent folder can be created/removed between snapshots. An I/O error
            // during enumeration below is not equivalent to absence, even if it says NotFound.
            let mut empty = PathTrie::new();
            empty.finalize();
            return Ok(empty);
        }
        Err(e) => return Err(io_err(root)(e)),
    }
    let snapshot = capture(root, &CaptureOptions::default(), previous, cancel).await?;
    if let Some((path, reason)) = snapshot.skipped.first() {
        return Err(EngineError::Watch(format!(
            "{} path(s) could not be read; for example {path}: {reason}",
            snapshot.skipped.len()
        )));
    }
    Ok(snapshot.trie)
}

impl InstallWatch {
    /// Start a session and take the baseline. On failure the session is closed as failed.
    pub async fn begin(
        store: Arc<Mutex<Store>>,
        app_name: &str,
        installer_path: Option<&str>,
        plan: WatchPlan<'_>,
        probe: Arc<dyn SystemProbe>,
        cancel: &Cancel,
    ) -> Result<Self> {
        if plan.folders.is_empty()
            && plan.registry.as_slice().is_empty()
            && !plan.services
            && !plan.tasks
        {
            return Err(EngineError::Watch(
                "Choose at least one folder, registry subtree, or system check.".into(),
            ));
        }
        if plan.folders.iter().any(|root| !root.is_absolute()) {
            return Err(EngineError::Watch(
                "Enter a full path for every watched folder.".into(),
            ));
        }
        let coverage = WatchCoverage {
            folders: ScopeCoverage {
                requested: plan.folders.len(),
                usable: 0,
            },
            registry: ScopeCoverage {
                requested: plan.registry.as_slice().len(),
                usable: 0,
            },
            services: ScopeCoverage {
                requested: usize::from(plan.services),
                usable: 0,
            },
            tasks: ScopeCoverage {
                requested: usize::from(plan.tasks),
                usable: 0,
            },
        };
        let monitor = SessionMonitor::start(store, app_name, installer_path, None)?;
        let mut watch = Self {
            monitor,
            roots: Vec::new(),
            services: None,
            tasks: None,
            registry: Vec::new(),
            probe,
            coverage,
            warnings: Vec::new(),
        };
        if let Err(e) = watch.baseline(plan, cancel).await {
            let _ = watch.abandon();
            return Err(e);
        }
        Ok(watch)
    }

    async fn baseline(&mut self, plan: WatchPlan<'_>, cancel: &Cancel) -> Result<()> {
        cancel.check()?;
        for root in plan.folders {
            match folder_snapshot(root, None, cancel).await {
                Ok(snapshot) => self.roots.push((root.clone(), snapshot)),
                Err(EngineError::Cancelled) => return Err(EngineError::Cancelled),
                Err(e) => self.warnings.push(format!(
                    "Folder {} was not captured before changes: {e}",
                    root.display()
                )),
            }
        }
        if plan.services {
            cancel.check()?;
            match probe_services(&self.probe).await {
                Ok(v) => self.services = Some(v),
                Err(e) => self
                    .warnings
                    .push(format!("Services were not checked before changes: {e}")),
            }
        }
        if plan.tasks {
            cancel.check()?;
            match probe_tasks(&self.probe).await {
                Ok(v) => self.tasks = Some(v),
                Err(e) => self.warnings.push(format!(
                    "Scheduled tasks were not checked before changes: {e}"
                )),
            }
        }
        for scope in plan.registry.as_slice() {
            cancel.check()?;
            match probe_registry(&self.probe, scope).await {
                Ok(keys) => self.registry.push((scope.clone(), keys)),
                Err(e) => self.warnings.push(format!(
                    "Registry subtree {scope} was not checked before changes: {e}"
                )),
            }
        }
        cancel.check()?;
        self.coverage.folders.usable = self.roots.len();
        self.coverage.registry.usable = self.registry.len();
        self.coverage.services.usable = usize::from(self.services.is_some());
        self.coverage.tasks.usable = usize::from(self.tasks.is_some());
        if !self.coverage.has_usable() {
            let reason = self.warnings.first().map_or("", String::as_str);
            return Err(EngineError::Watch(format!(
                "No requested scopes could be captured. {reason}"
            )));
        }
        Ok(())
    }

    /// Session id.
    pub fn session_id(&self) -> i64 {
        self.monitor.session_id()
    }

    /// Requested scopes and usable baselines before the changes.
    pub fn coverage(&self) -> WatchCoverage {
        self.coverage
    }

    /// Baseline exclusions, available before the user runs an installer.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Give up without recording changes; the session is marked failed.
    pub fn abandon(self) -> Result<()> {
        self.monitor.finish(SessionStatus::Failed)
    }

    /// Re-capture the usable baselines, then commit the changes and coverage together.
    pub async fn finish(mut self, cancel: &Cancel) -> Result<WatchSummary> {
        let (summary, changes) = match self.compare(cancel).await {
            Ok(comparison) => comparison,
            Err(e) => {
                let _ = self.monitor.finish(SessionStatus::Failed);
                return Err(e);
            }
        };
        let monitor = self.monitor;
        let cancel = cancel.clone();
        tokio::task::spawn_blocking(move || {
            let result = monitor.record_watch(summary, &changes, &cancel);
            if result.is_err() {
                let _ = monitor.finish(SessionStatus::Failed);
            }
            result
        })
        .await?
    }

    async fn compare(&mut self, cancel: &Cancel) -> Result<(WatchSummary, WatchChanges)> {
        let mut summary = WatchSummary {
            session_id: self.session_id(),
            coverage: self.coverage,
            files_changed: 0,
            services_changed: 0,
            tasks_changed: 0,
            registry_keys_changed: 0,
            warnings: std::mem::take(&mut self.warnings),
        };
        summary.coverage.folders.usable = 0;
        summary.coverage.services.usable = 0;
        summary.coverage.tasks.usable = 0;
        summary.coverage.registry.usable = 0;
        let mut changes = WatchChanges::default();
        for (root, before) in std::mem::take(&mut self.roots) {
            match folder_snapshot(&root, Some(&before), cancel).await {
                Ok(after) => {
                    changes.files.extend(diff(&before, &after));
                    summary.coverage.folders.usable += 1;
                }
                Err(EngineError::Cancelled) => return Err(EngineError::Cancelled),
                Err(e) => summary.warnings.push(format!(
                    "Folder {} was not captured after changes: {e}",
                    root.display()
                )),
            }
        }
        if let Some(old) = self.services.take() {
            cancel.check()?;
            match probe_services(&self.probe).await {
                Ok(new) => {
                    changes.services = diff_services(&old, &new);
                    summary.coverage.services.usable = 1;
                }
                Err(e) => summary
                    .warnings
                    .push(format!("Services were not checked after changes: {e}")),
            }
        }
        if let Some(old) = self.tasks.take() {
            cancel.check()?;
            match probe_tasks(&self.probe).await {
                Ok(new) => {
                    changes.tasks = diff_tasks(&old, &new);
                    summary.coverage.tasks.usable = 1;
                }
                Err(e) => summary.warnings.push(format!(
                    "Scheduled tasks were not checked after changes: {e}"
                )),
            }
        }
        for (scope, before) in std::mem::take(&mut self.registry) {
            cancel.check()?;
            let after = probe_registry(&self.probe, &scope).await;
            cancel.check()?;
            match after {
                Ok(after) => {
                    changes.registry.extend(diff_registry_keys(&before, &after));
                    summary.coverage.registry.usable += 1;
                }
                Err(e) => summary.warnings.push(format!(
                    "Registry subtree {scope} was not checked after changes: {e}"
                )),
            }
        }
        cancel.check()?;
        Ok((summary, changes))
    }
}
