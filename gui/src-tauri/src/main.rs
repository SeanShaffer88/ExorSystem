//! ExorSystem desktop shell: thin Tauri commands over `exor-storage` and `exor-engine`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use exor_engine::leftovers::{self, Leftover, ReviewLeftover};
use exor_engine::programs::InstalledProgram;
use exor_engine::registry::RegistryScopes;
use exor_engine::snapshot::{CaptureOptions, Progress, capture_with_progress};
use exor_engine::watch::{InstallWatch, SystemProbe, WatchCoverage, WatchPlan, WatchSummary};
use exor_engine::{Cancel, EngineError, execute, policy, report, templates};
use exor_storage::{
    Edge, LedgerEntry, Node, NodeType, PolicyAction, RelationshipType, Session, Store,
};
use serde::{Deserialize, Serialize};
mod archive;
mod dock;
mod inventory;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::async_runtime::Mutex as AsyncMutex;
use tauri::ipc::Channel;
use tauri::{Manager, State, WindowEvent};

struct AppState {
    store: Arc<Mutex<Store>>,
    watch: AsyncMutex<Option<InstallWatch>>,
    watch_capture: Mutex<Option<(u64, Cancel)>>,
    scan_cancel: Mutex<Option<Cancel>>,
    recent_uninstalls: Mutex<BTreeMap<String, InstalledProgram>>,
    data_dir: PathBuf,
    quarantine_root: PathBuf,
    legacy_data: Option<PathBuf>,
    recovery_warnings: Vec<String>,
    _profile_lock: std::fs::File,
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
    graph: Option<RelationshipGraph>,
    ledger: Vec<LedgerEntry>,
}

#[derive(Serialize)]
struct GraphNode {
    id: i64,
    path: String,
    kind: NodeType,
    session_name: Option<String>,
}

#[derive(Serialize)]
struct GraphLink {
    edge: Edge,
    node: GraphNode,
}

#[derive(Serialize)]
struct RelationshipGraph {
    focus: GraphNode,
    links: Vec<GraphLink>,
    total_edges: usize,
}

fn graph_node(
    store: &Store,
    node: Node,
    names: &mut BTreeMap<i64, Option<String>>,
) -> exor_storage::Result<GraphNode> {
    let session_name = if let Some(id) = node.first_detected_in {
        match names.entry(id) {
            std::collections::btree_map::Entry::Occupied(e) => e.get().clone(),
            std::collections::btree_map::Entry::Vacant(e) => {
                e.insert(store.session(id)?.map(|s| s.app_name)).clone()
            }
        }
    } else {
        None
    };
    Ok(GraphNode {
        id: node.node_id,
        path: node.path,
        kind: node.node_type,
        session_name,
    })
}

fn relationship_graph(
    store: &Store,
    node: Node,
    mut edges: Vec<Edge>,
) -> exor_storage::Result<RelationshipGraph> {
    // Prefer dependency/share evidence to attribution when an artifact has many edges.
    edges.sort_by_key(|e| {
        (
            !matches!(
                e.relationship_type,
                RelationshipType::DependsOn | RelationshipType::SharedWith
            ),
            e.edge_id,
        )
    });
    edges.dedup_by_key(|e| e.edge_id);
    let total_edges = edges.len();
    let mut names = BTreeMap::new();
    let focus = graph_node(store, node, &mut names)?;
    let mut links = Vec::new();
    for edge in edges.into_iter().take(60) {
        let other = if edge.child_node_id == focus.id {
            edge.parent_node_id
        } else {
            edge.child_node_id
        };
        if let Some(node) = store.node(other)? {
            links.push(GraphLink {
                edge,
                node: graph_node(store, node, &mut names)?,
            });
        }
    }
    Ok(RelationshipGraph {
        focus,
        links,
        total_edges,
    })
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
            graph: Some(
                relationship_graph(
                    &store,
                    p.node,
                    p.origins.into_iter().chain(p.dependents).collect(),
                )
                .map_err(|e| e.to_string())?,
            ),
            ledger: p.ledger,
        }),
        None => Ok(ProvenanceView {
            found: false,
            summary: format!(
                "{path} is not in the system record. It may predate monitoring or have been filtered as background noise."
            ),
            confidence: 0.0,
            graph: None,
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
    /// Logical lengths recorded when the files were moved, not allocated disk space.
    file_bytes: Option<u64>,
}

#[derive(Serialize)]
struct QuarantineResult {
    quarantine_id: String,
    moved: usize,
    skipped: usize,
    summary: String,
}

fn inside_folder(path: &Path, folder: &Path) -> bool {
    let (Ok(path), Ok(folder)) = (path.canonicalize(), folder.canonicalize()) else {
        return false;
    };
    path.ancestors().any(|ancestor| {
        #[cfg(windows)]
        {
            ancestor
                .as_os_str()
                .eq_ignore_ascii_case(folder.as_os_str())
        }
        #[cfg(not(windows))]
        {
            ancestor == folder
        }
    })
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
    if report.decisions.iter().any(|d| {
        d.action == PolicyAction::Quarantine && inside_folder(Path::new(&d.path), &state.data_dir)
    }) {
        return Err("An ExorSystem profile file was selected. Nothing was moved.".into());
    }
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

fn quarantine_sizes(json: &str) -> (usize, Option<u64>) {
    use exor_engine::quarantine::{QuarantineItem, QuarantineManifest};
    let Ok(manifest) = serde_json::from_str::<QuarantineManifest>(json) else {
        return (0, None);
    };
    manifest
        .items
        .into_iter()
        .fold((0, Some(0_u64)), |(files, bytes), item| match item {
            QuarantineItem::File { size, .. } => {
                (files + 1, bytes.and_then(|b| b.checked_add(size)))
            }
            QuarantineItem::Registry { .. } => (files, bytes),
        })
}

#[tauri::command]
async fn list_quarantines(state: State<'_, AppState>) -> Result<Vec<QuarantineRow>, String> {
    let store = Arc::clone(&state.store);
    tauri::async_runtime::spawn_blocking(move || {
        let rows = store
            .lock()
            .map_err(|_| "internal lock poisoned".to_string())?
            .quarantines()
            .map_err(|e| e.to_string())?;
        Ok(rows
            .into_iter()
            .map(|r| {
                let (files, file_bytes) = quarantine_sizes(&r.manifest_json);
                QuarantineRow {
                    quarantine_id: r.quarantine_id,
                    session_id: r.session_id,
                    status: r.status.as_str().into(),
                    files,
                    file_bytes,
                }
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Put quarantined files back. Never overwrites a file that has reappeared.
#[tauri::command]
fn restore_quarantine(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    quarantine_id: String,
) -> Result<usize, String> {
    let store = lock(&state)?;
    let r = execute::restore_by_id(&store, &state.quarantine_root, &quarantine_id)
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

/// Prefer Windows' current data. If its entry vanished after our uninstaller run, retain just
/// enough metadata to review leftovers during this app session; this is not a command source.
async fn review_program(state: &AppState, program_id: String) -> Result<InstalledProgram, String> {
    if let Some(p) = inventory::registry_program(program_id.clone()).await? {
        return Ok(p);
    }
    state
        .recent_uninstalls
        .lock()
        .map_err(|_| "internal lock poisoned".to_string())?
        .get(&program_id)
        .cloned()
        .ok_or_else(|| "no install information is available for this program".to_string())
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

#[derive(Deserialize, Serialize, PartialEq, Eq)]
struct UninstallPreview {
    executable: String,
    args: Vec<String>,
}

fn uninstall_path(exe: &str) -> Result<PathBuf, String> {
    #[cfg(windows)]
    {
        exor_win::uninstall_executable(exe)
    }
    #[cfg(not(windows))]
    {
        let _ = exe;
        Err("Windows uninstall commands are not supported on this platform".into())
    }
}

fn start_uninstaller(exe: &Path, args: &[String]) -> Result<std::process::ExitStatus, String> {
    let dir = exe
        .parent()
        .ok_or("the uninstall command has no executable directory")?;
    std::process::Command::new(exe)
        .args(args)
        .current_dir(dir)
        .status()
        .map_err(|e| {
            if e.raw_os_error() == Some(740) {
                "This uninstaller needs administrator rights. Use Windows Settings → Apps instead."
                    .into()
            } else {
                e.to_string()
            }
        })
}

async fn live_uninstall(
    program_id: String,
    quiet: bool,
) -> Result<(InstalledProgram, PathBuf, UninstallPreview), String> {
    let program = inventory::registry_program(program_id)
        .await?
        .ok_or_else(|| "that program is no longer in Windows' uninstall list".to_string())?;
    let cmd = program
        .registered_uninstall(quiet)
        .map_err(str::to_string)?;
    let (exe, args) = leftovers::split_command(cmd)
        .ok_or_else(|| "the registered uninstall command could not be read".to_string())?;
    let path = tauri::async_runtime::spawn_blocking(move || uninstall_path(&exe))
        .await
        .map_err(|e| e.to_string())??;
    let preview = UninstallPreview {
        executable: path.to_string_lossy().into_owned(),
        args,
    };
    Ok((program, path, preview))
}

/// Read and validate the vendor invocation. Never runs a process.
#[tauri::command]
async fn preview_uninstaller(program_id: String) -> Result<UninstallPreview, String> {
    live_uninstall(program_id, false)
        .await
        .map(|(_, _, preview)| preview)
}

/// Re-read the live machine registration. Refuse if it no longer matches the reviewed plan.
#[tauri::command]
async fn run_uninstaller(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    program_id: String,
    quiet: bool,
    expected: UninstallPreview,
) -> Result<UninstallRun, String> {
    let (mut p, exe, actual) = live_uninstall(program_id, quiet).await?;
    if actual != expected {
        return Err("The registered uninstaller changed. Close this wizard and review its new preview before continuing.".into());
    }
    let args = actual.args;
    dock::log(
        &app,
        "info",
        format!("Running the uninstaller for {}…", p.name),
    );
    let status = tauri::async_runtime::spawn_blocking(move || start_uninstaller(&exe, &args))
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
    p.uninstall_command = None;
    p.quiet_uninstall_command = None;
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
    state
        .recent_uninstalls
        .lock()
        .map_err(|_| "internal lock poisoned".to_string())?
        .insert(p.id.clone(), p);
    Ok(UninstallRun {
        exit_code: code,
        summary,
    })
}

#[derive(Serialize)]
struct LeftoverReview {
    folders: Vec<Leftover>,
    system_entries: Vec<ReviewLeftover>,
    warnings: Vec<String>,
}

/// Read-only review of folders and system entries that might still be present.
#[tauri::command]
async fn find_leftovers(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    program_id: String,
) -> Result<LeftoverReview, String> {
    let p = review_program(&state, program_id).await?;
    let review = tauri::async_runtime::spawn_blocking(move || {
        let folders = leftovers::find_leftovers(&p, &leftover_roots());
        #[cfg(windows)]
        let (registry, mut warnings) = exor_win::enumerate_software_keys(p.publisher.as_deref());
        #[cfg(not(windows))]
        let (registry, mut warnings) = (
            Vec::new(),
            vec!["Registry keys can only be checked on Windows.".into()],
        );
        let probe = probe();
        let services = probe.services().unwrap_or_else(|e| {
            warnings.push(format!("Services were not checked: {e}"));
            Vec::new()
        });
        let tasks = probe.tasks().unwrap_or_else(|e| {
            warnings.push(format!("Scheduled tasks were not checked: {e}"));
            Vec::new()
        });
        let system_entries = leftovers::review_system_leftovers(&p, &registry, &services, &tasks);
        LeftoverReview {
            folders,
            system_entries,
            warnings,
        }
    })
    .await
    .map_err(|e| e.to_string())?;
    dock::log(
        &app,
        "info",
        format!(
            "Found {} possible leftover folder(s) and {} system entry/entries for review.",
            review.folders.len(),
            review.system_entries.len()
        ),
    );
    Ok(review)
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
    let p = review_program(&state, program_id).await?;
    let allowed = tauri::async_runtime::spawn_blocking(move || {
        leftovers::find_leftovers(&p, &leftover_roots())
    })
    .await
    .map_err(|e| e.to_string())?;
    if folders.is_empty()
        || folders
            .iter()
            .any(|f| !allowed.iter().any(|l| l.can_quarantine && &l.path == f))
    {
        return Err("only the suggested folders inside application roots can be quarantined; run Find leftovers again".into());
    }
    let chosen: Vec<PathBuf> = folders.into_iter().map(PathBuf::from).collect();
    if chosen
        .iter()
        .any(|p| inside_folder(p, &state.data_dir) || inside_folder(&state.data_dir, p))
    {
        return Err("ExorSystem profile folders are not eligible for quarantine.".into());
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
    legacy_data_folder: Option<String>,
    recovery_warnings: Vec<String>,
}

/// Version and where ExorSystem keeps its files.
#[tauri::command]
fn app_info(state: State<'_, AppState>) -> AppInfo {
    AppInfo {
        recovery_warnings: state.recovery_warnings.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        data_folder: state.data_dir.to_string_lossy().into_owned(),
        quarantine_folder: state.quarantine_root.to_string_lossy().into_owned(),
        legacy_data_folder: state
            .legacy_data
            .as_ref()
            .map(|dir| dir.to_string_lossy().into_owned()),
    }
}

/// Real system snapshots on Windows; elsewhere the engine's "not available" probe.
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
    fn registry_keys(
        &self,
        scope: &exor_engine::registry::RegistryScope,
    ) -> Result<Vec<exor_engine::registry::RegistryKeyState>, String> {
        exor_win::capture_registry_keys(scope)
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

#[derive(Serialize)]
struct WatchStart {
    session_id: i64,
    coverage: WatchCoverage,
    warnings: Vec<String>,
}

#[derive(Deserialize)]
struct WatchSystem {
    services: bool,
    tasks: bool,
}

/// Leases the cancellation token for one capture, not for the watch waiting between captures.
struct CaptureLease<'a> {
    slot: &'a Mutex<Option<(u64, Cancel)>>,
    token: Cancel,
}

impl<'a> CaptureLease<'a> {
    fn acquire(slot: &'a Mutex<Option<(u64, Cancel)>>, capture_id: u64) -> Result<Self, String> {
        let mut active = slot
            .lock()
            .map_err(|_| "internal lock poisoned".to_string())?;
        if active.is_some() {
            return Err("a watch capture is already in progress".into());
        }
        let token = Cancel::new();
        *active = Some((capture_id, token.clone()));
        Ok(Self { slot, token })
    }
}

impl Drop for CaptureLease<'_> {
    fn drop(&mut self) {
        if let Ok(mut active) = self.slot.lock() {
            *active = None;
        }
    }
}

#[tauri::command]
fn stop_watch_capture(state: State<'_, AppState>, capture_id: u64) -> Result<bool, String> {
    let slot = state
        .watch_capture
        .lock()
        .map_err(|_| "internal lock poisoned".to_string())?;
    if let Some((_, token)) = slot.as_ref().filter(|(id, _)| *id == capture_id) {
        token.cancel();
        return Ok(true);
    }
    Ok(false)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WatchRequest {
    app_name: String,
    installer_path: Option<String>,
    folders: Vec<String>,
    registry_subtrees: String,
    system: WatchSystem,
}

/// Take the baseline. Run the installer after the snapshots have finished.
#[tauri::command]
async fn begin_watch(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: WatchRequest,
    capture_id: u64,
    on_ready: Channel<()>,
) -> Result<Option<WatchStart>, String> {
    let WatchRequest {
        app_name,
        installer_path,
        folders,
        registry_subtrees,
        system,
    } = request;
    let name = app_name.trim();
    if name.is_empty() {
        return Err("enter the application name".into());
    }
    let scopes = RegistryScopes::parse(&registry_subtrees)?;
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
    // Keep the async slot reserved until the baseline finishes, even for overlapping IPC calls.
    let mut slot = state
        .watch
        .try_lock()
        .map_err(|_| "the install watch is already taking a snapshot".to_string())?;
    if slot.is_some() {
        return Err("an installation is already being watched".into());
    }
    let capture = CaptureLease::acquire(&state.watch_capture, capture_id)?;
    let _ = on_ready.send(());
    dock::log(
        &app,
        "info",
        format!("Taking the 'before' picture for {name}…"),
    );
    let result = InstallWatch::begin(
        Arc::clone(&state.store),
        name,
        installer,
        WatchPlan {
            folders: &roots,
            registry: &scopes,
            services: system.services,
            tasks: system.tasks,
        },
        probe(),
        &capture.token,
    )
    .await;
    let w = match result {
        Ok(w) if capture.token.is_cancelled() => {
            w.abandon().map_err(|e| e.to_string())?;
            dock::log(
                &app,
                "info",
                "Watch baseline cancelled. No changes were recorded.",
            );
            return Ok(None);
        }
        Ok(w) => w,
        Err(EngineError::Cancelled) => {
            dock::log(
                &app,
                "info",
                "Watch baseline cancelled. No changes were recorded.",
            );
            return Ok(None);
        }
        Err(e) => return Err(e.to_string()),
    };
    let start = WatchStart {
        session_id: w.session_id(),
        coverage: w.coverage(),
        warnings: w.warnings().to_vec(),
    };
    *slot = Some(w);
    if start.warnings.is_empty() {
        dock::log(&app, "info", format!("Baseline ready for {name}."));
    } else {
        dock::log(
            &app,
            "warn",
            format!("Baseline ready for {name}; some scopes were skipped."),
        );
    }
    Ok(Some(start))
}

/// Take the after snapshots and record the observed changes in one session.
#[tauri::command]
async fn finish_watch(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    capture_id: u64,
    on_ready: Channel<()>,
) -> Result<Option<WatchSummary>, String> {
    let mut slot = state
        .watch
        .try_lock()
        .map_err(|_| "the install watch is already taking a snapshot".to_string())?;
    let capture = CaptureLease::acquire(&state.watch_capture, capture_id)?;
    let w = slot
        .take()
        .ok_or_else(|| "no installation is being watched".to_string())?;
    let _ = on_ready.send(());
    dock::log(&app, "info", "Taking the 'after' picture and comparing…");
    let r = w.finish(&capture.token).await;
    match &r {
        Ok(s) if !s.coverage.has_usable() => {
            dock::log(&app, "warn", "No watch scopes could be compared.")
        }
        Ok(s) if !s.warnings.is_empty() => dock::log(
            &app,
            "warn",
            "Watch compared the available scopes; some were skipped.",
        ),
        Ok(_) => dock::log(&app, "info", "Watch comparison complete."),
        Err(EngineError::Cancelled) => {
            dock::log(
                &app,
                "info",
                "Watch comparison cancelled. No changes were recorded.",
            );
            return Ok(None);
        }
        Err(e) => dock::log(&app, "error", format!("Recording failed: {e}")),
    }
    r.map(Some).map_err(|e| e.to_string())
}

/// Stop watching without recording anything.
#[tauri::command]
fn cancel_watch(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    if let Some(w) = state
        .watch
        .try_lock()
        .map_err(|_| "the install watch is already taking a snapshot".to_string())?
        .take()
    {
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

/// Locate the old portable data, without trusting or importing it into the new profile.
fn portable_data_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("data")))
        .unwrap_or_else(|| PathBuf::from("data"))
}

fn run() -> Result<(), String> {
    #[cfg(windows)]
    if exor_win::process_is_elevated()? {
        return Err("ExorSystem does not run with administrator rights. Open it normally from Start or File Explorer, not from an elevated terminal.".into());
    }
    #[cfg(windows)]
    let (dir, legacy_data) = {
        let legacy = portable_data_dir();
        (
            exor_win::prepare_user_data_dir()?,
            legacy.is_dir().then_some(legacy),
        )
    };
    #[cfg(not(windows))]
    let (dir, legacy_data) = (portable_data_dir(), None);
    #[cfg(not(windows))]
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let profile_lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(dir.join("desktop.lock"))
        .map_err(|e| format!("cannot open the profile lock: {e}"))?;
    profile_lock.try_lock().map_err(|e| format!("Cannot lock the ExorSystem profile. Close any other copy of ExorSystem and try again. ({e})"))?;
    let store = Store::open(&dir.join("exor.db")).map_err(|e| e.to_string())?;
    let interrupted = store
        .recover_unfinished_sessions()
        .map_err(|e| e.to_string())?;
    let quarantine_root = dir.join("quarantine");
    let recovery = execute::recover_interrupted(&store, &quarantine_root).unwrap_or_else(|e| {
        execute::RecoveryReport {
            warnings: vec![e.to_string()],
            ..Default::default()
        }
    });
    let recovery_warnings = recovery.warnings.clone();
    tauri::Builder::default()
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_denylist(&["log"])
                .build(),
        )
        .manage(dock::Dock::default())
        .manage(AppState {
            store: Arc::new(Mutex::new(store)),
            watch: AsyncMutex::new(None),
            watch_capture: Mutex::new(None),
            scan_cancel: Mutex::new(None),
            recent_uninstalls: Mutex::new(BTreeMap::new()),
            data_dir: dir,
            quarantine_root,
            legacy_data,
            recovery_warnings,
            _profile_lock: profile_lock,
        })
        .setup(move |app| {
            if let Some(main) = app.get_webview_window("main") {
                let handle = app.handle().clone();
                main.on_window_event(move |e| match e {
                    WindowEvent::Moved(_)
                    | WindowEvent::Resized(_)
                    | WindowEvent::ScaleFactorChanged { .. }
                    | WindowEvent::Focused(true) => dock::follow(&handle),
                    // the hidden log window must not keep the app alive
                    WindowEvent::CloseRequested { .. } => handle.exit(0),
                    _ => {}
                });
            }
            if let Some(log) = app.get_webview_window("log") {
                let handle = app.handle().clone();
                log.on_window_event(move |e| match e {
                    WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        dock::close(&handle);
                    }
                    WindowEvent::ScaleFactorChanged { .. } => dock::follow(&handle),
                    _ => {}
                });
            }
            dock::log(app.handle(), "info", "ExorSystem started.");
            if interrupted > 0 {
                dock::log(app.handle(), "warn", format!("Closed {interrupted} unfinished installation session(s). Their snapshots could not be recovered."));
            }
            if recovery.rolled_back > 0 || recovery.finalized > 0 {
                dock::log(
                    app.handle(),
                    "info",
                    format!(
                        "Quarantine recovery: {} rolled back, {} finalized.",
                        recovery.rolled_back, recovery.finalized
                    ),
                );
            }
            for warning in &recovery.warnings {
                dock::log(app.handle(), "warn", warning.clone());
            }
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
            archive::quarantine_batch,
            restore_quarantine,
            purge_quarantine,
            inventory::list_programs,
            inventory::list_portable_programs,
            inventory::register_portable_program,
            inventory::forget_portable_program,
            archive::history,
            archive::save_changes,
            archive::watch_comparison,
            archive::recent_watch_runs,
            export_report,
            list_templates,
            app_info,
            update_risk,
            make_portable,
            preview_uninstaller,
            run_uninstaller,
            find_leftovers,
            quarantine_leftovers,
            archive::session_items,
            begin_watch,
            finish_watch,
            stop_watch_capture,
            cancel_watch
        ])
        .run(tauri::generate_context!())
        .map_err(|e| e.to_string())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("ExorSystem failed to start: {e}");
        #[cfg(windows)]
        exor_win::show_startup_error(&e);
        std::process::exit(1);
    }
}
