-- ExorSystem extensions required by the blueprint's "Change Ledger" and
-- "Evidence-Preserving Quarantine" pillars (not part of the 3.A schema).

CREATE TABLE change_ledger (
    entry_id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER,
    node_id INTEGER,
    path TEXT NOT NULL,
    change_kind TEXT NOT NULL CHECK(change_kind IN ('created','modified','deleted','renamed','quarantined','restored')),
    old_hash TEXT,
    new_hash TEXT,
    source TEXT NOT NULL CHECK(source IN ('usn_journal','registry_callback','snapshot_diff','manifest','policy')),
    observed_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX idx_ledger_session ON change_ledger(session_id);
CREATE INDEX idx_ledger_path ON change_ledger(path);

CREATE TRIGGER change_ledger_no_update BEFORE UPDATE ON change_ledger
BEGIN SELECT RAISE(ABORT, 'change_ledger is append-only'); END;
CREATE TRIGGER change_ledger_no_delete BEFORE DELETE ON change_ledger
BEGIN SELECT RAISE(ABORT, 'change_ledger is append-only'); END;

CREATE TABLE quarantine_manifests (
    quarantine_id TEXT PRIMARY KEY,
    rule_id INTEGER REFERENCES policy_rules(rule_id) ON DELETE SET NULL,
    session_id INTEGER REFERENCES installation_sessions(session_id) ON DELETE SET NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    status TEXT NOT NULL CHECK(status IN ('quarantined','restored','purged')),
    manifest_path TEXT NOT NULL,
    manifest_json TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_edges_unique ON node_relationships(parent_node_id, child_node_id, relationship_type);
CREATE INDEX idx_nodes_session ON system_nodes(first_detected_in);
