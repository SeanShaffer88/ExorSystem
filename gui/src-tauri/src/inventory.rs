//! Merge the classic list, Windows packages and opt-in portable entries for display.

use super::{AppState, dock, lock};
use exor_engine::programs::{self, InstalledProgram, ProgramSource};
use exor_storage::{PortableProgram, Session};
use serde::Serialize;
use std::collections::HashSet;
use tauri::State;

#[derive(Serialize)]
struct ProgramRow {
    program: InstalledProgram,
    session_ids: Vec<i64>,
    /// Present only as a review target because its uninstall entry disappeared this run.
    removed: bool,
}

impl ProgramRow {
    fn new(program: InstalledProgram, sessions: &[Session], removed: bool) -> Self {
        // Never infer a cleanup target for an MSIX or portable app just from its display name.
        let session_ids = if program.source == ProgramSource::Registry {
            programs::match_sessions(&program, sessions)
                .iter()
                .map(|s| s.session_id)
                .collect()
        } else {
            Vec::new()
        };
        Self {
            program,
            session_ids,
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
    #[cfg(windows)]
    let (mut raw, warnings) = tauri::async_runtime::spawn_blocking(|| {
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
    let (mut raw, warnings) = (Vec::new(), Vec::new());
    let sessions = {
        let store = lock(&state)?;
        let portable = store.portable_programs().map_err(|e| e.to_string())?;
        raw.extend(portable.into_iter().map(programs::portable_program));
        store.sessions().map_err(|e| e.to_string())?
    };
    let current_ids: HashSet<_> = raw.iter().map(|p| p.id.clone()).collect();
    let mut programs: Vec<_> = programs::visible(raw, include_system)
        .into_iter()
        .map(|p| ProgramRow::new(p, &sessions, false))
        .collect();
    programs.extend(
        state
            .recent_uninstalls
            .lock()
            .map_err(|_| "internal lock poisoned".to_string())?
            .values()
            .filter(|p| !current_ids.contains(&p.id) && (include_system || p.is_listed()))
            .cloned()
            .map(|p| ProgramRow::new(p, &sessions, true)),
    );
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
        let raw = tauri::async_runtime::spawn_blocking(exor_win::enumerate_programs)
            .await
            .map_err(|e| e.to_string())?
            .map_err(|e| e.to_string())?;
        Ok(raw.into_iter().find(|p| p.id == id))
    }
    #[cfg(not(windows))]
    {
        let _ = id;
        Ok(None)
    }
}

#[tauri::command]
pub(super) fn list_portable_programs(
    state: State<'_, AppState>,
) -> Result<Vec<PortableProgram>, String> {
    lock(&state)?.portable_programs().map_err(|e| e.to_string())
}

#[tauri::command]
pub(super) async fn register_portable_program(
    state: State<'_, AppState>,
    name: String,
    executable: String,
) -> Result<(), String> {
    let entry = tauri::async_runtime::spawn_blocking(move || {
        programs::portable_registration(&name, &executable)
    })
    .await
    .map_err(|e| e.to_string())??;
    lock(&state)?
        .save_portable_program(&entry)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub(super) fn forget_portable_program(
    state: State<'_, AppState>,
    executable: String,
) -> Result<(), String> {
    lock(&state)?
        .forget_portable_program(&executable)
        .map(|_| ())
        .map_err(|e| e.to_string())
}
