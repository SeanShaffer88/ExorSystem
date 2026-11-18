//! The owned Activity log window attaches below the main window without taking focus. The
//! in-memory log is shared with the main window's status bar.

use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager};

const MAX_LINES: usize = 1000;

/// One line of the activity log.
#[derive(Clone, Serialize)]
pub struct LogLine {
    /// Session-local sequence, so multiple views can merge a snapshot with streamed events.
    pub seq: u64,
    /// Milliseconds since the Unix epoch.
    pub t: u64,
    /// `info`, `warn` or `error`.
    pub level: &'static str,
    /// Plain-language message.
    pub msg: String,
}

#[derive(Serialize)]
pub struct LogSnapshot {
    cleared_through: u64,
    lines: Vec<LogLine>,
}

#[derive(Default)]
struct LogBuffer {
    lines: VecDeque<LogLine>,
    last_sequence: u64,
    cleared_through: u64,
}

/// Scrollback plus native visibility and work-area reservation state.
#[derive(Default)]
pub struct Dock {
    buffer: Mutex<LogBuffer>,
    open: AtomicBool,
    reserved: AtomicBool,
    follow_pending: AtomicBool,
}

/// Record a line and tell every window.
pub fn log(app: &AppHandle, level: &'static str, msg: impl Into<String>) {
    let dock = app.state::<Dock>();
    let Ok(mut buffer) = dock.buffer.lock() else {
        return;
    };
    buffer.last_sequence += 1;
    let line = LogLine {
        seq: buffer.last_sequence,
        t: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
        level,
        msg: msg.into(),
    };
    if buffer.lines.len() >= MAX_LINES {
        buffer.lines.pop_front();
    }
    buffer.lines.push_back(line.clone());
    drop(buffer);
    let _ = app.emit("log", line);
}

// Called only on the GUI thread. The owned window follows minimize/restore automatically, but we
// still reconcile on restore (including a close requested while the main window was minimized).
fn reconcile(app: &AppHandle, open: bool) -> Result<(), String> {
    let main = app
        .get_webview_window("main")
        .ok_or("main window not found")?;
    let log = app
        .get_webview_window("log")
        .ok_or("activity log window not found")?;
    let result = |e: tauri::Error| e.to_string();
    #[cfg(windows)]
    let log_hwnd = log.hwnd().map_err(result)?;
    if !open {
        #[cfg(windows)]
        exor_win::show_activity_window(log_hwnd, false)?;
        #[cfg(not(windows))]
        log.hide().map_err(result)?;
    }
    if main.is_minimized().map_err(result)? {
        return Ok(());
    }
    #[cfg(windows)]
    {
        let dock = app.state::<Dock>();
        let mut reserved = dock.reserved.load(Ordering::SeqCst);
        if open || reserved {
            let monitor = main
                .current_monitor()
                .map_err(result)?
                .ok_or("monitor not found")?;
            let area = monitor.work_area();
            let placement = exor_win::place_activity_window(
                main.hwnd().map_err(result)?,
                log_hwnd,
                (
                    area.position.x,
                    area.position.y,
                    area.size.width,
                    area.size.height,
                ),
                monitor.scale_factor(),
                open,
                &mut reserved,
            );
            dock.reserved.store(reserved, Ordering::SeqCst);
            placement?;
        }
    }
    if open {
        #[cfg(windows)]
        exor_win::show_activity_window(log_hwnd, true)?;
        #[cfg(not(windows))]
        log.show().map_err(result)?;
    }
    Ok(())
}

fn set_open(app: &AppHandle, open: bool) -> Result<(), String> {
    let dock = app.state::<Dock>();
    // Initial placement can itself send DPI/resize notifications. Let them queue a follow pass.
    dock.open.store(open, Ordering::SeqCst);
    let result = reconcile(app, open);
    if open && result.is_err() {
        // If the owner was resized but the attachment failed, give the space back.
        let _ = reconcile(app, false);
    }
    let open = open && result.is_ok();
    dock.open.store(open, Ordering::SeqCst);
    let _ = app.emit("log-open", open);
    result
}

/// Batch move, resize, restore and DPI notifications. Do not change bounds recursively from a
/// window callback: SetWindowPos can synchronously deliver further native messages.
pub fn follow(app: &AppHandle) {
    let dock = app.state::<Dock>();
    if !(dock.open.load(Ordering::SeqCst) || dock.reserved.load(Ordering::SeqCst))
        || dock.follow_pending.swap(true, Ordering::SeqCst)
    {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let handle = app.clone();
        if app
            .run_on_main_thread(move || {
                let dock = handle.state::<Dock>();
                dock.follow_pending.store(false, Ordering::SeqCst);
                let open = dock.open.load(Ordering::SeqCst);
                if let Err(e) = reconcile(&handle, open)
                    && open
                {
                    let _ = set_open(&handle, false);
                    log(
                        &handle,
                        "error",
                        format!("Could not keep the activity log attached: {e}"),
                    );
                }
            })
            .is_err()
        {
            app.state::<Dock>()
                .follow_pending
                .store(false, Ordering::SeqCst);
        }
    });
}

/// Native close (for example Alt+F4) hides the owned window without destroying its scrollback.
pub fn close(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(e) = set_log_open(app.clone(), Some(false)).await {
            log(
                &app,
                "error",
                format!("Could not close the activity log: {e}"),
            );
        }
    });
}

#[tauri::command]
pub fn get_log(dock: tauri::State<'_, Dock>) -> Result<LogSnapshot, String> {
    let buffer = dock
        .buffer
        .lock()
        .map_err(|_| "could not read the activity log")?;
    Ok(LogSnapshot {
        cleared_through: buffer.cleared_through,
        lines: buffer.lines.iter().cloned().collect(),
    })
}

#[tauri::command]
pub fn clear_log(app: AppHandle, dock: tauri::State<'_, Dock>) -> Result<u64, String> {
    let cleared_through = {
        let mut buffer = dock
            .buffer
            .lock()
            .map_err(|_| "could not clear the activity log")?;
        buffer.lines.clear();
        buffer.cleared_through = buffer.last_sequence;
        buffer.cleared_through
    };
    // An event before this sequence can still be in flight to a WebView.
    let _ = app.emit("log-cleared", cleared_through);
    Ok(cleared_through)
}

#[tauri::command]
pub async fn set_log_open(app: AppHandle, open: Option<bool>) -> Result<(), String> {
    let (send, mut receive) = tauri::async_runtime::channel(1);
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let open = open.unwrap_or_else(|| !handle.state::<Dock>().open.load(Ordering::SeqCst));
        let _ = send.try_send(set_open(&handle, open));
    })
    .map_err(|e| e.to_string())?;
    receive
        .recv()
        .await
        .ok_or_else(|| "window update interrupted".to_string())?
}

#[tauri::command]
pub fn log_is_open(dock: tauri::State<'_, Dock>) -> bool {
    dock.open.load(Ordering::SeqCst)
}
