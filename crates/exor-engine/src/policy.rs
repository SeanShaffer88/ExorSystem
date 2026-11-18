//! Cleanup policy simulator: pure dry-run evaluation of JSONLogic rules against candidates.

use crate::error::Result;
use crate::jsonlogic::{apply, truthy};
use exor_storage::{Node, NodeType, PolicyAction, PolicyRule, RelationshipType, Store};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// A proposed cleanup target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Candidate {
    /// Path / key.
    pub path: String,
    /// Kind.
    pub node_type: NodeType,
    /// Extra attributes visible to rules (`confidence`, `shared`, `session_id`, ...).
    pub attrs: Map<String, Value>,
}

impl Candidate {
    /// Evaluation context: `attrs` plus `path` and `node_type`.
    pub fn data(&self) -> Value {
        let mut m = self.attrs.clone();
        m.insert("path".into(), Value::String(self.path.clone()));
        m.insert(
            "node_type".into(),
            Value::String(self.node_type.as_str().into()),
        );
        Value::Object(m)
    }
}

/// Outcome for one candidate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Decision {
    /// Path / key.
    pub path: String,
    /// Kind; only file decisions can be executed.
    pub node_type: NodeType,
    /// Final action.
    pub action: PolicyAction,
    /// Deciding rule id and name, if any rule matched.
    pub rule: Option<(i64, String)>,
    /// Human-readable reason.
    pub reason: String,
}

/// Whole simulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationReport {
    /// One decision per candidate, input order.
    pub decisions: Vec<Decision>,
    /// Count of `keep`.
    pub keep: usize,
    /// Count of `quarantine`.
    pub quarantine: usize,
    /// Count of `flag_for_review`.
    pub flag_for_review: usize,
}

fn protectiveness(a: PolicyAction) -> u8 {
    match a {
        PolicyAction::Keep => 0,
        PolicyAction::FlagForReview => 1,
        PolicyAction::Quarantine => 2,
    }
}

/// Evaluate every rule against every candidate. No side effects.
///
/// When several rules match, the most protective wins (keep, then flag_for_review, then
/// quarantine); ties go to the lowest rule id. No match means `flag_for_review`. Only files
/// can be quarantined, regardless of the rules.
pub fn simulate(rules: &[PolicyRule], candidates: &[Candidate]) -> Result<SimulationReport> {
    let mut decisions = Vec::with_capacity(candidates.len());
    for c in candidates {
        let data = c.data();
        let mut best: Option<&PolicyRule> = None;
        for r in rules {
            if truthy(&apply(&r.condition, &data)?) {
                best = match best {
                    Some(b)
                        if (protectiveness(b.action), b.rule_id)
                            <= (protectiveness(r.action), r.rule_id) =>
                    {
                        Some(b)
                    }
                    _ => Some(r),
                };
            }
        }
        let (action, rule, reason) = match best {
            Some(r) if r.action == PolicyAction::Quarantine && c.node_type != NodeType::File => (
                PolicyAction::FlagForReview,
                None,
                "system items are review-only".into(),
            ),
            Some(r) => (
                r.action,
                Some((r.rule_id, r.rule_name.clone())),
                format!("matched rule '{}'", r.rule_name),
            ),
            None => (PolicyAction::FlagForReview, None, "no rule matched".into()),
        };
        decisions.push(Decision {
            path: c.path.clone(),
            node_type: c.node_type,
            action,
            rule,
            reason,
        });
    }
    let count = |a| decisions.iter().filter(|d| d.action == a).count();
    Ok(SimulationReport {
        keep: count(PolicyAction::Keep),
        quarantine: count(PolicyAction::Quarantine),
        flag_for_review: count(PolicyAction::FlagForReview),
        decisions,
    })
}

pub(crate) fn candidate_from_node(store: &Store, n: &Node) -> Result<Candidate> {
    let origins = store.edges_as_child(n.node_id)?;
    let confidence = origins
        .iter()
        .filter(|e| e.relationship_type == RelationshipType::CreatedBy)
        .map(|e| e.confidence_score)
        .fold(0.3_f64, f64::max);
    let impact = store.shared_component_impact(n.node_id)?;
    let has_shared_edge = origins
        .iter()
        .any(|e| e.relationship_type == RelationshipType::SharedWith)
        || store
            .edges_as_parent(n.node_id)?
            .iter()
            .any(|e| e.relationship_type == RelationshipType::SharedWith);
    let used_elsewhere = impact
        .dependents
        .iter()
        .any(|d| d.first_detected_in != n.first_detected_in);
    let ext = n
        .path
        .rsplit_once('.')
        .map(|(_, e)| e.to_lowercase())
        .unwrap_or_default();
    let attrs = json!({
        "confidence": confidence,
        "shared": has_shared_edge || used_elsewhere,
        "dependents": impact.dependents.len(),
        "session_id": n.first_detected_in,
        "has_hash": n.hash.is_some(),
        "ext": ext,
    });
    let Value::Object(attrs) = attrs else {
        return Ok(Candidate {
            path: n.path.clone(),
            node_type: n.node_type,
            attrs: Map::new(),
        });
    };
    Ok(Candidate {
        path: n.path.clone(),
        node_type: n.node_type,
        attrs,
    })
}

/// Build candidates for every node first detected in `session_id`.
pub fn candidates_for_session(store: &Store, session_id: i64) -> Result<Vec<Candidate>> {
    store
        .nodes_for_session(session_id)?
        .iter()
        .map(|n| candidate_from_node(store, n))
        .collect()
}
