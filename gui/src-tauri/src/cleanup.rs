//! Main-window-only approval protocol. IPC accepts an operation id, not mutation paths.

use crate::{AppState, QuarantineResult, dock, inventory, jobs, leftover_roots, review_program};
use exor_engine::cleanup::{self, MAX_ROOTS, ReviewHeader, ReviewPage, Reviews};
use exor_engine::leftovers::{self, Leftover};
use exor_engine::{Cancel, execute, templates};
use exor_storage::{SessionStatus, Store};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{AppHandle, State};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PlanRequest {
    program_id: String,
    session_id: Option<i64>,
    template: Option<String>,
    folders: Vec<String>,
}

pub(crate) type Plans = Arc<Reviews<(PlanRequest, exor_engine::programs::InstalledProgram)>>;

fn guarded_roots(
    program: &exor_engine::programs::InstalledProgram,
    request: &PlanRequest,
) -> Result<Vec<PathBuf>, String> {
    let allowed = leftovers::find_leftovers(program, &leftover_roots());
    if request.folders.is_empty()
        || request.folders.len() > MAX_ROOTS
        || request
            .folders
            .iter()
            .any(|f| !allowed.iter().any(|p| p.can_quarantine && &p.path == f))
    {
        return Err(
            "Choose the suggested application folders again. A folder is no longer eligible."
                .into(),
        );
    }
    Ok(request.folders.iter().map(PathBuf::from).collect())
}

#[tauri::command]
pub(crate) async fn cleanup_folders(
    app: AppHandle,
    state: State<'_, AppState>,
    program_id: String,
) -> Result<Vec<Leftover>, String> {
    let job = state.jobs.work(&app, "Reading application folders", None)?;
    let program = review_program(&state, program_id).await?;
    jobs::run(job, move || {
        Ok(leftovers::find_leftovers(&program, &leftover_roots()))
    })
    .await
}

#[tauri::command]
pub(crate) async fn link_cleanup_session(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: i64,
    program_id: String,
) -> Result<(), String> {
    let job = state.jobs.work(&app, "Linking an application", None)?;
    let program = inventory::registry_program(program_id)
        .await?
        .ok_or("Choose a current classic registration.")?;
    let db = state.data_dir.join("exor.db");
    jobs::run(job, move || {
        let store = Store::open_writer(&db).map_err(|e| e.to_string())?;
        if !store
            .session(session_id)
            .map_err(|e| e.to_string())?
            .is_some_and(|s| s.status == Some(SessionStatus::Completed))
        {
            return Err("Only a completed session can be linked.".into());
        }
        store
            .link_program(session_id, &program.id, &program.name)
            .map_err(|e| e.to_string())
    })
    .await
}

fn check_session(store: &Store, request: &PlanRequest) -> Result<(), String> {
    let id = match (request.session_id, &request.template) {
        (None, None) => return Ok(()),
        (Some(id), Some(_)) => id,
        _ => return Err(
            "Choose a completed session with a policy, or review leftovers without attribution."
                .into(),
        ),
    };
    if !store
        .session(id)
        .map_err(|e| e.to_string())?
        .is_some_and(|s| s.status == Some(SessionStatus::Completed))
    {
        return Err("Choose a completed session.".into());
    }
    if store
        .program_link(id)
        .map_err(|e| e.to_string())?
        .as_deref()
        != Some(request.program_id.as_str())
    {
        return Err(
            "Explicitly link this session to the selected Windows registration first.".into(),
        );
    }
    Ok(())
}

#[tauri::command]
pub(crate) async fn prepare_cleanup(
    app: AppHandle,
    state: State<'_, AppState>,
    request: PlanRequest,
) -> Result<ReviewHeader, String> {
    if request.program_id.len() > 1024
        || request.folders.len() > MAX_ROOTS
        || request.folders.iter().any(|p| p.len() > 4096)
    {
        return Err("Cleanup request exceeds the review limit.".into());
    }
    let cancel = Cancel::new();
    let job = state
        .jobs
        .work(&app, "Preparing cleanup", Some(cancel.clone()))?;
    let program = review_program(&state, request.program_id.clone()).await?;
    let profile = state.data_dir.clone();
    let plans = Arc::clone(&state.cleanup_plans);
    jobs::run(job, move || {
        let store = Store::open_reader(&profile.join("exor.db")).map_err(|e| e.to_string())?;
        check_session(&store, &request)?;
        let roots = guarded_roots(&program, &request)?;
        let plan = if let (Some(id), Some(template)) = (request.session_id, &request.template) {
            let rules = templates::rules_for(template).ok_or("Unknown cleanup policy.")?;
            cleanup::prepare_policy(&store, id, &rules, &roots, &profile, &cancel)
        } else {
            cleanup::prepare_leftovers(&roots, &profile, request.session_id, &cancel)
        }
        .map_err(|e| e.to_string())?;
        cancel.check().map_err(|e| e.to_string())?;
        plans
            .insert((request, program), plan)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub(crate) fn stop_cleanup_preflight(state: State<'_, AppState>) -> bool {
    state.jobs.stop_preflight()
}

#[tauri::command]
pub(crate) fn cleanup_plan_page(
    state: State<'_, AppState>,
    id: u64,
    offset: usize,
    excluded: bool,
) -> Result<ReviewPage, String> {
    state
        .cleanup_plans
        .page(id, offset, excluded)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(crate) fn discard_cleanup_plan(state: State<'_, AppState>, id: u64) {
    state.cleanup_plans.discard(id);
}

#[tauri::command]
pub(crate) async fn apply_cleanup(
    app: AppHandle,
    state: State<'_, AppState>,
    id: u64,
    approved_files: usize,
) -> Result<QuarantineResult, String> {
    let job = state.jobs.work(&app, "Applying reviewed cleanup", None)?;
    let ((request, reviewed), plan) = state
        .cleanup_plans
        .take_approved(id, approved_files)
        .map_err(|e| e.to_string())?;
    let program = review_program(&state, request.program_id.clone()).await?;
    if program != reviewed {
        return Err(
            "The Windows registration changed after review. Build a new cleanup plan.".into(),
        );
    }
    let profile = state.data_dir.clone();
    let root = state.quarantine_root.clone();
    let result = jobs::run(job, move || {
        let store = Store::open_writer(&profile.join("exor.db")).map_err(|e| e.to_string())?;
        check_session(&store, &request)?;
        let allowed = guarded_roots(&program, &request)?;
        plan.check_roots(&allowed, &profile).map_err(|e| e.to_string())?;
        if let (Some(session), Some(template)) = (request.session_id, &request.template) {
            let rules = templates::rules_for(template).ok_or("Unknown cleanup policy.")?;
            cleanup::recheck_policy(&store, &plan, session, &rules).map_err(|e| e.to_string())?;
        }
        let (manifest, _) = execute::record_reviewed(&store, &root, plan.session_id, &plan.files, &Cancel::new()).map_err(|e| e.to_string())?;
        let moved = plan.files.len();
        let skipped = plan.exclusions.len();
        Ok(QuarantineResult { quarantine_id: manifest.quarantine_id, moved, skipped,
            summary: format!("Moved {moved} reviewed file(s). {skipped} excluded item(s) were left alone. Held files are available for restore.") })
    }).await?;
    dock::log(&app, "info", &result.summary);
    Ok(result)
}
