//! ExorSystem desktop shell: thin Tauri commands over `exor-storage` and `exor-engine`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use exor_engine::leftovers::{self, Leftover};
use exor_engine::programs::{self, InstalledProgram};
use exor_engine::snapshot::{CaptureOptions, Progress, capture_with_progress};
use exor_engine::watch::{InstallWatch, SystemProbe, WatchSummary};
use exor_engine::{Cancel, EngineError, execute, policy, report, templates};
use exor_storage::{Edge, LedgerEntry, Session, Store};
use serde::Serialize;
mod dock;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::ipc::Channel;
use tauri::{Manager, State, WindowEvent};

struct AppState {
    store: Arc<Mutex<Store>>,
    watch: Mutex<Option<InstallWatch>>,
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
    app: tauri::AppHandle,
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
    dock::log(
        &app,
        "info",
        format!("Quarantined {moved} item(s) from installation #{session_id}."),
    );
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
fn restore_quarantine(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    quarantine_id: String,
) -> Result<usize, String> {
    let store = lock(&state)?;
    let r = execute::restore_by_id(&store, &state.quarantine_root, &quarantine_id, false)
        .map(|m| m.items.len())
        .map_err(|e| e.to_string());
    match &r {
        Ok(n) => dock::log(
            &app,
            "info",
            format!("Restored {n} item(s) from quarantine."),
        ),
        Err(e) => dock::log(&app, "error", format!("Restore failed: {e}")),
    }
    r
}

/// Permanently delete quarantined files. The UI asks for a second confirmation first.
#[tauri::command]
fn purge_quarantine(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    quarantine_id: String,
) -> Result<usize, String> {
    let store = lock(&state)?;
    let r = execute::purge_by_id(&store, &state.quarantine_root, &quarantine_id)
        .map(|m| m.items.len())
        .map_err(|e| e.to_string());
    match &r {
        Ok(n) => dock::log(
            &app,
            "warn",
            format!("Permanently deleted {n} quarantined item(s)."),
        ),
        Err(e) => dock::log(&app, "error", format!("Delete failed: {e}")),
    }
    r
}

#[derive(Serialize)]
struct ProgramRow {
    program: InstalledProgram,
    /// Recorded installation sessions that belong to this program.
    session_ids: Vec<i64>,
}

/// Programs Windows lists as installed, tidied, each linked to any recorded sessions.
/// Empty off Windows (the list comes from the registry).
#[tauri::command]
async fn list_programs(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    include_system: bool,
) -> Result<Vec<ProgramRow>, String> {
    #[cfg(windows)]
    let raw = tauri::async_runtime::spawn_blocking(exor_win::enumerate_programs)
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    let raw: Vec<InstalledProgram> = Vec::new();
    let sessions = lock(&state)?.sessions().map_err(|e| e.to_string())?;
    let listed = programs::visible(raw, include_system);
    dock::log(
        &app,
        "info",
        format!("Listed {} installed program(s).", listed.len()),
    );
    Ok(listed
        .into_iter()
        .map(|program| {
            let session_ids = programs::match_sessions(&program, &sessions)
                .iter()
                .map(|s| s.session_id)
                .collect();
            ProgramRow {
                program,
                session_ids,
            }
        })
        .collect())
}

/// Look a program up by its registry id. Windows only: the list comes from the registry.
async fn program_by_id(id: String) -> Result<InstalledProgram, String> {
    #[cfg(windows)]
    {
        let raw = tauri::async_runtime::spawn_blocking(exor_win::enumerate_programs)
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        raw.into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| "that program is no longer in Windows' list".to_string())
    }
    #[cfg(not(windows))]
    {
        let _ = id;
        Err("uninstalling needs Windows".to_string())
    }
}

/// Folders where programs usually leave files.
fn leftover_roots() -> Vec<PathBuf> {
    [
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramData",
        "LOCALAPPDATA",
        "APPDATA",
    ]
    .iter()
    .filter_map(std::env::var_os)
    .map(PathBuf::from)
    .collect()
}

#[derive(Serialize)]
struct UninstallRun {
    /// Process exit code; `None` when Windows gave none.
    exit_code: Option<i32>,
    summary: String,
}

/// Start the program's own uninstaller and wait for it to finish. No shell is used. The command
/// comes from Windows' registry entry for the program, never from the UI.
#[tauri::command]
async fn run_uninstaller(
    app: tauri::AppHandle,
    program_id: String,
    quiet: bool,
) -> Result<UninstallRun, String> {
    let p = program_by_id(program_id).await?;
    if !programs::elevated_uninstall_allowed(p.scope) {
        return Err(
            "For safety, ExorSystem will not run a per-user uninstaller with administrator rights. Uninstall it from Windows Settings."
                .into(),
        );
    }
    let cmd = if quiet {
        p.quiet_uninstall_command
            .as_ref()
            .or(p.uninstall_command.as_ref())
    } else {
        p.uninstall_command.as_ref()
    }
    .ok_or_else(|| "this program has no uninstaller registered".to_string())?
    .clone();
    let (exe, args) = leftovers::split_command(&cmd)
        .ok_or_else(|| "the registered uninstall command could not be read".to_string())?;
    dock::log(
        &app,
        "info",
        format!("Running the uninstaller for {}…", p.name),
    );
    let status = tauri::async_runtime::spawn_blocking(move || {
        std::process::Command::new(exe).args(args).status()
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| {
        dock::log(
            &app,
            "error",
            format!("Could not start the uninstaller: {e}"),
        );
        format!("could not start the uninstaller: {e}")
    })?;
    let code = status.code();
    let summary = if status.success() {
        format!(
            "The uninstaller for {} finished. Next, look for leftovers.",
            p.name
        )
    } else {
        format!(
            "The uninstaller for {} ended with code {}. It may have been cancelled. You can still look for leftovers.",
            p.name,
            code.map_or("unknown".into(), |c| c.to_string())
        )
    };
    dock::log(
        &app,
        if status.success() { "info" } else { "warn" },
        summary.clone(),
    );
    Ok(UninstallRun {
        exit_code: code,
        summary,
    })
}

/// Folders that may be left after uninstalling the program. Nothing is changed.
#[tauri::command]
async fn find_leftovers(
    app: tauri::AppHandle,
    program_id: String,
) -> Result<Vec<Leftover>, String> {
    let p = program_by_id(program_id).await?;
    let roots = leftover_roots();
    let found = tauri::async_runtime::spawn_blocking(move || leftovers::find_leftovers(&p, &roots))
        .await
        .map_err(|e| e.to_string())?;
    dock::log(
        &app,
        "info",
        format!("Found {} possible leftover folder(s).", found.len()),
    );
    Ok(found)
}

/// Quarantine the chosen leftover folders. Only folders `find_leftovers` suggests are accepted.
#[tauri::command]
async fn quarantine_leftovers(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    program_id: String,
    folders: Vec<String>,
    session_id: Option<i64>,
) -> Result<QuarantineResult, String> {
    let p = program_by_id(program_id).await?;
    let roots = leftover_roots();
    let allowed =
        tauri::async_runtime::spawn_blocking(move || leftovers::find_leftovers(&p, &roots))
            .await
            .map_err(|e| e.to_string())?;
    let chosen: Vec<PathBuf> = folders
        .iter()
        .filter(|f| allowed.iter().any(|l| l.path.eq_ignore_ascii_case(f)))
        .map(PathBuf::from)
        .collect();
    if chosen.is_empty() {
        return Err("none of those folders is a suggested leftover".into());
    }
    let store = lock(&state)?;
    let (id, moved) =
        leftovers::quarantine_leftovers(&store, &state.quarantine_root, session_id, &chosen)
            .map_err(|e| e.to_string())?;
    dock::log(
        &app,
        "info",
        format!("Quarantined {moved} leftover file(s)."),
    );
    Ok(QuarantineResult {
        quarantine_id: id,
        moved,
        skipped: 0,
        summary: format!("Moved {moved} file(s) to quarantine. You can restore them at any time."),
    })
}

/// Where saved reports go: the user's Documents folder, else their home folder.
fn reports_dir() -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let docs = home.join("Documents");
    if docs.is_dir() { docs } else { home }
}

/// A file name that is safe on Windows.
fn safe_name(s: &str) -> String {
    let n: String = s
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " -_.()".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    let n = n.trim().trim_matches('.').to_string();
    if n.is_empty() {
        "installation".into()
    } else {
        n.chars().take(80).collect()
    }
}

/// Save a plain-language report for one recorded installation. Never overwrites a file.
#[tauri::command]
fn export_report(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: i64,
    template: String,
) -> Result<String, String> {
    let store = lock(&state)?;
    let name = store
        .session(session_id)
        .map_err(|e| e.to_string())?
        .map(|s| s.app_name)
        .unwrap_or_default();
    let text = report::session_report(&store, session_id, &template).map_err(|e| e.to_string())?;
    let dir = reports_dir();
    let base = format!("ExorSystem report - {}", safe_name(&name));
    let mut path = dir.join(format!("{base}.md"));
    let mut n = 2;
    while path.exists() {
        path = dir.join(format!("{base} ({n}).md"));
        n += 1;
    }
    std::fs::write(&path, text).map_err(|e| format!("could not save the report: {e}"))?;
    let shown = path.to_string_lossy().into_owned();
    dock::log(&app, "info", format!("Saved report to {shown}."));
    Ok(shown)
}

#[derive(Serialize)]
struct PortableView {
    folder: String,
    copied: usize,
    missing: usize,
    registry_keys: usize,
    unsupported: usize,
    summary: String,
}

/// Copy a recorded installation's files into a new portable folder in Documents.
#[tauri::command]
fn make_portable(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: i64,
) -> Result<PortableView, String> {
    let store = lock(&state)?;
    let name = store
        .session(session_id)
        .map_err(|e| e.to_string())?
        .map(|s| s.app_name)
        .unwrap_or_default();
    let nodes = store
        .nodes_for_session(session_id)
        .map_err(|e| e.to_string())?;
    let plan = exor_engine::portable::plan_portable(&nodes);
    let base = format!("ExorSystem portable - {}", safe_name(&name));
    let dir = reports_dir();
    let mut dest = dir.join(&base);
    let mut n = 2;
    while dest.exists() {
        dest = dir.join(format!("{base} ({n})"));
        n += 1;
    }
    let r = exor_engine::portable::build_portable(&plan, &dest).map_err(|e| e.to_string())?;
    let folder = dest.to_string_lossy().into_owned();
    let summary = format!(
        "Copied {} file(s) to {folder}. {} listed file(s) were missing. {} registry key(s) and {} item(s) that cannot be portable are listed in portable-plan.json. Your installed program was not changed.",
        r.copied,
        r.missing.len(),
        plan.export_registry.len(),
        plan.unsupported.len()
    );
    dock::log(&app, "info", summary.clone());
    Ok(PortableView {
        folder,
        copied: r.copied,
        missing: r.missing.len(),
        registry_keys: plan.export_registry.len(),
        unsupported: plan.unsupported.len(),
        summary,
    })
}

#[derive(Serialize)]
struct RiskRow {
    name: String,
    why: String,
}

#[derive(Serialize)]
struct RiskView {
    low: Vec<RiskRow>,
    medium: Vec<RiskRow>,
    high: Vec<RiskRow>,
}

/// How risky changing each recorded program would be, in batches (low first).
#[tauri::command]
fn update_risk(state: State<'_, AppState>) -> Result<RiskView, String> {
    let store = lock(&state)?;
    let b = exor_engine::risk::batch_by_risk(
        exor_engine::risk::candidates_from_store(&store).map_err(|e| e.to_string())?,
    );
    let rows = |v: Vec<exor_engine::risk::UpdateCandidate>| {
        v.into_iter()
            .map(|c| RiskRow {
                why: exor_engine::risk::explain(&c),
                name: c.name,
            })
            .collect()
    };
    Ok(RiskView {
        low: rows(b.low),
        medium: rows(b.medium),
        high: rows(b.high),
    })
}

#[derive(Serialize)]
struct TemplateInfo {
    name: String,
    description: String,
}

/// Cleanup policies the user can choose between.
#[tauri::command]
fn list_templates() -> Vec<TemplateInfo> {
    templates::templates()
        .into_iter()
        .map(|t| TemplateInfo {
            name: t.name.to_string(),
            description: t.description.to_string(),
        })
        .collect()
}

#[derive(Serialize)]
struct AppInfo {
    version: String,
    data_folder: String,
    quarantine_folder: String,
}

/// Version and where ExorSystem keeps its files.
#[tauri::command]
fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        data_folder: data_dir().to_string_lossy().into_owned(),
        quarantine_folder: state.quarantine_root.to_string_lossy().into_owned(),
    }
}

#[derive(Serialize)]
struct HistoryRow {
    id: i64,
    when: Option<String>,
    path: String,
    what: String,
    session_id: Option<i64>,
}

/// Newest recorded changes, optionally limited to paths containing `filter`.
#[tauri::command]
fn history(
    state: State<'_, AppState>,
    filter: Option<String>,
    limit: usize,
) -> Result<Vec<HistoryRow>, String> {
    let store = lock(&state)?;
    let f = filter.as_deref().map(str::trim).filter(|f| !f.is_empty());
    let rows = store
        .ledger_recent(limit.clamp(1, 1000), f)
        .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|e| HistoryRow {
            id: e.entry_id,
            when: e.observed_at,
            path: e.entry.path,
            what: e.entry.change_kind.as_str().to_string(),
            session_id: e.entry.session_id,
        })
        .collect())
}

#[derive(Serialize)]
struct SessionItem {
    path: String,
    node_type: String,
    confidence: f64,
    shared: bool,
}

#[derive(Serialize)]
struct SessionItems {
    total: usize,
    shared: usize,
    by_type: std::collections::BTreeMap<String, usize>,
    /// First 500 items, least certain last.
    items: Vec<SessionItem>,
}

/// Everything one recorded installation left behind, with how sure we are about each item.
#[tauri::command]
fn session_items(state: State<'_, AppState>, session_id: i64) -> Result<SessionItems, String> {
    let store = lock(&state)?;
    let cands = policy::candidates_for_session(&store, session_id).map_err(|e| e.to_string())?;
    let mut by_type = std::collections::BTreeMap::new();
    let mut shared = 0;
    let mut items: Vec<SessionItem> = cands
        .iter()
        .map(|c| {
            let node_type = serde_json::to_value(c.node_type)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default();
            *by_type.entry(node_type.clone()).or_insert(0) += 1;
            let is_shared = c
                .attrs
                .get("shared")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if is_shared {
                shared += 1;
            }
            SessionItem {
                path: c.path.clone(),
                node_type,
                confidence: c
                    .attrs
                    .get("confidence")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.3),
                shared: is_shared,
            }
        })
        .collect();
    items.sort_by(|a, b| {
        b.confidence
            .total_cmp(&a.confidence)
            .then_with(|| a.path.cmp(&b.path))
    });
    let total = items.len();
    items.truncate(500);
    Ok(SessionItems {
        total,
        shared,
        by_type,
        items,
    })
}

/// Real service and task listings on Windows; elsewhere the engine's "not available" probe.
#[cfg(windows)]
struct SystemState;

#[cfg(windows)]
impl SystemProbe for SystemState {
    fn services(&self) -> Result<Vec<exor_engine::collectors::ServiceInfo>, String> {
        exor_win::enumerate_services().map_err(|e| e.to_string())
    }
    fn tasks(&self) -> Result<Vec<exor_engine::collectors::TaskInfo>, String> {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        exor_engine::collectors::read_tasks_dir(&PathBuf::from(root).join("System32").join("Tasks"))
            .map_err(|e| e.to_string())
    }
}

fn probe() -> Arc<dyn SystemProbe> {
    #[cfg(windows)]
    {
        Arc::new(SystemState)
    }
    #[cfg(not(windows))]
    {
        Arc::new(exor_engine::watch::NullProbe)
    }
}

fn watch_slot<'a>(
    s: &'a State<'_, AppState>,
) -> Result<std::sync::MutexGuard<'a, Option<InstallWatch>>, String> {
    s.watch
        .lock()
        .map_err(|_| "internal lock poisoned".to_string())
}

/// Take the "before" picture of the watched folders, services and tasks. Run the installer next.
#[tauri::command]
async fn begin_watch(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    app_name: String,
    installer_path: Option<String>,
    folders: Vec<String>,
) -> Result<i64, String> {
    if watch_slot(&state)?.is_some() {
        return Err("an installation is already being watched".into());
    }
    let name = app_name.trim();
    if name.is_empty() {
        return Err("enter the application name".into());
    }
    let roots: Vec<PathBuf> = folders
        .iter()
        .map(|f| f.trim())
        .filter(|f| !f.is_empty())
        .map(PathBuf::from)
        .collect();
    let installer = installer_path
        .as_deref()
        .map(str::trim)
        .filter(|p| !p.is_empty());
    dock::log(
        &app,
        "info",
        format!("Taking the 'before' picture for {name}…"),
    );
    let w = InstallWatch::begin(
        Arc::clone(&state.store),
        name,
        installer,
        &roots,
        probe(),
        &Cancel::new(),
    )
    .await
    .map_err(|e| e.to_string())?;
    let id = w.session_id();
    dock::log(
        &app,
        "info",
        format!("Watching {name}. Run the installer, then finish."),
    );
    let mut slot = watch_slot(&state)?;
    if slot.is_some() {
        let _ = w.abandon();
        return Err("an installation is already being watched".into());
    }
    *slot = Some(w);
    Ok(id)
}

/// Take the "after" picture and record every difference in one session.
#[tauri::command]
async fn finish_watch(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<WatchSummary, String> {
    let w = watch_slot(&state)?
        .take()
        .ok_or_else(|| "no installation is being watched".to_string())?;
    dock::log(&app, "info", "Taking the 'after' picture and comparing…");
    let r = w.finish(&Cancel::new()).await.map_err(|e| e.to_string());
    match &r {
        Ok(_) => dock::log(&app, "info", "Installation recorded."),
        Err(e) => dock::log(&app, "error", format!("Recording failed: {e}")),
    }
    r
}

/// Stop watching without recording anything.
#[tauri::command]
fn cancel_watch(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if let Some(w) = watch_slot(&state)?.take() {
        w.abandon().map_err(|e| e.to_string())?;
        dock::log(&app, "info", "Stopped watching. Nothing was recorded.");
    }
    Ok(())
}

/// Snapshot a folder without blocking the UI: progress streams over `on_event`.
#[tauri::command]
async fn scan(
    app: tauri::AppHandle,
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
    dock::log(&app, "info", format!("Scanning {root}…"));
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
    match &result {
        Ok(snap) => dock::log(
            &app,
            "info",
            format!("Scan finished: {} file(s).", snap.trie.len()),
        ),
        Err(EngineError::Cancelled) => dock::log(&app, "warn", "Scan cancelled."),
        Err(e) => dock::log(&app, "error", format!("Scan failed: {e}")),
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

fn quarantine_dir(data_dir: &std::path::Path) -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        let system_data = exor_win::program_data_dir().map_err(|e| e.to_string())?;
        let canonical_data = data_dir
            .canonicalize()
            .map_err(|e| format!("cannot resolve {}: {e}", data_dir.display()))?;
        let identity = exor_engine::hashing::hash_bytes(
            canonical_data.to_string_lossy().to_lowercase().as_bytes(),
        );
        Ok(system_data
            .join("ExorSystem")
            .join(identity)
            .join("quarantine"))
    }
    #[cfg(not(windows))]
    {
        Ok(data_dir.join("quarantine"))
    }
}

fn run() -> Result<(), String> {
    let dir = data_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let store = Store::open(&dir.join("exor.db")).map_err(|e| e.to_string())?;
    let quarantine_root = quarantine_dir(&dir)?;
    std::fs::create_dir_all(&quarantine_root)
        .map_err(|e| format!("cannot create protected quarantine storage: {e}"))?;
    #[cfg(windows)]
    exor_win::secure_quarantine_dir(&quarantine_root).map_err(|e| e.to_string())?;
    let quarantine_root = quarantine_root
        .canonicalize()
        .map_err(|e| format!("cannot resolve protected quarantine storage: {e}"))?;
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
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_denylist(&["log"])
                .build(),
        )
        .manage(dock::Dock::default())
        .manage(AppState {
            store: Arc::new(Mutex::new(store)),
            watch: Mutex::new(None),
            scan_cancel: Mutex::new(None),
            quarantine_root,
        })
        .setup(|app| {
            let handle = app.handle().clone();
            if let Some(main) = app.get_webview_window("main") {
                main.on_window_event(move |e| match e {
                    WindowEvent::Moved(_) | WindowEvent::Resized(_) => dock::follow(&handle),
                    // the hidden log window must not keep the app alive
                    WindowEvent::CloseRequested { .. } => handle.exit(0),
                    _ => {}
                });
            }
            dock::log(app.handle(), "info", "ExorSystem started.");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            dock::get_log,
            dock::clear_log,
            dock::set_log_open,
            dock::log_is_open,
            list_sessions,
            provenance,
            simulate,
            scan,
            cancel_scan,
            quarantine_session,
            list_quarantines,
            restore_quarantine,
            purge_quarantine,
            list_programs,
            history,
            export_report,
            list_templates,
            app_info,
            update_risk,
            make_portable,
            run_uninstaller,
            find_leftovers,
            quarantine_leftovers,
            session_items,
            begin_watch,
            finish_watch,
            cancel_watch
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
