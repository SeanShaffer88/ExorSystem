//! Update batching by risk.

use crate::error::Result;
use crate::policy;
use exor_storage::{NodeType, Store};
use serde::{Deserialize, Serialize};

/// Risk tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RiskLevel {
    /// Safe to batch automatically.
    Low,
    /// Review before applying.
    Medium,
    /// Apply alone, with a restore point.
    High,
}

/// A pending update with the facts that drive its risk.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UpdateCandidate {
    /// Application name.
    pub name: String,
    /// Other sessions/apps relying on its components.
    pub shared_dependents: usize,
    /// Whether it installs or changes services/drivers.
    pub touches_services: bool,
    /// Lowest attribution confidence among its components.
    pub min_confidence: f64,
}

/// Classify one candidate.
pub fn assess(c: &UpdateCandidate) -> RiskLevel {
    if (c.touches_services && c.shared_dependents > 0) || c.shared_dependents >= 5 {
        RiskLevel::High
    } else if c.touches_services || c.shared_dependents > 0 || c.min_confidence < 0.7 {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    }
}

/// Batches ordered for execution: low first.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RiskBatches {
    /// Low risk.
    pub low: Vec<UpdateCandidate>,
    /// Medium risk.
    pub medium: Vec<UpdateCandidate>,
    /// High risk.
    pub high: Vec<UpdateCandidate>,
}

/// Split updates into risk batches.
pub fn batch_by_risk(updates: Vec<UpdateCandidate>) -> RiskBatches {
    let mut b = RiskBatches::default();
    for u in updates {
        match assess(&u) {
            RiskLevel::Low => b.low.push(u),
            RiskLevel::Medium => b.medium.push(u),
            RiskLevel::High => b.high.push(u),
        }
    }
    b
}

/// Plain-language reason for a candidate's tier.
pub fn explain(c: &UpdateCandidate) -> String {
    let mut why = Vec::new();
    if c.touches_services {
        why.push("it installs or changes a service".to_string());
    }
    if c.shared_dependents > 0 {
        why.push(format!(
            "{} of its items may be shared with other programs",
            c.shared_dependents
        ));
    }
    if c.min_confidence < 0.7 {
        why.push("some of its items are only guesses".to_string());
    }
    if why.is_empty() {
        "Everything about it is confirmed or very likely, and nothing is shared.".to_string()
    } else {
        format!("Because {}.", why.join(", and "))
    }
}

/// One candidate per recorded installation, built from what was traced to it.
pub fn candidates_from_store(store: &Store) -> Result<Vec<UpdateCandidate>> {
    let mut out = Vec::new();
    for s in store.sessions()? {
        let cands = policy::candidates_for_session(store, s.session_id)?;
        let mut shared = 0;
        let mut min_conf: f64 = 1.0;
        let mut services = false;
        for c in &cands {
            if c.attrs
                .get("shared")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
            {
                shared += 1;
            }
            min_conf = min_conf.min(
                c.attrs
                    .get("confidence")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.3),
            );
            services |= matches!(c.node_type, NodeType::Service);
        }
        out.push(UpdateCandidate {
            name: s.app_name,
            shared_dependents: shared,
            touches_services: services,
            min_confidence: min_conf,
        });
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn tiers_and_reasons() {
        let quiet = UpdateCandidate {
            name: "A".into(),
            shared_dependents: 0,
            touches_services: false,
            min_confidence: 1.0,
        };
        let risky = UpdateCandidate {
            name: "B".into(),
            shared_dependents: 2,
            touches_services: true,
            min_confidence: 0.3,
        };
        assert_eq!(assess(&quiet), RiskLevel::Low);
        assert_eq!(assess(&risky), RiskLevel::High);
        assert!(explain(&quiet).contains("nothing is shared"));
        let e = explain(&risky);
        assert!(e.contains("service") && e.contains("2 of its items") && e.contains("guesses"));
    }

    #[test]
    fn empty_store_has_no_candidates_and_new_session_is_low_risk() {
        let store = Store::open_in_memory().unwrap();
        assert!(candidates_from_store(&store).unwrap().is_empty());
        store.begin_session("Foo", None, None).unwrap();
        let c = candidates_from_store(&store).unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(assess(&c[0]), RiskLevel::Low);
    }
}
