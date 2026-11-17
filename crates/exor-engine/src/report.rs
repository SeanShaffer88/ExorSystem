//! Accessibility-first plain-language summaries: no colour, symbols or layout carry meaning.

use crate::error::{EngineError, Result};
use crate::policy::{self, SimulationReport};
use crate::templates;
use exor_storage::{PolicyAction, Provenance, Store};

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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod report_tests {
    use super::*;

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
