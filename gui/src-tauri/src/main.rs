//! ExorSystem desktop shell: thin Tauri commands over `exor-storage` and `exor-engine`.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use exor_engine::leftovers::{self, Leftover, ReviewLeftover};
use exor_engine::programs::InstalledProgram;
use exor_engine::registry::RegistryScopes;
use exor_engine::snapshot::{CaptureOptions, Progress, capture_with_progress};
use exor_engine::watch::{
    InstallWatch, SystemProbe, WatchCoverage, WatchPlan, WatchProgress, WatchSummary,
};
use exor_engine::{Cancel, EngineError, execute, policy, report, templates};
use exor_storage::{
    BatchSummary, Edge, LedgerEntry, Node, NodeType, RelationshipType, Session, Store,
};
use serde::{Deserialize, Serialize};
mod archive;
mod cleanup;
mod dock;
mod inventory;
mod jobs;
mod profile;

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
    recent_uninstalls: Mutex<BTreeMap<String, (std::time::Instant, InstalledProgram)>>,
    cleanup_plans: cleanup::Plans,
    jobs: jobs::Gate,
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
    total: i64,
    next_offset: Option<usize>,
}

#[tauri::command]
async fn list_sessions(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<Session>, String> {
    jobs::read(&app, &state, |s| {
        s.recent_sessions(150).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
async fn provenance(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<ProvenanceView, String> {
    if path.len() > 4096 {
        return Err("Path exceeds the query limit.".into());
    }
    jobs::read(&app, &state, move |store| match store
        .provenance_for_path(&path)
        .map_err(|e| e.to_string())?
    {
        Some(p) => Ok(ProvenanceView {
            found: true,
            summary: report::describe_provenance(&p),
            confidence: p.confidence,
            graph: Some(
                relationship_graph(
                    store,
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
    }).await
}

#[tauri::command]
async fn simulate(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: i64,
    template: String,
    offset: Option<usize>,
) -> Result<SimulationView, String> {
    jobs::read(&app, &state, move |store| {
        let rules = templates::rules_for(&template).ok_or("Unknown cleanup policy.")?;
        let offset = offset.unwrap_or(0);
        let page = store
            .session_nodes_page(session_id, None, offset, 40)
            .map_err(|e| e.to_string())?;
        let r = policy::simulate_nodes(store, &rules, &page.nodes).map_err(|e| e.to_string())?;
        let next = offset.saturating_add(r.decisions.len());
        Ok(SimulationView {
            summary: format!("This page: {}", report::summarize_simulation(&r)),
            keep: r.keep,
            quarantine: r.quarantine,
            flag_for_review: r.flag_for_review,
            next_offset: (i64::try_from(next).unwrap_or(i64::MAX) < page.total).then_some(next),
            total: page.total,
            decisions: r.decisions,
        })
    })
    .await
}

#[derive(Serialize)]
struct QuarantineResult {
    quarantine_id: String,
    moved: usize,
    skipped: usize,
    summary: String,
}

#[derive(Serialize)]
struct QuarantinePage {
    rows: Vec<BatchSummary>,
    next_offset: Option<usize>,
}

#[tauri::command]
async fn list_quarantines(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    offset: Option<usize>,
) -> Result<QuarantinePage, String> {
    let offset = offset.unwrap_or(0);
    jobs::read(&app, &state, move |store| {
        let mut rows = store
            .batch_summaries(41, offset)
            .map_err(|e| e.to_string())?;
        let next_offset = (rows.len() > 40).then_some(offset.saturating_add(40));
        rows.truncate(40);
        Ok(QuarantinePage { rows, next_offset })
    })
    .await
}

/// Put quarantined files back. Never overwrites a file that has reappeared.
#[tauri::command]
async fn restore_quarantine(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    quarantine_id: String,
) -> Result<usize, String> {
    let job = state.jobs.work(&app, "Restore quarantine batch", None)?;
    let db = state.data_dir.join("exor.db");
    let root = state.quarantine_root.clone();
    let r = jobs::run(job, move || {
        let store = Store::open_writer(&db).map_err(|e| e.to_string())?;
        execute::restore_by_id(&store, &root, &quarantine_id)
            .map(|m| m.items.len())
            .map_err(|e| e.to_string())
    })
    .await;
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
async fn purge_quarantine(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    quarantine_id: String,
) -> Result<usize, String> {
    let job = state.jobs.work(&app, "Purge quarantine batch", None)?;
    let db = state.data_dir.join("exor.db");
    let root = state.quarantine_root.clone();
    let r = jobs::run(job, move || {
        let store = Store::open_writer(&db).map_err(|e| e.to_string())?;
        execute::purge_by_id(&store, &root, &quarantine_id)
            .map(|m| m.items.len())
            .map_err(|e| e.to_string())
    })
    .await;
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
    let mut recent = state
        .recent_uninstalls
        .lock()
        .map_err(|_| "recent uninstall cache unavailable")?;
    prune_uninstalls(&mut recent);
    recent
        .get(&program_id)
        .map(|(_, p)| p.clone())
        .ok_or_else(|| "No current install information is available for this program.".into())
}

fn prune_uninstalls(cache: &mut BTreeMap<String, (std::time::Instant, InstalledProgram)>) {
    cache.retain(|_, (time, _)| time.elapsed() < std::time::Duration::from_secs(3600));
}

/// Folders where programs usually leave files.
fn leftover_roots() -> Vec<PathBuf> {
    leftovers::default_roots()
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

fn uninstall_arguments(command: &str) -> Result<(PathBuf, Vec<String>), String> {
    #[cfg(windows)]
    {
        exor_win::uninstall_arguments(command)
    }
    #[cfg(not(windows))]
    {
        let _ = command;
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
    let cmd = cmd.to_string();
    let (path, args) = tauri::async_runtime::spawn_blocking(move || uninstall_arguments(&cmd))
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
async fn preview_uninstaller(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    program_id: String,
) -> Result<UninstallPreview, String> {
    let _job = state
        .jobs
        .work(&app, "Reading the uninstaller registration", None)?;
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
    let _job = state.jobs.work(&app, "The vendor uninstaller", None)?;
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
    let mut recent = state
        .recent_uninstalls
        .lock()
        .map_err(|_| "recent uninstall cache unavailable")?;
    prune_uninstalls(&mut recent);
    if recent.len() >= 32
        && let Some(id) = recent
            .iter()
            .min_by_key(|(_, (t, _))| *t)
            .map(|(id, _)| id.clone())
    {
        recent.remove(&id);
    }
    recent.insert(p.id.clone(), (std::time::Instant::now(), p));
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
    let job = state.jobs.work(&app, "Checking possible leftovers", None)?;
    let p = review_program(&state, program_id).await?;
    let review = jobs::run(job, move || {
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
        let mut system_entries = system_entries;
        if system_entries.len() > 100 { warnings.push("Showing the first 100 advisory system matches; check Windows for the complete list.".into()); }
        system_entries.truncate(100);
        Ok(LeftoverReview { folders, system_entries, warnings })
    }).await?;
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
async fn export_report(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: i64,
    template: String,
) -> Result<String, String> {
    let job = state
        .jobs
        .work(&app, "Saving the installation report", None)?;
    let db = state.data_dir.join("exor.db");
    let dir = app.path().document_dir().map_err(|e| e.to_string())?;
    let shown = jobs::run(job, move || {
        let store = Store::open_reader(&db).map_err(|e| e.to_string())?;
        let name = store
            .session(session_id)
            .map_err(|e| e.to_string())?
            .ok_or("Unknown session.")?
            .app_name;
        let text =
            report::session_report(&store, session_id, &template).map_err(|e| e.to_string())?;
        let base = format!("ExorSystem report - {}", safe_name(&name));
        exor_engine::publication::write_unique(&dir, &base, "md", text.as_bytes())
            .map(|p| p.to_string_lossy().into_owned())
            .map_err(|e| e.to_string())
    })
    .await?;
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

/// Export recorded files and a plan. Does not create a runnable application.
#[tauri::command]
async fn make_portable(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    session_id: i64,
) -> Result<PortableView, String> {
    let job = state.jobs.work(&app, "Exporting recorded files", None)?;
    let db = state.data_dir.join("exor.db");
    let dir = app.path().document_dir().map_err(|e| e.to_string())?;
    let result = jobs::run(job, move || {
        let store = Store::open_reader(&db).map_err(|e| e.to_string())?;
        let name = store.session(session_id).map_err(|e| e.to_string())?.ok_or("Unknown session.")?.app_name;
        let plan = exor_engine::portable::plan_recorded(&store, session_id).map_err(|e| e.to_string())?;
        let base = format!("ExorSystem recorded files - {}", safe_name(&name));
        let (dest, r) = exor_engine::portable::export_recorded(&plan, &dir, &base).map_err(|e| e.to_string())?;
        let folder = dest.to_string_lossy().into_owned();
        let summary = format!("Copied {} file(s) to {folder}; {} missing, changed or unsupported file(s) were not copied. See portable-plan.json for the record; this is not a runnable app.", r.copied, r.missing.len());
        Ok(PortableView { folder, copied: r.copied, missing: r.missing.len(), registry_keys: plan.export_registry.len(), unsupported: plan.unsupported.len(), summary })
    }).await?;
    dock::log(&app, "info", result.summary.clone());
    Ok(result)
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
    not_assessed: Vec<RiskRow>,
}

/// How risky changing each recorded program would be, in batches (low first).
#[tauri::command]
async fn update_risk(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<RiskView, String> {
    jobs::read(&app, &state, |store| {
        let b = exor_engine::risk::batch_by_risk(
            exor_engine::risk::candidates_from_store(store).map_err(|e| e.to_string())?,
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
            not_assessed: rows(b.not_assessed),
        })
    })
    .await
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
    fn services_cancellable(
        &self,
        stop: &Cancel,
    ) -> Result<Vec<exor_engine::collectors::ServiceInfo>, String> {
        exor_win::enumerate_services_cancellable(stop).map_err(|e| e.to_string())
    }
    fn tasks_cancellable(
        &self,
        stop: &Cancel,
    ) -> Result<Vec<exor_engine::collectors::TaskInfo>, String> {
        let root = std::env::var_os("SystemRoot")
            .map(PathBuf::from)
            .ok_or("Windows system folder unavailable")?;
        exor_engine::collectors::read_tasks_cancellable(&root.join("System32").join("Tasks"), stop)
            .map_err(|e| e.to_string())
    }
    fn registry_cancellable(
        &self,
        scope: &exor_engine::registry::RegistryScope,
        stop: &Cancel,
    ) -> Result<Vec<exor_engine::registry::RegistryKeyState>, String> {
        exor_win::capture_registry_cancellable(scope, stop)
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
    #[serde(default)]
    strict: bool,
}

#[derive(Clone, Serialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
enum WatchEvent {
    Ready,
    Progress(WatchProgress),
}

/// Take the baseline. Run the installer after the snapshots have finished.
#[tauri::command]
async fn begin_watch(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    request: WatchRequest,
    capture_id: u64,
    on_ready: Channel<WatchEvent>,
) -> Result<Option<WatchStart>, String> {
    let WatchRequest {
        app_name,
        installer_path,
        folders,
        registry_subtrees,
        system,
        strict,
    } = request;
    let name = app_name.trim();
    if name.is_empty() || name.len() > 240 || name.chars().any(char::is_control) {
        return Err("Enter an application name of 1–240 bytes.".into());
    }
    if folders.len() > 8
        || folders.iter().any(|p| p.len() > 4096)
        || registry_subtrees.len() > 16_384
        || installer_path.as_ref().is_some_and(|p| p.len() > 4096)
    {
        return Err("Watch scope exceeds its input limits.".into());
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
    let _job = state
        .jobs
        .work(&app, "Watch capture", Some(capture.token.clone()))?;
    let _ = on_ready.send(WatchEvent::Ready);
    let notify = Arc::new(move |p| {
        let _ = on_ready.send(WatchEvent::Progress(p));
    });
    dock::log(
        &app,
        "info",
        format!("Taking the 'before' picture for {name}…"),
    );
    let result = InstallWatch::begin_with_progress(
        Arc::clone(&state.store),
        name,
        installer,
        WatchPlan {
            folders: &roots,
            registry: &scopes,
            services: system.services,
            tasks: system.tasks,
            strict,
        },
        probe(),
        &capture.token,
        notify,
    )
    .await;
    let w = match result {
        Ok(w) if capture.token.is_cancelled() => {
            tauri::async_runtime::spawn_blocking(move || w.abandon())
                .await
                .map_err(|e| e.to_string())?
                .map_err(|e| e.to_string())?;
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
    on_ready: Channel<WatchEvent>,
) -> Result<Option<WatchSummary>, String> {
    let mut slot = state
        .watch
        .try_lock()
        .map_err(|_| "the install watch is already taking a snapshot".to_string())?;
    let capture = CaptureLease::acquire(&state.watch_capture, capture_id)?;
    let _job = state
        .jobs
        .work(&app, "Watch capture", Some(capture.token.clone()))?;
    let w = slot
        .take()
        .ok_or_else(|| "no installation is being watched".to_string())?;
    let _ = on_ready.send(WatchEvent::Ready);
    let notify = Arc::new(move |p| {
        let _ = on_ready.send(WatchEvent::Progress(p));
    });
    dock::log(&app, "info", "Taking the 'after' picture and comparing…");
    let r = w.finish_with_progress(&capture.token, notify).await;
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
async fn cancel_watch(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let job = state
        .jobs
        .work(&app, "Discarding the watch baseline", None)?;
    if let Some(w) = state
        .watch
        .try_lock()
        .map_err(|_| "the install watch is already taking a snapshot".to_string())?
        .take()
    {
        jobs::run(job, move || w.abandon().map_err(|e| e.to_string())).await?;
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
    strict: Option<bool>,
    on_event: Channel<ScanEvent>,
) -> Result<(), String> {
    if root.len() > 4096 || !Path::new(&root).is_absolute() || root.contains('\0') {
        return Err("Enter an absolute local folder path.".into());
    }
    let cancel = Cancel::new();
    let _job = state.jobs.work(&app, "Folder scan", Some(cancel.clone()))?;
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
        &CaptureOptions {
            strict: strict.unwrap_or(false),
            ..Default::default()
        },
        None,
        &cancel,
        Arc::new(move |p| {
            let _ = progress.send(ScanEvent::Progress(p));
        }),
    )
    .await;
    if let Ok(mut slot) = state.scan_cancel.lock() {
        *slot = None;
    }
    match &result {
        Ok(snap) => dock::log(
            &app,
            "info",
            format!("Scan finished: {} file(s).", snap.files),
        ),
        Err(EngineError::Cancelled) => dock::log(&app, "warn", "Scan cancelled."),
        Err(e) => dock::log(&app, "error", format!("Scan failed: {e}")),
    }
    let event = match result {
        Ok(snap) => ScanEvent::Done {
            files: snap.files,
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
            cleanup_plans: cleanup::Plans::default(),
            jobs: jobs::Gate::default(),
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
                    WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        handle.state::<AppState>().jobs.close(&handle);
                    }
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
            dock::log_status,
            dock::clear_log,
            dock::set_log_open,
            dock::log_is_open,
            list_sessions,
            pick_path,
            profile::storage_usage,
            profile::backup_profile,
            profile::restore_profile_copy,
            profile::rotate_archived_batch,
            provenance,
            simulate,
            scan,
            cancel_scan,
            cleanup::cleanup_folders,
            cleanup::link_cleanup_session,
            cleanup::prepare_cleanup,
            cleanup::stop_cleanup_preflight,
            cleanup::cleanup_plan_page,
            cleanup::discard_cleanup_plan,
            cleanup::apply_cleanup,
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

#[tauri::command]
async fn pick_path(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    directory: bool,
) -> Result<Option<String>, String> {
    let job = state
        .jobs
        .work(window.app_handle(), "The Windows picker", None)?;
    #[cfg(windows)]
    {
        let owner = window.hwnd().map_err(|e| e.to_string())?.0 as isize;
        jobs::run(job, move || exor_win::pick_path(owner, directory)).await
    }
    #[cfg(not(windows))]
    {
        let _ = (job, directory);
        Err("Use manual path entry on this platform.".into())
    }
}
