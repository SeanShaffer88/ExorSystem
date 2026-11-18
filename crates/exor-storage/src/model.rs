//! Typed rows and closed-domain enums mirroring the schema CHECK constraints.

use serde::{Deserialize, Serialize};

macro_rules! str_enum {
    ($(#[$m:meta])* $name:ident { $($var:ident => $s:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub enum $name { $( #[serde(rename = $s)] #[allow(missing_docs)] $var ),+ }
        impl $name {
            /// Database text form.
            pub const fn as_str(self) -> &'static str {
                match self { $(Self::$var => $s),+ }
            }
        }
        impl std::str::FromStr for $name {
            type Err = $crate::StorageError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s {
                    $($s => Ok(Self::$var),)+
                    other => Err($crate::StorageError::InvalidValue {
                        field: stringify!($name),
                        value: other.to_string(),
                    }),
                }
            }
        }
    };
}

str_enum! {
    /// Kind of system artifact.
    NodeType { File => "file", RegistryKey => "registry_key", RegistryValue => "registry_value", Service => "service", Task => "task" }
}
str_enum! {
    /// Edge semantics. Read as `child <type> parent` (artifact `created_by` installer, app `depends_on` library).
    RelationshipType { CreatedBy => "created_by", ModifiedBy => "modified_by", DependsOn => "depends_on", SharedWith => "shared_with" }
}
str_enum! {
    /// Session lifecycle.
    SessionStatus { Monitoring => "monitoring", Completed => "completed", Failed => "failed" }
}
str_enum! {
    /// Cleanup policy outcome.
    PolicyAction { Keep => "keep", Quarantine => "quarantine", FlagForReview => "flag_for_review" }
}
str_enum! {
    /// Ledger change kind.
    ChangeKind { Created => "created", Modified => "modified", Deleted => "deleted", Renamed => "renamed", Quarantined => "quarantined", Restored => "restored" }
}
str_enum! {
    /// Domain of a recorded observation. Does not describe whether it is safe to remove.
    ChangeDomain { File => "file", Registry => "registry", System => "system" }
}
str_enum! {
    /// Ledger evidence origin.
    ChangeSource { UsnJournal => "usn_journal", RegistryCallback => "registry_callback", SnapshotDiff => "snapshot_diff", Manifest => "manifest", Policy => "policy" }
}
str_enum! {
    /// Quarantine lifecycle.
    QuarantineStatus { Quarantined => "quarantined", Restored => "restored", Purged => "purged" }
}

/// Probabilistic attribution evidence with the blueprint's fixed scores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Evidence {
    /// Confirmed via a manifest (1.0).
    Manifest,
    /// Inferred from temporal correlation with a monitored session (0.7).
    Temporal,
    /// Heuristic / unknown (0.3).
    Heuristic,
}

impl Evidence {
    /// Confidence score.
    pub const fn score(self) -> f64 {
        match self {
            Self::Manifest => 1.0,
            Self::Temporal => 0.7,
            Self::Heuristic => 0.3,
        }
    }
    /// Stored `evidence_source` label.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Manifest => "manifest",
            Self::Temporal => "temporal",
            Self::Heuristic => "heuristic",
        }
    }
}

/// Installation session row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Session {
    /// Primary key.
    pub session_id: i64,
    /// Application name.
    pub app_name: String,
    /// Installer path.
    pub installer_path: Option<String>,
    /// Installer family fingerprint.
    pub fingerprint_family: Option<String>,
    /// Start timestamp (UTC text).
    pub started_at: Option<String>,
    /// End timestamp.
    pub ended_at: Option<String>,
    /// Status.
    pub status: Option<SessionStatus>,
}

/// Scope counts for one source. A skipped source is different from an opted-out source.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScopeCoverage {
    /// Number of folders or subtrees requested, or 0/1 for each system probe.
    pub requested: usize,
    /// Usable baselines at start; successfully compared snapshot pairs at finish.
    pub usable: usize,
}

/// Baseline or comparison coverage, independent of how many changes were observed.
#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct WatchCoverage {
    /// Folder scopes.
    pub folders: ScopeCoverage,
    /// Registry subtree scopes.
    pub registry: ScopeCoverage,
    /// Service listing scope.
    pub services: ScopeCoverage,
    /// Scheduled-task scope.
    pub tasks: ScopeCoverage,
}

impl WatchCoverage {
    /// Whether at least one scope has evidence at this stage.
    pub fn has_usable(&self) -> bool {
        [self.folders, self.registry, self.services, self.tasks]
            .iter()
            .any(|scope| scope.usable > 0)
    }
}

/// Saved result of a Watch comparison. Counts are only meaningful for compared scopes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WatchSummary {
    /// Session that now holds the evidence, or a failed session if there were no comparable scopes.
    pub session_id: i64,
    /// Successfully compared scopes out of those requested.
    pub coverage: WatchCoverage,
    /// Files added, changed or removed in comparable watched folders.
    pub files_changed: usize,
    /// Service changes recorded.
    pub services_changed: usize,
    /// Scheduled-task changes recorded.
    pub tasks_changed: usize,
    /// Registry key changes recorded.
    pub registry_keys_changed: usize,
    /// Reasons for skipped scopes at either end of the watch.
    pub warnings: Vec<String>,
}

/// A recent saved comparison and its display context. Only sessions with actual comparisons enter
/// this list, including zero-change runs and failed comparisons with no comparable scopes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WatchRun {
    /// Name entered for the session.
    pub app_name: String,
    /// Time the comparison was saved, as UTC text.
    pub ended_at: Option<String>,
    /// Saved coverage and counts.
    pub summary: WatchSummary,
}

/// A portable application a user chose to list in the inventory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortableProgram {
    /// The display name supplied by the user.
    pub name: String,
    /// Absolute path of the executable. ExorSystem never launches it.
    pub executable: String,
}

/// Node to insert or update.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewNode {
    /// Kind.
    pub node_type: NodeType,
    /// Unique path / key / name.
    pub path: String,
    /// BLAKE3 hex.
    pub hash: Option<String>,
    /// ACL fingerprint.
    pub acl_fingerprint: Option<String>,
    /// Session that first saw it.
    pub first_detected_in: Option<i64>,
}

/// Stored node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Node {
    /// Primary key.
    pub node_id: i64,
    /// Kind.
    pub node_type: NodeType,
    /// Path.
    pub path: String,
    /// BLAKE3 hex.
    pub hash: Option<String>,
    /// ACL fingerprint.
    pub acl_fingerprint: Option<String>,
    /// First session.
    pub first_detected_in: Option<i64>,
}

/// Stored edge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    /// Primary key.
    pub edge_id: i64,
    /// Origin / dependency / creator.
    pub parent_node_id: i64,
    /// Derived / dependent / artifact.
    pub child_node_id: i64,
    /// Semantics.
    pub relationship_type: RelationshipType,
    /// 0.0..=1.0.
    pub confidence_score: f64,
    /// Evidence label.
    pub evidence_source: Option<String>,
}

/// Stored policy rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolicyRule {
    /// Primary key.
    pub rule_id: i64,
    /// Name.
    pub rule_name: String,
    /// JSONLogic condition.
    pub condition: serde_json::Value,
    /// Outcome when the condition is truthy.
    pub action: PolicyAction,
}

/// Ledger entry to append.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NewLedgerEntry {
    /// Session.
    pub session_id: Option<i64>,
    /// Node, if known.
    pub node_id: Option<i64>,
    /// Path (kept even if the node is later gone).
    pub path: String,
    /// What happened.
    pub change_kind: ChangeKind,
    /// Previous hash.
    pub old_hash: Option<String>,
    /// New hash.
    pub new_hash: Option<String>,
    /// Origin.
    pub source: ChangeSource,
}

/// Stored ledger entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LedgerEntry {
    /// Primary key.
    pub entry_id: i64,
    /// Fields as appended.
    pub entry: NewLedgerEntry,
    /// Observation time (UTC text).
    pub observed_at: Option<String>,
    /// Recorded node kind, or path/source classification for observations without a node.
    pub domain: ChangeDomain,
}

/// A bounded page of changes in descending entry order.
#[derive(Debug, Clone, PartialEq)]
pub struct LedgerPage {
    /// Matching changes, including evidence without a current node.
    pub entries: Vec<LedgerEntry>,
    /// Exclusive cursor for the next page, only present if older matches exist.
    pub next_before_id: Option<i64>,
}

/// Filters for one bounded page of the change ledger. All filters apply before pagination.
#[derive(Debug, Default, Clone, Copy)]
pub struct LedgerQuery<'a> {
    /// Literal substring of the recorded path.
    pub path_contains: Option<&'a str>,
    /// Installation to include, or all (including unassigned observations).
    pub session_id: Option<i64>,
    /// Exclusive newest bound for older pages.
    pub before_id: Option<i64>,
    /// One evidence domain, or all domains.
    pub domain: Option<ChangeDomain>,
    /// One observation kind, or all kinds.
    pub change_kind: Option<ChangeKind>,
}

/// A bounded, path-ordered page of nodes first detected in one session.
#[derive(Debug)]
pub struct SessionNodePage {
    /// Number of nodes first detected in the session, before filtering.
    pub total: usize,
    /// Number of nodes matching the literal path filter.
    pub matching: usize,
    /// Nodes on the requested page.
    pub nodes: Vec<Node>,
}

/// Stored quarantine record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuarantineRecord {
    /// Opaque id.
    pub quarantine_id: String,
    /// Rule that triggered it.
    pub rule_id: Option<i64>,
    /// Session.
    pub session_id: Option<i64>,
    /// Status.
    pub status: QuarantineStatus,
    /// Manifest file on disk.
    pub manifest_path: String,
    /// Manifest content copy.
    pub manifest_json: String,
}

/// Full trace of one artifact back to its installation session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    /// The artifact.
    pub node: Node,
    /// Installation session that first detected it.
    pub session: Option<Session>,
    /// Edges where the node is the child (its origins/dependencies).
    pub origins: Vec<Edge>,
    /// Edges where the node is the parent (things derived from / depending on it).
    pub dependents: Vec<Edge>,
    /// Ledger history, oldest first.
    pub ledger: Vec<LedgerEntry>,
    /// Best `created_by` confidence; 0.3 when no attribution edge exists.
    pub confidence: f64,
}

/// Who else relies on a node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedImpact {
    /// The node.
    pub node: Node,
    /// Dependents (via `depends_on` / `shared_with`).
    pub dependents: Vec<Node>,
    /// Distinct sessions involved (node's own plus dependents').
    pub sessions: Vec<i64>,
}
