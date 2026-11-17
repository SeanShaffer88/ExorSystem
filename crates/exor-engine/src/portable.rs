//! Portable-app transition assistant: what can move into a self-contained folder, what cannot.

use exor_storage::{Node, NodeType};
use serde::{Deserialize, Serialize};

/// Plan for turning an installed app into a portable one.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortablePlan {
    /// Files to copy beneath the portable app folder.
    pub copy_files: Vec<String>,
    /// Registry keys/values to export and replay at launch.
    pub export_registry: Vec<String>,
    /// Items that cannot be made portable (services, scheduled tasks), with the reason.
    pub unsupported: Vec<(String, String)>,
}

/// Build a plan from the nodes attributed to a session.
pub fn plan_portable(nodes: &[Node]) -> PortablePlan {
    let mut p = PortablePlan::default();
    for n in nodes {
        match n.node_type {
            NodeType::File => p.copy_files.push(n.path.clone()),
            NodeType::RegistryKey | NodeType::RegistryValue => {
                p.export_registry.push(n.path.clone())
            }
            NodeType::Service => p.unsupported.push((
                n.path.clone(),
                "services need installation and administrator rights".into(),
            )),
            NodeType::Task => p
                .unsupported
                .push((n.path.clone(), "scheduled tasks are system-wide".into())),
        }
    }
    p
}
