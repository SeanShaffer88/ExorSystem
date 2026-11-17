//! Accessibility-first plain-language summaries: no colour, symbols or layout carry meaning.

use crate::policy::SimulationReport;
use exor_storage::{PolicyAction, Provenance};

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

/// Plain-language provenance sentence for one artifact.
pub fn describe_provenance(p: &Provenance) -> String {
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
