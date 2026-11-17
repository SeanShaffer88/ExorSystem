//! Built-in policy templates.

use crate::error::Result;
use exor_storage::{PolicyAction, Store};
use serde_json::{Value, json};

/// A named bundle of rules.
#[derive(Debug, Clone)]
pub struct RuleTemplate {
    /// Template name.
    pub name: &'static str,
    /// Plain-language description.
    pub description: &'static str,
    /// `(rule name, JSONLogic, action)`.
    pub rules: Vec<(&'static str, Value, PolicyAction)>,
}

fn keep_shared() -> (&'static str, Value, PolicyAction) {
    (
        "keep shared components",
        json!({"!!": [{"var": "shared"}]}),
        PolicyAction::Keep,
    )
}
fn keep_windows() -> (&'static str, Value, PolicyAction) {
    (
        "keep Windows system paths",
        json!({"starts_with": [{"var": "path"}, "C:\\Windows\\"]}),
        PolicyAction::Keep,
    )
}

/// All templates.
pub fn templates() -> Vec<RuleTemplate> {
    vec![
        RuleTemplate {
            name: "conservative",
            description: "Only quarantine files confirmed by a manifest; everything uncertain goes to review.",
            rules: vec![
                keep_shared(),
                keep_windows(),
                (
                    "review uncertain items",
                    json!({"<": [{"var": "confidence"}, 0.7]}),
                    PolicyAction::FlagForReview,
                ),
                (
                    "quarantine confirmed files",
                    json!({"and": [{"==": [{"var": "node_type"}, "file"]}, {">=": [{"var": "confidence"}, 1.0]}]}),
                    PolicyAction::Quarantine,
                ),
            ],
        },
        RuleTemplate {
            name: "standard",
            description: "Quarantine items inferred or confirmed (confidence 0.7+); review the rest.",
            rules: vec![
                keep_shared(),
                keep_windows(),
                (
                    "review uncertain items",
                    json!({"<": [{"var": "confidence"}, 0.7]}),
                    PolicyAction::FlagForReview,
                ),
                (
                    "quarantine attributed items",
                    json!({">=": [{"var": "confidence"}, 0.7]}),
                    PolicyAction::Quarantine,
                ),
            ],
        },
        RuleTemplate {
            name: "thorough",
            description: "Quarantine everything attributed to the session, even by heuristic; shared items stay.",
            rules: vec![
                keep_shared(),
                keep_windows(),
                (
                    "quarantine all attributed items",
                    json!({">=": [{"var": "confidence"}, 0.3]}),
                    PolicyAction::Quarantine,
                ),
            ],
        },
    ]
}

/// Install a template's rules into the store; returns new rule ids.
pub fn install(store: &Store, name: &str) -> Result<Vec<i64>> {
    let Some(t) = templates().into_iter().find(|t| t.name == name) else {
        return Err(crate::EngineError::Policy(format!(
            "unknown template '{name}'"
        )));
    };
    t.rules
        .iter()
        .map(|(n, c, a)| store.add_rule(n, c, *a).map_err(Into::into))
        .collect()
}

/// Template rules as in-memory `PolicyRule`s (negative ids, nothing persisted) for dry runs.
pub fn rules_for(name: &str) -> Option<Vec<exor_storage::PolicyRule>> {
    let t = templates().into_iter().find(|t| t.name == name)?;
    Some(
        t.rules
            .into_iter()
            .zip(1i64..)
            .map(|((n, c, a), i)| exor_storage::PolicyRule {
                rule_id: -i,
                rule_name: n.to_string(),
                condition: c,
                action: a,
            })
            .collect(),
    )
}
