// Mirrors the serde output of exor-storage / exor-engine.
export type ChangeKind = "created" | "modified" | "deleted" | "renamed" | "quarantined" | "restored";
export type ChangeDomain = "file" | "registry" | "system";
export type PolicyAction = "keep" | "quarantine" | "flag_for_review";

export interface Session {
  session_id: number;
  app_name: string;
  installer_path: string | null;
  fingerprint_family: string | null;
  started_at: string | null;
  ended_at: string | null;
  status: "monitoring" | "completed" | "failed" | null;
}

export interface Edge {
  edge_id: number;
  parent_node_id: number;
  child_node_id: number;
  relationship_type: "created_by" | "modified_by" | "depends_on" | "shared_with";
  confidence_score: number;
  evidence_source: string | null;
}

export interface LedgerEntry {
  entry_id: number;
  entry: { path: string; change_kind: string; source: string; old_hash: string | null; new_hash: string | null };
  observed_at: string | null;
}

export interface ProvenanceView {
  found: boolean;
  summary: string;
  confidence: number;
  graph: RelationshipGraph | null;
  ledger: LedgerEntry[];
}

export interface GraphNode {
  id: number;
  path: string;
  kind: Decision["node_type"];
  session_name: string | null;
}

export interface RelationshipGraph {
  focus: GraphNode;
  links: { edge: Edge; node: GraphNode }[];
  total_edges: number;
}

export interface Decision {
  path: string;
  node_type: "file" | "registry_key" | "registry_value" | "service" | "task";
  action: PolicyAction;
  rule: [number, string] | null;
  reason: string;
}

export interface SimulationView {
  summary: string;
  keep: number;
  quarantine: number;
  flag_for_review: number;
  decisions: Decision[];
}

export interface ScanProgress {
  phase: "walking" | "hashing" | "done";
  files_seen: number;
  files_hashed: number;
  bytes_hashed: number;
}

export type ScanEvent =
  | { event: "progress"; data: ScanProgress }
  | { event: "done"; data: { files: number; skipped: number; root_digest: string } }
  | { event: "cancelled" }
  | { event: "failed"; data: { message: string } };

export interface QuarantineRow {
  quarantine_id: string;
  session_id: number | null;
  status: "in_progress" | "quarantined" | "restored" | "purged" | "rolled_back" | string;
  files: number;
  file_bytes: number | null;
}

export interface BatchPreview {
  status: QuarantineRow["status"];
  created_unix_ms: number;
  total_files: number;
  files: { original_path: string; size: number }[];
  next_offset: number | null;
}

export interface QuarantineResult {
  quarantine_id: string;
  moved: number;
  skipped: number;
  summary: string;
}

export interface WatchSystem {
  services: boolean;
  tasks: boolean;
}

export interface ScopeCoverage {
  requested: number;
  /** Usable baselines at start, successfully compared pairs at finish. */
  usable: number;
}

export type WatchCoverage = Record<"folders" | "registry" | "services" | "tasks", ScopeCoverage>;

export interface WatchStart {
  session_id: number;
  coverage: WatchCoverage;
  warnings: string[];
}

export interface WatchSummary extends WatchStart {
  files_changed: number;
  services_changed: number;
  tasks_changed: number;
  registry_keys_changed: number;
}

export interface WatchRun {
  app_name: string;
  ended_at: string | null;
  summary: WatchSummary;
}

export interface InstalledProgram {
  id: string;
  source: "registry" | "msix" | "portable";
  name: string;
  version: string | null;
  publisher: string | null;
  install_location: string | null;
  uninstall_command: string | null;
  quiet_uninstall_command: string | null;
  install_date: string | null;
  size_kb: number | null;
  scope: "machine" | "machine32" | "user";
  system_component: boolean;
  parent_key: string | null;
  windows_installer: boolean;
}

export interface ProgramRow {
  program: InstalledProgram;
  session_ids: number[];
  removed: boolean;
}

export interface ProgramInventory {
  programs: ProgramRow[];
  warnings: string[];
}

export interface PortableProgram {
  name: string;
  executable: string;
}

export interface SessionItem {
  path: string;
  node_type: string;
  confidence: number;
  shared: boolean;
}

export interface SessionItems {
  total: number;
  matching: number;
  items: SessionItem[];
}

export interface LeftoverFolder {
  path: string;
  reason: string;
  confidence: number;
  can_quarantine: boolean;
}

export interface LeftoverReview {
  folders: LeftoverFolder[];
  system_entries: {
    kind: "registry_key" | "service" | "task";
    path: string;
    reason: string;
  }[];
  warnings: string[];
}
export interface UninstallPreview {
  executable: string;
  args: string[];
}

export interface UninstallRun {
  exit_code: number | null;
  summary: string;
}

export interface HistoryRow {
  id: number;
  when: string | null;
  path: string;
  what: string;
  source: string;
  session_id: number | null;
  domain: ChangeDomain;
  old_hash: string | null;
  new_hash: string | null;
}

export interface HistoryPage {
  rows: HistoryRow[];
  next_before_id: number | null;
}

export interface ChangeExport {
  path: string;
  records: number;
  truncated: boolean;
}

export interface PortableView {
  folder: string;
  copied: number;
  missing: number;
  registry_keys: number;
  unsupported: number;
  summary: string;
}

export interface RiskRow {
  name: string;
  why: string;
}
export interface RiskView {
  low: RiskRow[];
  medium: RiskRow[];
  high: RiskRow[];
}

export interface TemplateInfo {
  name: string;
  description: string;
}
export interface AppInfo {
  version: string;
  data_folder: string;
  quarantine_folder: string;
  legacy_data_folder: string | null;
  recovery_warnings: string[];
}
