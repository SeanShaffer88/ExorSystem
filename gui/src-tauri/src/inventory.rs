//! Merge the classic list, Windows packages and opt-in portable entries for display.

use super::{AppState, dock, jobs, prune_uninstalls};
use exor_engine::programs::{self, InstalledProgram, ProgramSource};
use exor_storage::{PortableProgram, Session, Store};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};
use tauri::State;

#[derive(Serialize)]
struct ProgramRow {
    program: InstalledProgram,
    session_ids: Vec<i64>,
    inferred_session_ids: Vec<i64>,
    /// Present only as a review target because its uninstall entry disappeared this run.
    removed: bool,
}

impl ProgramRow {
    fn new(
        program: InstalledProgram,
        sessions: &[Session],
        links: &BTreeMap<i64, String>,
        removed: bool,
    ) -> Self {
        let session_ids = sessions
            .iter()
            .filter(|s| links.get(&s.session_id) == Some(&program.id))
            .map(|s| s.session_id)
            .collect();
        let inferred_session_ids = if program.source == ProgramSource::Registry {
            programs::match_sessions(&program, sessions)
                .iter()
                .filter(|s| !links.contains_key(&s.session_id))
                .map(|s| s.session_id)
                .collect()
        } else {
            vec![]
        };
        Self {
            program,
            session_ids,
            inferred_session_ids,
            removed,
        }
    }
}

#[derive(Serialize)]
pub(super) struct ProgramInventory {
    programs: Vec<ProgramRow>,
    warnings: Vec<String>,
}

#[tauri::command]
pub(super) async fn list_programs(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    include_system: bool,
) -> Result<ProgramInventory, String> {
    let _job = state.jobs.work(&app, "Reading Windows inventory", None)?;
    #[cfg(windows)]
    let (mut raw, mut warnings) = tauri::async_runtime::spawn_blocking(|| {
        let mut warnings = Vec::new();
        let mut raw = match exor_win::enumerate_programs() {
            Ok(entries) => entries,
            Err(e) => {
                warnings.push(format!("Classic programs could not be listed: {e}"));
                Vec::new()
            }
        };
        match exor_win::enumerate_packages() {
            Ok(packages) => raw.extend(packages),
            Err(e) => warnings.push(format!("MSIX apps could not be listed: {e}")),
        }
        (raw, warnings)
    })
    .await
    .map_err(|e| e.to_string())?;
    #[cfg(not(windows))]
    let (mut raw, mut warnings) = (Vec::new(), Vec::new());
    let (portable, sessions, links) = jobs::read(&app, &state, |store| {
        let portable = store.portable_programs().map_err(|e| e.to_string())?;
        let sessions = store.recent_sessions(150).map_err(|e| e.to_string())?;
        let links = sessions
            .iter()
            .filter_map(|s| match store.program_link(s.session_id) {
                Ok(Some(id)) => Some(Ok((s.session_id, id))),
                Ok(None) => None,
                Err(e) => Some(Err(e.to_string())),
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        Ok((portable, sessions, links))
    })
    .await?;
    raw.extend(portable.into_iter().map(programs::portable_program));
    let current_ids: HashSet<_> = raw.iter().map(|p| p.id.clone()).collect();
    let mut programs: Vec<_> = programs::visible(raw, include_system)
        .into_iter()
        .map(|p| ProgramRow::new(p, &sessions, &links, false))
        .collect();
    {
        let mut recent = state
            .recent_uninstalls
            .lock()
            .map_err(|_| "recent uninstalls unavailable")?;
        prune_uninstalls(&mut recent);
        programs.extend(
            recent
                .values()
                .map(|(_, p)| p)
                .filter(|p| !current_ids.contains(&p.id) && (include_system || p.is_listed()))
                .cloned()
                .map(|p| ProgramRow::new(p, &sessions, &links, true)),
        );
    }
    if programs.len() > 2_000 {
        programs.truncate(2_000);
        warnings.push("Inventory reached its display limit (2,000 entries). Use Windows Settings for other registrations.".into());
    }
    dock::log(
        &app,
        "info",
        format!("Listed {} program entries.", programs.len()),
    );
    Ok(ProgramInventory { programs, warnings })
}

/// Only live classic registry entries are ever considered as uninstall commands.
pub(super) async fn registry_program(id: String) -> Result<Option<InstalledProgram>, String> {
    #[cfg(windows)]
    {
        tauri::async_runtime::spawn_blocking(move || exor_win::registry_program(&id))
            .await
            .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = id;
        Ok(None)
    }
}

#[tauri::command]
pub(super) async fn list_portable_programs(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<PortableProgram>, String> {
    jobs::read(&app, &state, |store| {
        store.portable_programs().map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub(super) async fn register_portable_program(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    name: String,
    executable: String,
) -> Result<(), String> {
    let job = state.jobs.work(&app, "Registering a portable app", None)?;
    let db = state.data_dir.join("exor.db");
    jobs::run(job, move || {
        let entry = programs::portable_registration(&name, &executable)?;
        let store = Store::open_writer(&db).map_err(|e| e.to_string())?;
        if store.portable_programs().map_err(|e| e.to_string())?.len() >= 250 {
            return Err("Portable inventory is full. Forget an entry first.".into());
        }
        store
            .save_portable_program(&entry)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub(super) async fn forget_portable_program(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    executable: String,
) -> Result<(), String> {
    let job = state.jobs.work(&app, "Forgetting a portable app", None)?;
    let db = state.data_dir.join("exor.db");
    jobs::run(job, move || {
        Store::open_writer(&db)
            .and_then(|s| s.forget_portable_program(&executable))
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
}
