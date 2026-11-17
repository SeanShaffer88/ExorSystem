//! Install watch: baseline before an installer runs, diff after, attribute everything to one session.
//!
//! Files come from folder snapshots; services and scheduled tasks come from a [`SystemProbe`]
//! (the real one lives in the Windows shell, tests use a mock). If a probe fails on either side the
//! service/task diff is skipped with a warning, because comparing against nothing would report
//! every item as new or removed.

use crate::cancel::Cancel;
use crate::collectors::{ServiceInfo, TaskInfo, diff_services, diff_tasks};
use crate::error::Result;
use crate::monitor::SessionMonitor;
use crate::snapshot::{CaptureOptions, capture};
use crate::trie::{PathTrie, diff};
use exor_storage::{SessionStatus, Store};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// Source of system state that is not a file.
pub trait SystemProbe: Send + Sync {
    /// All services and driver services.
    fn services(&self) -> std::result::Result<Vec<ServiceInfo>, String>;
    /// All scheduled tasks.
    fn tasks(&self) -> std::result::Result<Vec<TaskInfo>, String>;
}

/// A probe that reports nothing (non-Windows development).
pub struct NullProbe;

impl SystemProbe for NullProbe {
    fn services(&self) -> std::result::Result<Vec<ServiceInfo>, String> {
        Err("service listing is not available on this platform".into())
    }
    fn tasks(&self) -> std::result::Result<Vec<TaskInfo>, String> {
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
    /// Things that could not be checked.
    pub warnings: Vec<String>,
    /// Plain-language result.
    pub summary: String,
}

/// An installation being watched.
pub struct InstallWatch {
    monitor: SessionMonitor,
    app_name: String,
    roots: Vec<(PathBuf, PathTrie)>,
    services: Option<Vec<ServiceInfo>>,
    tasks: Option<Vec<TaskInfo>>,
    probe: Arc<dyn SystemProbe>,
    warnings: Vec<String>,
}

async fn probe_services(p: &Arc<dyn SystemProbe>) -> std::result::Result<Vec<ServiceInfo>, String> {
    let p = Arc::clone(p);
    tokio::task::spawn_blocking(move || p.services())
        .await
        .map_err(|e| e.to_string())?
}

async fn probe_tasks(p: &Arc<dyn SystemProbe>) -> std::result::Result<Vec<TaskInfo>, String> {
    let p = Arc::clone(p);
    tokio::task::spawn_blocking(move || p.tasks())
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
        probe: Arc<dyn SystemProbe>,
        cancel: &Cancel,
    ) -> Result<Self> {
        let monitor = SessionMonitor::start(store, app_name, installer_path, None)?;
        let mut tries = Vec::new();
        for r in roots {
            match capture(r, &CaptureOptions::default(), None, cancel).await {
                Ok(s) => tries.push((r.clone(), s.trie)),
                Err(e) => {
                    let _ = monitor.finish(SessionStatus::Failed);
                    return Err(e);
                }
            }
        }
        let mut warnings = Vec::new();
        let services = match probe_services(&probe).await {
            Ok(v) => Some(v),
            Err(e) => {
                warnings.push(format!("Services were not checked before the install: {e}"));
                None
            }
        };
        let tasks = match probe_tasks(&probe).await {
            Ok(v) => Some(v),
            Err(e) => {
                warnings.push(format!(
                    "Scheduled tasks were not checked before the install: {e}"
                ));
                None
            }
        };
        Ok(Self {
            monitor,
            app_name: app_name.to_string(),
            roots: tries,
            services,
            tasks,
            probe,
            warnings,
        })
    }

    /// Session id.
    pub fn session_id(&self) -> i64 {
        self.monitor.session_id()
    }

    /// Give up without recording changes; the session is marked failed.
    pub fn abandon(self) -> Result<()> {
        self.monitor.finish(SessionStatus::Failed)
    }

    /// Re-capture everything, record the differences, and close the session.
    pub async fn finish(self, cancel: &Cancel) -> Result<WatchSummary> {
        let Self {
            monitor,
            app_name,
            roots,
            services,
            tasks,
            probe,
            mut warnings,
        } = self;
        match Self::record(
            &monitor,
            &roots,
            services,
            tasks,
            &probe,
            &mut warnings,
            cancel,
        )
        .await
        {
            Ok((files, svc, tsk)) => {
                monitor.finish(SessionStatus::Completed)?;
                let summary = format!(
                    "Recorded {files} file change(s), {svc} service change(s) and {tsk} scheduled-task change(s) for {app_name}."
                );
                Ok(WatchSummary {
                    session_id: monitor.session_id(),
                    files_changed: files,
                    services_changed: svc,
                    tasks_changed: tsk,
                    warnings,
                    summary,
                })
            }
            Err(e) => {
                let _ = monitor.finish(SessionStatus::Failed);
                Err(e)
            }
        }
    }

    async fn record(
        monitor: &SessionMonitor,
        roots: &[(PathBuf, PathTrie)],
        services: Option<Vec<ServiceInfo>>,
        tasks: Option<Vec<TaskInfo>>,
        probe: &Arc<dyn SystemProbe>,
        warnings: &mut Vec<String>,
        cancel: &Cancel,
    ) -> Result<(usize, usize, usize)> {
        let mut files = 0;
        for (root, before) in roots {
            let after = capture(root, &CaptureOptions::default(), Some(before), cancel).await?;
            files += monitor.attribute_diff(&diff(before, &after.trie))?;
        }
        let mut svc = 0;
        if let Some(old) = services {
            match probe_services(probe).await {
                Ok(new) => svc = monitor.attribute_services(&diff_services(&old, &new))?,
                Err(e) => {
                    warnings.push(format!("Services were not checked after the install: {e}"))
                }
            }
        }
        let mut tsk = 0;
        if let Some(old) = tasks {
            match probe_tasks(probe).await {
                Ok(new) => tsk = monitor.attribute_tasks(&diff_tasks(&old, &new))?,
                Err(e) => warnings.push(format!(
                    "Scheduled tasks were not checked after the install: {e}"
                )),
            }
        }
        Ok((files, svc, tsk))
    }
}
