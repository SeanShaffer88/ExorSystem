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
