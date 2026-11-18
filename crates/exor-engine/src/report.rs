//! Accessibility-first plain-language summaries: no colour, symbols or layout carry meaning.

use crate::error::{EngineError, Result, io_err};
use crate::policy::{self, SimulationReport};
use crate::templates;
use exor_storage::{
    ChangeKind, ChangeSource, LedgerEntry, LedgerQuery, NodeType, PolicyAction, Provenance, Store,
    WatchSummary,
};
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::{ErrorKind, Write};
use std::path::Path;

/// Words for a confidence score.
pub fn confidence_words(c: f64) -> &'static str {
    if c >= 1.0 {
        "confirmed by the installer's own manifest"
    } else if c >= 0.7 {
        "very likely, based on timing during installation"
    } else {
        "uncertain, based on a guess"
    }
}

/// One-paragraph summary of a dry run, readable aloud by a screen reader.
pub fn summarize_simulation(r: &SimulationReport) -> String {
    let total = r.decisions.len();
    let mut s = format!(
        "Dry run complete. {total} items were checked. {} would be kept, {} would be moved to quarantine, and {} need your review. Nothing was changed.",
        r.keep, r.quarantine, r.flag_for_review
    );
    for d in r
        .decisions
        .iter()
        .filter(|d| d.action == PolicyAction::FlagForReview)
        .take(5)
    {
        s.push_str(&format!(" Needs review: {} ({}).", d.path, d.reason));
    }
    s
}

/// Full plain-language Markdown report for one recorded installation: what it left behind, how
/// sure we are, what a cleanup under `template` would do. Headings and lists only; no colour.
pub fn session_report(store: &Store, session_id: i64, template: &str) -> Result<String> {
    let session = store
        .session(session_id)?
        .ok_or_else(|| EngineError::Quarantine(format!("no installation #{session_id}")))?;
    let rules = templates::rules_for(template)
        .ok_or_else(|| EngineError::Quarantine(format!("unknown policy '{template}'")))?;
    let cands = policy::candidates_for_session(store, session_id)?;
    let sim = policy::simulate(&rules, &cands)?;
    let conf = |c: &policy::Candidate| {
        c.attrs
            .get("confidence")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.3)
    };
    let shared = |c: &policy::Candidate| {
        c.attrs
            .get("shared")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    };
    let mut items: Vec<&policy::Candidate> = cands.iter().collect();
    items.sort_by(|a, b| {
        conf(b)
            .total_cmp(&conf(a))
            .then_with(|| a.path.cmp(&b.path))
    });
    let mut out = format!("# Installation report: {}\n\n", session.app_name);
    if let Some(t) = &session.started_at {
        out.push_str(&format!("Recorded: {t}\n\n"));
    }
    out.push_str(&format!(
        "{} item(s) were traced to this installation, {} of them possibly shared with other programs.\n\n",
        items.len(),
        items.iter().filter(|c| shared(c)).count()
    ));
    append_watch_coverage(&mut out, store.watch_comparison(session_id)?.as_ref());
    out.push_str(&format!(
        "## Cleanup preview ({template})\n\n{}\n\n",
        summarize_simulation(&sim)
    ));
    out.push_str("## Items\n\n");
    for c in items {
        out.push_str(&format!(
            "- {} ({}): {}{}\n",
            c.path,
            serde_json::to_value(c.node_type)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default(),
            confidence_words(conf(c)),
            if shared(c) { "; may be shared" } else { "" }
        ));
    }
    Ok(out)
}

fn append_watch_coverage(out: &mut String, summary: Option<&WatchSummary>) {
    out.push_str("## Watch coverage\n\n");
    let Some(summary) = summary else {
        out.push_str("No comparison coverage was saved for this session. Coverage cannot be inferred from the change list.\n\n");
        return;
    };
    for (label, scope, changes) in [
        (
            "File system",
            summary.coverage.folders,
            summary.files_changed,
        ),
        (
            "Registry",
            summary.coverage.registry,
            summary.registry_keys_changed,
        ),
        (
            "Services",
            summary.coverage.services,
            summary.services_changed,
        ),
        (
            "Scheduled tasks",
            summary.coverage.tasks,
            summary.tasks_changed,
        ),
    ] {
        let result = match (scope.requested, scope.usable) {
            (0, _) => "Not requested".to_string(),
            (requested, 0) => format!("Skipped (0 of {requested} scopes compared)"),
            (requested, usable) => {
                format!("{usable} of {requested} scopes compared; {changes} change(s) recorded")
            }
        };
        out.push_str(&format!("- {label}: {result}.\n"));
    }
    if !summary.warnings.is_empty() {
        out.push_str("\nSkipped scope details are available in Change inspector.\n");
    }
    out.push_str("\nThese counts are for the saved snapshot comparison only. They do not identify which process changed an item.\n\n");
}

/// An attributed artifact, including the same shared-evidence marker as policy review.
#[derive(Debug, Serialize)]
pub struct SessionEvidence {
    /// Recorded path (or system identifier).
    pub path: String,
    /// Recorded artifact kind.
    pub node_type: NodeType,
    /// Confidence in the attribution, not a safety score.
    pub confidence: f64,
    /// A shared relation or a dependent from another session was observed.
    pub shared: bool,
}

/// Browse one bounded page without re-reading the filesystem or the registry.
#[derive(Debug, Serialize)]
pub struct SessionEvidencePage {
    /// Number of recorded nodes in the session before path filtering.
    pub total: i64,
    /// Number of matching nodes.
    pub matching: i64,
    /// Page of matching nodes, with confidence and shared markers.
    pub items: Vec<SessionEvidence>,
}

/// Page through all attributed nodes, computing the relationship markers only for the displayed
/// nodes. Change Inspector remains the place to look up observations that have no node.
pub fn session_evidence_page(
    store: &Store,
    session_id: i64,
    path_contains: Option<&str>,
    offset: usize,
    limit: usize,
) -> Result<SessionEvidencePage> {
    let page = store.session_nodes_page(session_id, path_contains, offset, limit)?;
    let items = page
        .nodes
        .iter()
        .map(|node| {
            let c = policy::candidate_from_node(store, node)?;
            Ok(SessionEvidence {
                path: c.path,
                node_type: c.node_type,
                confidence: c
                    .attrs
                    .get("confidence")
                    .and_then(serde_json::Value::as_f64)
                    .unwrap_or(0.3),
                shared: c
                    .attrs
                    .get("shared")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect::<Result<_>>()?;
    Ok(SessionEvidencePage {
        total: page.total,
        matching: page.matching,
        items,
    })
}

const CHANGE_EXPORT_LIMIT: usize = 1000;

/// Summary returned to the UI after saving the filtered ledger file.
#[derive(Debug, Serialize)]
pub struct ChangeExport {
    /// Where the new file was saved.
    pub path: String,
    /// Number of matching observations written.
    pub records: usize,
    /// Older matches exist beyond the export limit.
    pub truncated: bool,
}

#[derive(Serialize)]
struct ExportFilters<'a> {
    session_id: Option<i64>,
    path_contains: Option<&'a str>,
    change_kind: Option<ChangeKind>,
}

#[derive(Serialize)]
struct ChangeArchive<'a> {
    format: &'static str,
    version: u32,
    filters: ExportFilters<'a>,
    truncated: bool,
    observations: Vec<LedgerEntry>,
}

/// Save the newest matching observations across all domains (at most 1000). Export is read-only
/// with respect to the ledger and uses create-new file semantics even across racing writers.
pub fn export_changes(
    store: &Store,
    directory: &Path,
    session_id: Option<i64>,
    path_contains: Option<&str>,
    change_kind: Option<ChangeKind>,
) -> Result<ChangeExport> {
    let page = store.ledger_query(
        CHANGE_EXPORT_LIMIT,
        LedgerQuery {
            path_contains,
            session_id,
            change_kind,
            ..Default::default()
        },
    )?;
    let truncated = page.next_before_id.is_some();
    let records = page.entries.len();
    let archive = ChangeArchive {
        format: "exorsystem.observations",
        version: 1,
        filters: ExportFilters {
            session_id,
            path_contains,
            change_kind,
        },
        truncated,
        observations: page.entries,
    };
    let bytes = serde_json::to_vec_pretty(&archive)?;
    for n in 1..=1000 {
        let name = if n == 1 {
            "ExorSystem changes.json".into()
        } else {
            format!("ExorSystem changes ({n}).json")
        };
        let path = directory.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                if let Err(error) = file.write_all(&bytes).and_then(|()| file.sync_all()) {
                    drop(file);
                    let _ = std::fs::remove_file(&path);
                    return Err(io_err(path)(error));
                }
                return Ok(ChangeExport {
                    path: path.to_string_lossy().into_owned(),
                    records,
                    truncated,
                });
            }
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(io_err(path)(e)),
        }
    }
    Err(io_err(directory)(std::io::Error::new(
        ErrorKind::AlreadyExists,
        "no unused changes export name",
    )))
}

/// Plain-language provenance sentence for one artifact.
pub fn describe_provenance(p: &Provenance) -> String {
    if let Some(s) = &p.session
        && p.node.node_type == NodeType::RegistryKey
        && p.ledger.iter().any(|row| {
            row.entry.source == ChangeSource::SnapshotDiff
                && row.entry.session_id == Some(s.session_id)
        })
    {
        return format!(
            "{} was observed during the watch for {} (session {}). A snapshot cannot tell which process changed the key.",
            p.node.path, s.app_name, s.session_id
        );
    }
    let who = match &p.session {
        Some(s) => format!(
            "installed by {} (session {}{})",
            s.app_name,
            s.session_id,
            s.fingerprint_family
                .as_deref()
                .map(|f| format!(", {f} installer"))
                .unwrap_or_default()
        ),
        None => "not linked to any recorded installation".to_string(),
    };
    format!(
        "{} was {}. Attribution is {}.",
        p.node.path,
        who,
        confidence_words(p.confidence)
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod report_tests {
    use super::*;

    #[test]
    fn export_uses_the_filters_includes_all_domains_and_never_overwrites() {
        use exor_storage::NewLedgerEntry;
        let dir = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let session = store.begin_session("Fixture", None, None).unwrap();
        let append = |session_id, path: String, change_kind| {
            store
                .append_ledger(&NewLedgerEntry {
                    session_id,
                    node_id: None,
                    path,
                    change_kind,
                    old_hash: None,
                    new_hash: None,
                    source: ChangeSource::SnapshotDiff,
                })
                .unwrap();
        };
        for i in 0..1001 {
            append(
                Some(session),
                if i % 2 == 0 {
                    format!(r"C:\App_%\{i}")
                } else {
                    format!(r"HKCU\Software\App_%\{i}")
                },
                ChangeKind::Deleted,
            );
        }
        append(
            Some(session),
            r"C:\App_%\WrongKind".into(),
            ChangeKind::Created,
        );
        append(
            Some(session),
            r"C:\App_A\WildcardNotLiteral".into(),
            ChangeKind::Deleted,
        );
        append(None, r"C:\App_%\Unassigned".into(), ChangeKind::Deleted);
        let original = dir.path().join("ExorSystem changes.json");
        std::fs::write(&original, b"keep me").unwrap();
        let report = export_changes(
            &store,
            dir.path(),
            Some(session),
            Some("app_%"),
            Some(ChangeKind::Deleted),
        )
        .unwrap();
        assert_eq!(report.records, 1000);
        assert!(report.truncated);
        assert_eq!(std::fs::read(&original).unwrap(), b"keep me");
        assert!(report.path.ends_with("ExorSystem changes (2).json"));
        let json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(report.path).unwrap()).unwrap();
        assert_eq!(json["format"], "exorsystem.observations");
        assert_eq!(json["version"], 1);
        assert_eq!(
            json["filters"],
            serde_json::json!({ "session_id": session, "path_contains": "app_%", "change_kind": "deleted" })
        );
        assert_eq!(json["truncated"], true);
        let rows: Vec<LedgerEntry> = serde_json::from_value(json["observations"].clone()).unwrap();
        assert_eq!(rows.len(), 1000);
        assert_eq!(rows[0].entry.path, r"C:\App_%\1000");
        assert_eq!(rows[999].entry.path, r"HKCU\Software\App_%\1");
        assert!(
            rows.iter().all(|r| r.entry.session_id == Some(session)
                && r.entry.change_kind == ChangeKind::Deleted)
        );
        assert_eq!(
            store.ledger_recent(10, None).unwrap()[0].entry.path,
            r"C:\App_%\Unassigned"
        );
        let empty =
            export_changes(&store, dir.path(), Some(session), Some("missing"), None).unwrap();
        assert_eq!((empty.records, empty.truncated), (0, false));
        assert!(empty.path.ends_with("ExorSystem changes (3).json"));
    }

    #[test]
    fn unknown_session_and_policy_are_errors() {
        let store = Store::open_in_memory().unwrap();
        assert!(session_report(&store, 99, "standard").is_err());
        let id = store.begin_session("Foo", None, None).unwrap();
        assert!(session_report(&store, id, "nope").is_err());
        let r = session_report(&store, id, "standard").unwrap();
        assert!(r.starts_with("# Installation report: Foo"));
        assert!(r.contains("0 item(s)"));
    }
}
