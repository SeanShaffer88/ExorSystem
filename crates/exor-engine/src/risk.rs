//! Update batching by risk.

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
