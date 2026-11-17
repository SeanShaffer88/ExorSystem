-- Blueprint schema, verbatim (section 3.A). The two PRAGMAs of the blueprint
-- (journal_mode = WAL, foreign_keys = ON) cannot run inside a migration
-- transaction; Store::open applies them on every connection instead.

CREATE TABLE installation_sessions (
    session_id INTEGER PRIMARY KEY AUTOINCREMENT,
    app_name TEXT NOT NULL,
    installer_path TEXT,
    fingerprint_family TEXT,
    started_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    ended_at TIMESTAMP,
    status TEXT CHECK(status IN ('monitoring', 'completed', 'failed'))
);

CREATE TABLE system_nodes (
    node_id INTEGER PRIMARY KEY AUTOINCREMENT,
    node_type TEXT CHECK(node_type IN ('file', 'registry_key', 'registry_value', 'service', 'task')),
    path TEXT UNIQUE NOT NULL,
    hash TEXT,
    acl_fingerprint TEXT,
    first_detected_in INTEGER REFERENCES installation_sessions(session_id)
);

CREATE TABLE node_relationships (
    edge_id INTEGER PRIMARY KEY AUTOINCREMENT,
    parent_node_id INTEGER REFERENCES system_nodes(node_id) ON DELETE CASCADE,
    child_node_id INTEGER REFERENCES system_nodes(node_id) ON DELETE CASCADE,
    relationship_type TEXT CHECK(relationship_type IN ('created_by', 'modified_by', 'depends_on', 'shared_with')),
    confidence_score REAL CHECK(confidence_score BETWEEN 0.0 AND 1.0),
    evidence_source TEXT
);

CREATE TABLE policy_rules (
    rule_id INTEGER PRIMARY KEY AUTOINCREMENT,
    rule_name TEXT NOT NULL,
    condition_json TEXT NOT NULL,
    action TEXT CHECK(action IN ('keep', 'quarantine', 'flag_for_review'))
);

CREATE INDEX idx_nodes_path ON system_nodes(path);
CREATE INDEX idx_edges_lookup ON node_relationships(parent_node_id, child_node_id);
