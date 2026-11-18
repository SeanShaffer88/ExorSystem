//! Read saved evidence and explicitly export bounded observations without re-probing the system.

use crate::{AppState, dock, jobs};
use exor_engine::execute::{self, BatchPreview};
use exor_engine::report::{self, ChangeExport, SessionEvidencePage};
use exor_storage::{
    ChangeDomain, ChangeKind, LedgerEntry, LedgerQuery, Store, WatchRun, WatchSummary,
};
use serde::Serialize;
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

/// Read the saved scope coverage for this session without re-reading the system.
#[tauri::command]
pub(crate) async fn watch_comparison(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: i64,
) -> Result<Option<WatchSummary>, String> {
    jobs::read(&app, &state, move |store| {
        store
            .watch_comparison(session_id)
            .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub(crate) async fn recent_watch_runs(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<WatchRun>, String> {
    jobs::read(&app, &state, |store| {
        store.recent_watch_runs(6).map_err(|e| e.to_string())
    })
    .await
}

fn scope_filter(filter: Option<&str>) -> Option<&str> {
    filter.map(str::trim).filter(|s| !s.is_empty())
}

#[derive(Default, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct HistoryFilters {
    filter: Option<String>,
    session_id: Option<i64>,
    before_id: Option<i64>,
    domain: Option<ChangeDomain>,
    change_kind: Option<ChangeKind>,
    from_date: Option<String>,
    before_date: Option<String>,
}

fn midnight(day: Option<&str>) -> Result<Option<String>, String> {
    let Some(day) = day else { return Ok(None) };
    if day.len() != 10 {
        return Err("Enter dates as YYYY-MM-DD (UTC).".into());
    }
    let parts: Vec<_> = day.split('-').collect();
    if parts.len() != 3
        || parts[0].len() != 4
        || parts[1].len() != 2
        || parts[2].len() != 2
        || parts.iter().any(|s| !s.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err("Enter dates as YYYY-MM-DD (UTC).".into());
    }
    let year: u32 = parts[0].parse().map_err(|_| "Invalid year.")?;
    let month: u32 = parts[1].parse().map_err(|_| "Invalid month.")?;
    let date: u32 = parts[2].parse().map_err(|_| "Invalid day.")?;
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400)) {
                29
            } else {
                28
            }
        }
        _ => 0,
    };
    if year == 0 || !(1..=days).contains(&date) {
        return Err("The selected UTC date is not valid.".into());
    }
    Ok(Some(format!("{day} 00:00:00")))
}

impl HistoryFilters {
    fn query<T>(
        &self,
        exec: impl FnOnce(LedgerQuery<'_>) -> Result<T, String>,
    ) -> Result<T, String> {
        if self.filter.as_ref().is_some_and(|f| f.len() > 4096) {
            return Err("Path filter is too long.".into());
        }
        let from = midnight(self.from_date.as_deref())?;
        let until = midnight(self.before_date.as_deref())?;
        if let (Some(a), Some(b)) = (&from, &until)
            && a >= b
        {
            return Err("Pick a Before date later than the From date.".into());
        }
        exec(LedgerQuery {
            session_id: self.session_id,
            path_contains: scope_filter(self.filter.as_deref()),
            before_id: self.before_id,
            domain: self.domain,
            change_kind: self.change_kind,
            from_utc: from.as_deref(),
            until_utc: until.as_deref(),
        })
    }
}

/// Read one page, including observations without a known node.
#[tauri::command]
pub(crate) async fn history(
    app: AppHandle,
    state: State<'_, AppState>,
    filters: HistoryFilters,
    limit: usize,
) -> Result<HistoryPage, String> {
    jobs::read(&app, &state, move |store| {
        filters.query(|query| {
            let page = store
                .ledger_query(limit.min(100), query)
                .map_err(|e| e.to_string())?;
            Ok(HistoryPage {
                rows: page.entries.into_iter().map(Into::into).collect(),
                next_before_id: page.next_before_id,
            })
        })
    })
    .await
}

/// Export a bounded page using the selected filters and exclusive continuation cursor.
#[tauri::command]
pub(crate) async fn save_changes(
    app: AppHandle,
    state: State<'_, AppState>,
    filters: HistoryFilters,
) -> Result<ChangeExport, String> {
    let job = state.jobs.work(&app, "Saving change observations", None)?;
    let db = state.data_dir.join("exor.db");
    let dir = app
        .path()
        .document_dir()
        .map_err(|e| format!("could not locate Documents: {e}"))?;
    let result = jobs::run(job, move || {
        let store = Store::open_reader(&db).map_err(|e| e.to_string())?;
        filters.query(|query| {
            report::export_change_page(&store, &dir, query).map_err(|e| e.to_string())
        })
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
    app: AppHandle,
    state: State<'_, AppState>,
    quarantine_id: String,
    offset: usize,
) -> Result<BatchPreview, String> {
    jobs::read(&app, &state, move |store| {
        execute::preview_batch(store, &quarantine_id, offset).map_err(|e| e.to_string())
    })
    .await
}

/// Browse evidence across all recorded nodes rather than exporting a first-page sample to the UI.
#[tauri::command]
pub(crate) async fn session_items(
    app: AppHandle,
    state: State<'_, AppState>,
    session_id: i64,
    filter: Option<String>,
    offset: usize,
) -> Result<SessionEvidencePage, String> {
    jobs::read(&app, &state, move |store| {
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
