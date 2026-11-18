//! Read saved evidence and explicitly export bounded observations without re-probing the system.

use crate::{AppState, dock};
use exor_engine::execute::{self, BatchPreview};
use exor_engine::report::{self, ChangeExport, SessionEvidencePage};
use exor_storage::{
    ChangeDomain, ChangeKind, LedgerEntry, LedgerQuery, Store, WatchRun, WatchSummary,
};
use serde::Serialize;
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

#[derive(Serialize)]
struct HistoryRow {
    id: i64,
    when: Option<String>,
    path: String,
    what: ChangeKind,
    source: exor_storage::ChangeSource,
    session_id: Option<i64>,
    domain: ChangeDomain,
    old_hash: Option<String>,
    new_hash: Option<String>,
}

impl From<LedgerEntry> for HistoryRow {
    fn from(e: LedgerEntry) -> Self {
        Self {
            id: e.entry_id,
            when: e.observed_at,
            path: e.entry.path,
            what: e.entry.change_kind,
            source: e.entry.source,
            session_id: e.entry.session_id,
            domain: e.domain,
            old_hash: e.entry.old_hash,
            new_hash: e.entry.new_hash,
        }
    }
}

#[derive(Serialize)]
pub(crate) struct HistoryPage {
    rows: Vec<HistoryRow>,
    next_before_id: Option<i64>,
}

async fn query<T: Send + 'static>(
    state: &AppState,
    f: impl FnOnce(&Store) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let store = Arc::clone(&state.store);
    tauri::async_runtime::spawn_blocking(move || {
        let store = store
            .lock()
            .map_err(|_| "internal lock poisoned".to_string())?;
        f(&store)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Read the saved scope coverage for this session without re-reading the system.
#[tauri::command]
pub(crate) async fn watch_comparison(
    state: State<'_, AppState>,
    session_id: i64,
) -> Result<Option<WatchSummary>, String> {
    query(&state, move |store| {
        store
            .watch_comparison(session_id)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub(crate) async fn recent_watch_runs(state: State<'_, AppState>) -> Result<Vec<WatchRun>, String> {
    query(&state, |store| {
        store.recent_watch_runs(6).map_err(|e| e.to_string())
    })
    .await
}

fn scope_filter(filter: Option<&str>) -> Option<&str> {
    filter.map(str::trim).filter(|s| !s.is_empty())
}

/// Read one filtered page, including observations without a known node.
#[tauri::command]
pub(crate) async fn history(
    state: State<'_, AppState>,
    filter: Option<String>,
    limit: usize,
    session_id: Option<i64>,
    before_id: Option<i64>,
    domain: Option<ChangeDomain>,
    change_kind: Option<ChangeKind>,
) -> Result<HistoryPage, String> {
    query(&state, move |store| {
        let page = store
            .ledger_query(
                limit,
                LedgerQuery {
                    path_contains: scope_filter(filter.as_deref()),
                    session_id,
                    before_id,
                    domain,
                    change_kind,
                },
            )
            .map_err(|e| e.to_string())?;
        Ok(HistoryPage {
            rows: page.entries.into_iter().map(Into::into).collect(),
            next_before_id: page.next_before_id,
        })
    })
    .await
}

/// Save the selected filters across all domains. The output destination is not settable by IPC.
#[tauri::command]
pub(crate) async fn save_changes(
    app: AppHandle,
    state: State<'_, AppState>,
    filter: Option<String>,
    session_id: Option<i64>,
    change_kind: Option<ChangeKind>,
) -> Result<ChangeExport, String> {
    let dir = app
        .path()
        .document_dir()
        .map_err(|e| format!("could not locate Documents: {e}"))?;
    let result = query(&state, move |store| {
        report::export_changes(
            store,
            &dir,
            session_id,
            scope_filter(filter.as_deref()),
            change_kind,
        )
        .map_err(|e| e.to_string())
    })
    .await?;
    dock::log(
        &app,
        "info",
        format!(
            "Saved {} change observation(s) to {}.",
            result.records, result.path
        ),
    );
    Ok(result)
}

/// Preview the database's recorded batch manifest without opening any held files.
#[tauri::command]
pub(crate) async fn quarantine_batch(
    state: State<'_, AppState>,
    quarantine_id: String,
    offset: usize,
) -> Result<BatchPreview, String> {
    query(&state, move |store| {
        execute::preview_batch(store, &quarantine_id, offset).map_err(|e| e.to_string())
    })
    .await
}

/// Browse evidence across all recorded nodes rather than exporting a first-page sample to the UI.
#[tauri::command]
pub(crate) async fn session_items(
    state: State<'_, AppState>,
    session_id: i64,
    filter: Option<String>,
    offset: usize,
) -> Result<SessionEvidencePage, String> {
    query(&state, move |store| {
        report::session_evidence_page(
            store,
            session_id,
            scope_filter(filter.as_deref()),
            offset,
            12,
        )
        .map_err(|e| e.to_string())
    })
    .await
}
