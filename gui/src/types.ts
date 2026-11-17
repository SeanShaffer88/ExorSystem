// Mirrors the serde output of exor-storage / exor-engine.
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
  origins: Edge[];
  ledger: LedgerEntry[];
}

export interface Decision {
  path: string;
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
}

export interface QuarantineResult {
  quarantine_id: string;
  moved: number;
  skipped: number;
  summary: string;
}

export interface WatchSummary {
  session_id: number;
  files_changed: number;
  services_changed: number;
  tasks_changed: number;
  warnings: string[];
  summary: string;
}

export interface InstalledProgram {
  id: string;
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
}

export interface SessionItem {
  path: string;
  node_type: string;
  confidence: number;
  shared: boolean;
}

export interface SessionItems {
  total: number;
  shared: number;
  by_type: Record<string, number>;
  items: SessionItem[];
}

export interface Leftover {
  path: string;
  reason: string;
  confidence: number;
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
  session_id: number | null;
}
