//! The activity-log window: a small fixed-size borderless window docked to the right edge of the
//! main window. It rests hidden *behind* the main window and slides out sideways when opened.
//!
//! Also owns the in-memory activity log (a ring buffer) that feeds it.

use serde::Serialize;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WebviewWindow};

const MAX_LINES: usize = 1000;
/// Logical size of the log window.
const WIDTH: f64 = 420.0;
const HEIGHT: f64 = 380.0;
const STEPS: u32 = 14;
const STEP_MS: u64 = 12;

/// One line of the activity log.
#[derive(Clone, Serialize)]
pub struct LogLine {
    /// Milliseconds since the Unix epoch.
    pub t: u64,
    /// `info`, `warn` or `error`.
    pub level: &'static str,
    /// Plain-language message.
    pub msg: String,
}

/// Log buffer plus slide state. Managed by Tauri.
#[derive(Default)]
pub struct Dock {
    lines: Mutex<VecDeque<LogLine>>,
    /// Where the window is heading: `true` = out, `false` = tucked behind.
    open: AtomicBool,
    /// Bumped by every slide so an older animation stops when a newer one starts.
    generation: AtomicU64,
    /// Current slide position, 0.0 (hidden behind) to 1.0 (fully out).
    frac: Mutex<f64>,
}

/// Record a line and tell every window.
pub fn log(app: &AppHandle, level: &'static str, msg: impl Into<String>) {
    let line = LogLine {
        t: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0),
        level,
        msg: msg.into(),
    };
    if let Ok(mut q) = app.state::<Dock>().lines.lock() {
        if q.len() >= MAX_LINES {
            q.pop_front();
        }
        q.push_back(line.clone());
    }
    let _ = app.emit("log", line);
}

fn windows(app: &AppHandle) -> Option<(WebviewWindow, WebviewWindow)> {
    Some((
        app.get_webview_window("main")?,
        app.get_webview_window("log")?,
    ))
}

/// Fixed size, scaled for the screen and never taller than the main window.
fn fit(main: &WebviewWindow, log: &WebviewWindow) -> Option<PhysicalSize<u32>> {
    let scale = main.scale_factor().ok()?;
    let main_h = main.outer_size().ok()?.height as f64;
    let h = (HEIGHT * scale).min(main_h * 0.9).max(120.0 * scale);
    let size = PhysicalSize::new((WIDTH * scale).round() as u32, h.round() as u32);
    log.set_size(size).ok()?;
    Some(size)
}

/// Put the log window at slide position `f`, bottom-aligned with the main window. At `f = 0` it
/// sits entirely inside the main window's footprint, so the main window covers it.
fn place(main: &WebviewWindow, log: &WebviewWindow, f: f64) -> Option<()> {
    let mp = main.outer_position().ok()?;
    let ms = main.outer_size().ok()?;
    let ls = log.outer_size().ok()?;
    let right = mp.x + ms.width as i32;
    let x = right - ls.width as i32 + (f * ls.width as f64).round() as i32;
    let y = mp.y + ms.height as i32 - ls.height as i32;
    log.set_position(PhysicalPosition::new(x, y)).ok()
}

fn ease(t: f64) -> f64 {
    1.0 - (1.0 - t).powi(3)
}

/// Slide the log window out (`true`) or back behind the main window (`false`).
pub fn set_open(app: &AppHandle, open: bool, animate: bool) {
    let dock = app.state::<Dock>();
    if dock.open.swap(open, Ordering::SeqCst) == open {
        return;
    }
    let generation = dock.generation.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    std::thread::spawn(move || {
        let Some((main, log)) = windows(&app) else {
            return;
        };
        let dock = app.state::<Dock>();
        if open {
            if fit(&main, &log).is_none() {
                return;
            }
            if !log.is_visible().unwrap_or(false) {
                *dock.frac.lock().unwrap_or_else(|e| e.into_inner()) = 0.0;
                place(&main, &log, 0.0);
                let _ = log.show();
                // showing activates the window; hand focus straight back so it stays underneath
                let _ = main.set_focus();
            }
        }
        let from = *dock.frac.lock().unwrap_or_else(|e| e.into_inner());
        let to = if open { 1.0 } else { 0.0 };
        let steps = if animate { STEPS } else { 1 };
        for i in 1..=steps {
            if dock.generation.load(Ordering::SeqCst) != generation {
                return;
            }
            let f = from + (to - from) * ease(f64::from(i) / f64::from(steps));
            place(&main, &log, f);
            *dock.frac.lock().unwrap_or_else(|e| e.into_inner()) = f;
            if i < steps {
                std::thread::sleep(Duration::from_millis(STEP_MS));
            }
        }
        if !open && dock.generation.load(Ordering::SeqCst) == generation {
            let _ = log.hide();
        }
    });
}

/// Keep the log window glued to the main window when it moves, resizes or minimizes.
pub fn follow(app: &AppHandle) {
    let dock = app.state::<Dock>();
    if !dock.open.load(Ordering::SeqCst) {
        return;
    }
    let Some((main, log)) = windows(app) else {
        return;
    };
    if main.is_minimized().unwrap_or(false) {
        let _ = log.hide();
        return;
    }
    // a slide in progress places the window itself on its next step
    let f = *dock.frac.lock().unwrap_or_else(|e| e.into_inner());
    if fit(&main, &log).is_some() {
        place(&main, &log, f);
    }
    if !log.is_visible().unwrap_or(true) {
        let _ = log.show();
        let _ = main.set_focus();
    }
}

#[tauri::command]
pub fn get_log(dock: tauri::State<'_, Dock>) -> Vec<LogLine> {
    dock.lines
        .lock()
        .map(|q| q.iter().cloned().collect())
        .unwrap_or_default()
}

#[tauri::command]
pub fn clear_log(dock: tauri::State<'_, Dock>) {
    if let Ok(mut q) = dock.lines.lock() {
        q.clear();
    }
}

#[tauri::command]
pub fn set_log_open(app: AppHandle, open: bool, animate: bool) {
    set_open(&app, open, animate);
    let _ = app.emit("log-open", open);
}

#[tauri::command]
pub fn log_is_open(dock: tauri::State<'_, Dock>) -> bool {
    dock.open.load(Ordering::SeqCst)
}
