//! Install watch: baseline before an installer runs, diff after, and record the evidence.
//!
//! Files come from folder snapshots. Services, scheduled tasks, and explicitly chosen registry
//! subtrees come from a [`SystemProbe`]. A failed probe on either side skips that comparison;
//! comparing a failed snapshot against an empty one would make up additions or removals.

use crate::cancel::Cancel;
use crate::collectors::{ServiceInfo, TaskInfo, diff_services, diff_tasks};
use crate::error::Result;
use crate::monitor::SessionMonitor;
use crate::registry::{RegistryKeyState, RegistryScope, RegistryScopes, diff_registry_keys};
use crate::snapshot::{CaptureOptions, capture};
use crate::trie::{PathTrie, diff};
use exor_storage::{SessionStatus, Store};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

type ProbeResult<T> = std::result::Result<Vec<T>, String>;

/// Source of system state that is not a file.
pub trait SystemProbe: Send + Sync {
    /// All services and driver services.
    fn services(&self) -> ProbeResult<ServiceInfo>;
    /// All scheduled tasks.
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

/// Result of [`InstallWatch::finish`].
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WatchSummary {
    /// Session that now holds the evidence.
    pub session_id: i64,
    /// Files added, changed or removed in the watched folders.
    pub files_changed: usize,
    /// Service changes recorded.
    pub services_changed: usize,
    /// Scheduled-task changes recorded.
    pub tasks_changed: usize,
    /// Registry key changes recorded.
    pub registry_keys_changed: usize,
    /// Registry subtrees successfully captured both times, including empty subtrees.
    pub registry_scopes_compared: usize,
    /// Things that could not be checked.
    pub warnings: Vec<String>,
    /// Plain-language result for files, services and tasks.
    pub summary: String,
}

/// An installation being watched.
pub struct InstallWatch {
    monitor: SessionMonitor,
    app_name: String,
    roots: Vec<(PathBuf, PathTrie)>,
    services: Option<Vec<ServiceInfo>>,
    tasks: Option<Vec<TaskInfo>>,
    registry: Vec<(RegistryScope, Vec<RegistryKeyState>)>,
    probe: Arc<dyn SystemProbe>,
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

impl InstallWatch {
    /// Start a session and take the baseline. On failure the session is closed as failed.
    pub async fn begin(
        store: Arc<Mutex<Store>>,
        app_name: &str,
        installer_path: Option<&str>,
        roots: &[PathBuf],
        registry_scopes: &RegistryScopes,
        probe: Arc<dyn SystemProbe>,
        cancel: &Cancel,
    ) -> Result<Self> {
        let monitor = SessionMonitor::start(store, app_name, installer_path, None)?;
        let mut watch = Self {
            monitor,
            app_name: app_name.to_string(),
            roots: Vec::new(),
            services: None,
            tasks: None,
            registry: Vec::new(),
            probe,
            warnings: Vec::new(),
        };
        if let Err(e) = watch.baseline(roots, registry_scopes, cancel).await {
            let _ = watch.abandon();
            return Err(e);
        }
        Ok(watch)
    }

    async fn baseline(
        &mut self,
        roots: &[PathBuf],
        scopes: &RegistryScopes,
        cancel: &Cancel,
    ) -> Result<()> {
        cancel.check()?;
        for root in roots {
            let snapshot = capture(root, &CaptureOptions::default(), None, cancel).await?;
            self.roots.push((root.clone(), snapshot.trie));
        }
        match probe_services(&self.probe).await {
            Ok(v) => self.services = Some(v),
            Err(e) => self
                .warnings
                .push(format!("Services were not checked before the install: {e}")),
        }
        match probe_tasks(&self.probe).await {
            Ok(v) => self.tasks = Some(v),
            Err(e) => self.warnings.push(format!(
                "Scheduled tasks were not checked before the install: {e}"
            )),
        }
        for scope in scopes.as_slice() {
            cancel.check()?;
            match probe_registry(&self.probe, scope).await {
                Ok(keys) => self.registry.push((scope.clone(), keys)),
                Err(e) => self.warnings.push(format!(
                    "Registry subtree {scope} was not checked before the install: {e}"
                )),
            }
        }
        cancel.check()
    }

    /// Session id.
    pub fn session_id(&self) -> i64 {
        self.monitor.session_id()
    }

    /// Baseline exclusions, available before the user runs an installer.
    pub fn warnings(&self) -> &[String] {
        &self.warnings
    }

    /// Give up without recording changes; the session is marked failed.
    pub fn abandon(self) -> Result<()> {
        self.monitor.finish(SessionStatus::Failed)
    }

    /// Re-capture everything, record the differences, and close the session.
    pub async fn finish(mut self, cancel: &Cancel) -> Result<WatchSummary> {
        match self.record(cancel).await {
            Ok(summary) => {
                self.monitor.finish(SessionStatus::Completed)?;
                Ok(summary)
            }
            Err(e) => {
                let _ = self.monitor.finish(SessionStatus::Failed);
                Err(e)
            }
        }
    }

    async fn record(&mut self, cancel: &Cancel) -> Result<WatchSummary> {
        cancel.check()?;
        let mut files = 0;
        for (root, before) in &self.roots {
            let after = capture(root, &CaptureOptions::default(), Some(before), cancel).await?;
            files += self.monitor.attribute_diff(&diff(before, &after.trie))?;
        }
        let mut svc = 0;
        if let Some(old) = self.services.take() {
            match probe_services(&self.probe).await {
                Ok(new) => {
                    svc = self
                        .monitor
                        .attribute_services(&diff_services(&old, &new))?
                }
                Err(e) => self
                    .warnings
                    .push(format!("Services were not checked after the install: {e}")),
            }
        }
        let mut tsk = 0;
        if let Some(old) = self.tasks.take() {
            match probe_tasks(&self.probe).await {
                Ok(new) => tsk = self.monitor.attribute_tasks(&diff_tasks(&old, &new))?,
                Err(e) => self.warnings.push(format!(
                    "Scheduled tasks were not checked after the install: {e}"
                )),
            }
        }
        let mut registry_keys_changed = 0;
        let mut registry_scopes_compared = 0;
        for (scope, before) in std::mem::take(&mut self.registry) {
            cancel.check()?;
            let after = probe_registry(&self.probe, &scope).await;
            cancel.check()?;
            match after {
                Ok(after) => {
                    registry_keys_changed += self
                        .monitor
                        .attribute_registry_keys(&diff_registry_keys(&before, &after))?;
                    registry_scopes_compared += 1;
                }
                Err(e) => self.warnings.push(format!(
                    "Registry subtree {scope} was not checked after the install: {e}"
                )),
            }
        }
        let app = &self.app_name;
        Ok(WatchSummary {
            session_id: self.session_id(),
            files_changed: files,
            services_changed: svc,
            tasks_changed: tsk,
            registry_keys_changed,
            registry_scopes_compared,
            warnings: std::mem::take(&mut self.warnings),
            summary: format!(
                "Recorded {files} file change(s), {svc} service change(s) and {tsk} scheduled-task change(s) for {app}."
            ),
        })
    }
}
