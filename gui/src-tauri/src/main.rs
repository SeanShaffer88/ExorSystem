//! ExorSystem desktop shell: thin Tauri commands over `exor-storage` and `exor-engine`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use exor_engine::snapshot::{CaptureOptions, Progress, capture_with_progress};
use exor_engine::{Cancel, EngineError, execute, policy, report, templates};
use exor_storage::{Edge, LedgerEntry, Session, Store};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::State;
use tauri::ipc::Channel;

struct AppState {
    store: Mutex<Store>,
    scan_cancel: Mutex<Option<Cancel>>,
    quarantine_root: PathBuf,
}

/// Events streamed to the UI over a Tauri v2 IPC channel while a scan runs.
#[derive(Clone, Serialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
enum ScanEvent {
    Progress(Progress),
    Done {
        files: usize,
        skipped: usize,
        root_digest: String,
    },
    Cancelled,
    Failed {
        message: String,
    },
}

fn lock<'a>(s: &'a State<'_, AppState>) -> Result<std::sync::MutexGuard<'a, Store>, String> {
    s.store
        .lock()
        .map_err(|_| "internal lock poisoned".to_string())
}

#[derive(Serialize)]
struct ProvenanceView {
    found: bool,
    summary: String,
    confidence: f64,
    origins: Vec<Edge>,
    ledger: Vec<LedgerEntry>,
}

#[derive(Serialize)]
struct SimulationView {
    summary: String,
    keep: usize,
    quarantine: usize,
    flag_for_review: usize,
    decisions: Vec<policy::Decision>,
}

#[tauri::command]
fn list_sessions(state: State<'_, AppState>) -> Result<Vec<Session>, String> {
    lock(&state)?.sessions().map_err(|e| e.to_string())
}

#[tauri::command]
fn provenance(state: State<'_, AppState>, path: String) -> Result<ProvenanceView, String> {
    let store = lock(&state)?;
    match store
        .provenance_for_path(&path)
        .map_err(|e| e.to_string())?
    {
        Some(p) => Ok(ProvenanceView {
            found: true,
            summary: report::describe_provenance(&p),
            confidence: p.confidence,
            origins: p.origins,
            ledger: p.ledger,
        }),
        None => Ok(ProvenanceView {
            found: false,
            summary: format!(
                "{path} is not in the system record. It may predate monitoring or have been filtered as background noise."
            ),
            confidence: 0.0,
            origins: Vec::new(),
            ledger: Vec::new(),
        }),
    }
}

#[tauri::command]
fn simulate(
    state: State<'_, AppState>,
    session_id: i64,
    template: String,
) -> Result<SimulationView, String> {
    let store = lock(&state)?;
    let rules =
        templates::rules_for(&template).ok_or_else(|| format!("unknown policy '{template}'"))?;
    let candidates =
        policy::candidates_for_session(&store, session_id).map_err(|e| e.to_string())?;
    let r = policy::simulate(&rules, &candidates).map_err(|e| e.to_string())?;
    Ok(SimulationView {
        summary: report::summarize_simulation(&r),
        keep: r.keep,
        quarantine: r.quarantine,
        flag_for_review: r.flag_for_review,
        decisions: r.decisions,
    })
}

#[derive(Serialize)]
struct QuarantineRow {
    quarantine_id: String,
    session_id: Option<i64>,
    status: String,
    files: usize,
}

#[derive(Serialize)]
struct QuarantineResult {
    quarantine_id: String,
    moved: usize,
    skipped: usize,
    summary: String,
}

/// Move the files a policy marks `quarantine` into a manifest-backed quarantine. Reversible.
#[tauri::command]
fn quarantine_session(
    state: State<'_, AppState>,
    session_id: i64,
    template: String,
) -> Result<QuarantineResult, String> {
    let store = lock(&state)?;
    let rules =
        templates::rules_for(&template).ok_or_else(|| format!("unknown policy '{template}'"))?;
    let candidates =
        policy::candidates_for_session(&store, session_id).map_err(|e| e.to_string())?;
    let report = policy::simulate(&rules, &candidates).map_err(|e| e.to_string())?;
    let out = execute::quarantine_decided(&store, &state.quarantine_root, session_id, &report)
        .map_err(|e| e.to_string())?;
    let moved = out.manifest.items.len();
    Ok(QuarantineResult {
        quarantine_id: out.manifest.quarantine_id,
        moved,
        skipped: out.skipped.len(),
        summary: format!(
            "Moved {moved} item(s) to quarantine. {} item(s) were not files and were left alone. You can restore them at any time.",
            out.skipped.len()
        ),
    })
}

#[tauri::command]
fn list_quarantines(state: State<'_, AppState>) -> Result<Vec<QuarantineRow>, String> {
    let store = lock(&state)?;
    let rows = store.quarantines().map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|r| {
            let files = serde_json::from_str::<serde_json::Value>(&r.manifest_json)
                .ok()
                .and_then(|v| v.get("items").and_then(|i| i.as_array().map(Vec::len)))
                .unwrap_or(0);
            let status = serde_json::to_value(r.status)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default();
            QuarantineRow {
                quarantine_id: r.quarantine_id,
                session_id: r.session_id,
                status,
                files,
            }
        })
        .collect())
}

/// Put quarantined files back. Never overwrites a file that has reappeared.
#[tauri::command]
fn restore_quarantine(state: State<'_, AppState>, quarantine_id: String) -> Result<usize, String> {
    let store = lock(&state)?;
    execute::restore_by_id(&store, &quarantine_id, false)
        .map(|m| m.items.len())
        .map_err(|e| e.to_string())
}

/// Permanently delete quarantined files. The UI asks for a second confirmation first.
#[tauri::command]
fn purge_quarantine(state: State<'_, AppState>, quarantine_id: String) -> Result<usize, String> {
    let store = lock(&state)?;
    execute::purge_by_id(&store, &quarantine_id)
        .map(|m| m.items.len())
        .map_err(|e| e.to_string())
}

/// Snapshot a folder without blocking the UI: progress streams over `on_event`.
#[tauri::command]
async fn scan(
    state: State<'_, AppState>,
    root: String,
    on_event: Channel<ScanEvent>,
) -> Result<(), String> {
    let cancel = Cancel::new();
    {
        let mut slot = state
            .scan_cancel
            .lock()
            .map_err(|_| "internal lock poisoned".to_string())?;
        if slot.is_some() {
            return Err("a scan is already running".into());
        }
        *slot = Some(cancel.clone());
    }
    let progress = on_event.clone();
    let result = capture_with_progress(
        std::path::Path::new(&root),
        &CaptureOptions::default(),
        None,
        &cancel,
        &move |p| {
            let _ = progress.send(ScanEvent::Progress(p));
        },
    )
    .await;
    if let Ok(mut slot) = state.scan_cancel.lock() {
        *slot = None;
    }
    let event = match result {
        Ok(snap) => ScanEvent::Done {
            files: snap.trie.len(),
            skipped: snap.skipped.len(),
            root_digest: snap.trie.root_digest(),
        },
        Err(EngineError::Cancelled) => ScanEvent::Cancelled,
        Err(e) => ScanEvent::Failed {
            message: e.to_string(),
        },
    };
    on_event.send(event).map_err(|e| e.to_string())
}

/// Ask the running scan (if any) to stop.
#[tauri::command]
fn cancel_scan(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(c) = state
        .scan_cancel
        .lock()
        .map_err(|_| "internal lock poisoned".to_string())?
        .as_ref()
    {
        c.cancel();
    }
    Ok(())
}

/// Portable by default: data lives next to the executable (`data/exor.db`).
fn data_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("data")))
        .unwrap_or_else(|| PathBuf::from("data"))
}

fn run() -> Result<(), String> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let store = Store::open(&dir.join("exor.db")).map_err(|e| e.to_string())?;
    let quarantine_root = dir.join("quarantine");
    // A crash mid-quarantine must never strand files: undo any interrupted run before the UI opens.
    match execute::recover_interrupted(&quarantine_root) {
        Ok(done) if !done.is_empty() => eprintln!(
            "ExorSystem: rolled back {} interrupted quarantine(s)",
            done.len()
        ),
        Ok(_) => {}
        Err(e) => eprintln!("ExorSystem: quarantine recovery failed: {e}"),
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .manage(AppState {
            store: Mutex::new(store),
            scan_cancel: Mutex::new(None),
            quarantine_root,
        })
        .invoke_handler(tauri::generate_handler![
            list_sessions,
            provenance,
            simulate,
            scan,
            cancel_scan,
            quarantine_session,
            list_quarantines,
            restore_quarantine,
            purge_quarantine
        ])
        .run(tauri::generate_context!())
        .map_err(|e| e.to_string())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("ExorSystem failed to start: {e}");
        std::process::exit(1);
    }
}
