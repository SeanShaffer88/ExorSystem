//! Bounded desktop jobs. No unbounded blocking-pool queue and no store mutex across I/O.
//! One expensive/mutating job at a time; up to four independent WAL readers.

use crate::{AppState, dock};
use exor_engine::Cancel;
use exor_storage::Store;
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Default)]
struct Slots {
    work: Option<(&'static str, Option<Cancel>)>,
    readers: usize,
    closing: bool,
}

#[derive(Default)]
pub(crate) struct Gate(Arc<Mutex<Slots>>);

pub(crate) struct Job {
    slots: Arc<Mutex<Slots>>,
    app: AppHandle,
    write: bool,
}

impl Gate {
    pub(crate) fn work(
        &self,
        app: &AppHandle,
        label: &'static str,
        cancel: Option<Cancel>,
    ) -> Result<Job, String> {
        self.acquire(app, Some((label, cancel)))
    }

    fn acquire(
        &self,
        app: &AppHandle,
        work: Option<(&'static str, Option<Cancel>)>,
    ) -> Result<Job, String> {
        let mut slots = self.0.lock().map_err(|_| "desktop job lock unavailable")?;
        if slots.closing {
            return Err("ExorSystem is closing.".into());
        }
        let write = work.is_some();
        if write {
            if let Some((name, _)) = &slots.work {
                return Err(format!(
                    "{name} is already running. Try again when it finishes."
                ));
            }
            slots.work = work;
        } else {
            if slots.readers >= 4 {
                return Err("Evidence readers are busy. Try again in a moment.".into());
            }
            slots.readers += 1;
        }
        Ok(Job {
            slots: Arc::clone(&self.0),
            app: app.clone(),
            write,
        })
    }

    pub(crate) fn close(&self, app: &AppHandle) {
        let Ok(mut slots) = self.0.lock() else { return };
        if slots.closing {
            return;
        }
        slots.closing = true;
        if let Some((_, Some(token))) = &slots.work {
            token.cancel();
        }
        let ready = slots.work.is_none() && slots.readers == 0;
        let wait_for = slots
            .work
            .as_ref()
            .map_or("evidence queries", |(name, _)| name);
        drop(slots);
        if ready {
            finish_close(app);
        } else {
            let message = format!(
                "Closing after {wait_for} reaches a safe stopping point. Vendor processes are not terminated."
            );
            let _ = app.emit("shutdown-wait", &message);
            dock::log(app, "info", message);
        }
    }

    pub(crate) fn stop_preflight(&self) -> bool {
        self.0.lock().ok().is_some_and(|slots| {
            if let Some(("Preparing cleanup", Some(token))) = &slots.work {
                token.cancel();
                true
            } else {
                false
            }
        })
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        let Ok(mut slots) = self.slots.lock() else {
            return;
        };
        if self.write {
            slots.work = None;
        } else {
            slots.readers -= 1;
        }
        let exit = slots.closing && slots.work.is_none() && slots.readers == 0;
        drop(slots);
        if exit {
            finish_close(&self.app);
        }
    }
}

fn finish_close(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // A completed command can release its gate just before its async Watch slot drops.
        // Wait for that slot instead of silently leaving a completed baseline behind.
        let watch = app.state::<AppState>().watch.lock().await.take();
        if let Some(watch) = watch {
            let _ = tauri::async_runtime::spawn_blocking(move || watch.abandon()).await;
        }
        app.exit(0);
    });
}

pub(crate) async fn read<T: Send + 'static>(
    app: &AppHandle,
    state: &AppState,
    f: impl FnOnce(&Store) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let job = state.jobs.acquire(app, None)?;
    let db = state.data_dir.join("exor.db");
    tauri::async_runtime::spawn_blocking(move || {
        let _job = job;
        f(&Store::open_reader(&db).map_err(|e| e.to_string())?)
    })
    .await
    .map_err(|e| e.to_string())?
}

pub(crate) async fn run<T: Send + 'static>(
    job: Job,
    f: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _job = job;
        f()
    })
    .await
    .map_err(|e| e.to_string())?
}
