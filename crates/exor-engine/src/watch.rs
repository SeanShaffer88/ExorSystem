//! Install watch: baseline before an installer runs, diff after, and record the evidence.
//!
//! Files and selected registry, service, and scheduled-task metadata are observed independently.
//! Unreadable scopes on either side are skipped, not mistaken for empty snapshots.

use crate::cancel::Cancel;
use crate::collectors::{ServiceInfo, TaskInfo, diff_services, diff_tasks};
use crate::error::{EngineError, Result};
use crate::monitor::{SessionMonitor, WatchChanges};
use crate::registry::{RegistryKeyState, RegistryScope, RegistryScopes, diff_registry_keys};
use crate::snapshot::{CaptureOptions, Progress, capture_with_progress};
use crate::trie::{PathTrie, diff};
pub use exor_storage::{ScopeCoverage, WatchCoverage, WatchSummary};
use exor_storage::{SessionStatus, Store};
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
    /// Capture service metadata with cancellation between records where supported.
    fn services_cancellable(&self, _stop: &Cancel) -> ProbeResult<ServiceInfo> {
        self.services()
    }
    /// Capture task definitions with cancellation between records where supported.
    fn tasks_cancellable(&self, _stop: &Cancel) -> ProbeResult<TaskInfo> {
        self.tasks()
    }
    /// Capture one registry scope with cancellation between records where supported.
    fn registry_cancellable(
        &self,
        scope: &RegistryScope,
        _stop: &Cancel,
    ) -> ProbeResult<RegistryKeyState> {
        self.registry_keys(scope)
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
    /// Hash every file again and preserve case-only sibling paths.
    pub strict: bool,
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
    options: CaptureOptions,
}

/// A capture phase and (for file scopes) throttled real I/O counts.
#[derive(Clone, serde::Serialize)]
pub struct WatchProgress {
    /// Human-readable work stage.
    pub phase: &'static str,
    /// Current folder or registry scope, when applicable.
    pub scope: Option<String>,
    /// File progress scoped to the current folder.
    pub files: Option<Progress>,
}

type Notify = Arc<dyn Fn(WatchProgress) + Send + Sync>;

fn notify(callback: &Notify, phase: &'static str, scope: Option<String>) {
    callback(WatchProgress {
        phase,
        scope,
        files: None,
    });
}

const MAX_ROOTS: usize = 8;

async fn probe_services(p: &Arc<dyn SystemProbe>, stop: &Cancel) -> ProbeResult<ServiceInfo> {
    let (p, stop) = (Arc::clone(p), stop.clone());
    tokio::task::spawn_blocking(move || p.services_cancellable(&stop))
        .await
        .map_err(|e| e.to_string())?
}

async fn probe_tasks(p: &Arc<dyn SystemProbe>, stop: &Cancel) -> ProbeResult<TaskInfo> {
    let (p, stop) = (Arc::clone(p), stop.clone());
    tokio::task::spawn_blocking(move || p.tasks_cancellable(&stop))
        .await
        .map_err(|e| e.to_string())?
}

async fn probe_registry(
    p: &Arc<dyn SystemProbe>,
    scope: &RegistryScope,
    stop: &Cancel,
) -> ProbeResult<RegistryKeyState> {
    let (p, stop) = (Arc::clone(p), stop.clone());
    let scope = scope.clone();
    tokio::task::spawn_blocking(move || p.registry_cancellable(&scope, &stop))
        .await
        .map_err(|e| e.to_string())?
}

async fn folder_snapshot(
    root: &Path,
    previous: Option<&PathTrie>,
    opts: &CaptureOptions,
    cancel: &Cancel,
    progress: Notify,
) -> Result<PathTrie> {
    cancel.check()?;
    let root_to_check = root.to_path_buf();
    if !tokio::task::spawn_blocking(move || crate::quarantine::plain_dirs(&root_to_check)).await?? {
        let mut empty = if opts.strict {
            PathTrie::new_strict()
        } else {
            PathTrie::new()
        };
        empty.finalize();
        return Ok(empty);
    }
    let scope = root.display().to_string();
    let snapshot = capture_with_progress(
        root,
        opts,
        previous,
        cancel,
        Arc::new(move |files| {
            progress(WatchProgress {
                phase: "Capturing files",
                scope: Some(scope.clone()),
                files: Some(files),
            });
        }),
    )
    .await?;
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
        Self::begin_with_progress(
            store,
            app_name,
            installer_path,
            plan,
            probe,
            cancel,
            Arc::new(|_| {}),
        )
        .await
    }

    /// Start a baseline and report which scopes are being captured.
    pub async fn begin_with_progress(
        store: Arc<Mutex<Store>>,
        app_name: &str,
        installer_path: Option<&str>,
        plan: WatchPlan<'_>,
        probe: Arc<dyn SystemProbe>,
        cancel: &Cancel,
        progress: Notify,
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
        if plan.folders.len() > MAX_ROOTS {
            return Err(EngineError::Watch(format!(
                "Choose at most {MAX_ROOTS} folders."
            )));
        }
        if plan
            .folders
            .iter()
            .any(|root| crate::quarantine::local_path(root).is_err())
        {
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
        let app = app_name.to_owned();
        let installer = installer_path.map(str::to_owned);
        let monitor = tokio::task::spawn_blocking(move || {
            SessionMonitor::start(store, &app, installer.as_deref(), None)
        })
        .await??;
        let n = plan.folders.len().max(1);
        let options = CaptureOptions {
            strict: plan.strict,
            total_hash_bytes: (4 * 1024 * 1024 * 1024) / n as u64,
            walk: crate::walker::WalkOptions {
                max_entries: 24_000.min(48_000 / n),
                max_path_bytes: (8 * 1024 * 1024).min(16 * 1024 * 1024 / n),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut watch = Self {
            monitor,
            roots: Vec::new(),
            services: None,
            tasks: None,
            registry: Vec::new(),
            probe,
            coverage,
            warnings: Vec::new(),
            options,
        };
        if let Err(e) = watch.baseline(plan, cancel, progress).await {
            let _ = tokio::task::spawn_blocking(move || watch.abandon()).await;
            return Err(e);
        }
        Ok(watch)
    }

    async fn baseline(
        &mut self,
        plan: WatchPlan<'_>,
        cancel: &Cancel,
        progress: Notify,
    ) -> Result<()> {
        cancel.check()?;
        for root in plan.folders {
            match folder_snapshot(root, None, &self.options, cancel, progress.clone()).await {
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
            notify(&progress, "Reading service configuration", None);
            match probe_services(&self.probe, cancel).await {
                Ok(v) => self.services = Some(v),
                Err(e) => self
                    .warnings
                    .push(format!("Services were not checked before changes: {e}")),
            }
        }
        if plan.tasks {
            cancel.check()?;
            notify(&progress, "Reading task definitions", None);
            match probe_tasks(&self.probe, cancel).await {
                Ok(v) => self.tasks = Some(v),
                Err(e) => self.warnings.push(format!(
                    "Scheduled tasks were not checked before changes: {e}"
                )),
            }
        }
        for scope in plan.registry.as_slice() {
            cancel.check()?;
            notify(
                &progress,
                "Reading registry metadata",
                Some(scope.to_string()),
            );
            match probe_registry(&self.probe, scope, cancel).await {
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
    pub async fn finish(self, cancel: &Cancel) -> Result<WatchSummary> {
        self.finish_with_progress(cancel, Arc::new(|_| {})).await
    }

    /// Compare usable scopes and report capture and commit phases.
    pub async fn finish_with_progress(
        mut self,
        cancel: &Cancel,
        progress: Notify,
    ) -> Result<WatchSummary> {
        let (summary, changes) = match self.compare(cancel, &progress).await {
            Ok(comparison) => comparison,
            Err(e) => {
                let _ =
                    tokio::task::spawn_blocking(move || self.monitor.finish(SessionStatus::Failed))
                        .await;
                return Err(e);
            }
        };
        notify(&progress, "Recording the comparison", None);
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

    async fn compare(
        &mut self,
        cancel: &Cancel,
        progress: &Notify,
    ) -> Result<(WatchSummary, WatchChanges)> {
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
            match folder_snapshot(
                &root,
                Some(&before),
                &self.options,
                cancel,
                progress.clone(),
            )
            .await
            {
                Ok(after) => {
                    notify(
                        progress,
                        "Comparing file snapshots",
                        Some(root.display().to_string()),
                    );
                    changes
                        .files
                        .extend(tokio::task::spawn_blocking(move || diff(&before, &after)).await?);
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
            notify(progress, "Reading service configuration", None);
            match probe_services(&self.probe, cancel).await {
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
            notify(progress, "Reading task definitions", None);
            match probe_tasks(&self.probe, cancel).await {
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
            notify(
                progress,
                "Reading registry metadata",
                Some(scope.to_string()),
            );
            let after = probe_registry(&self.probe, &scope, cancel).await;
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
