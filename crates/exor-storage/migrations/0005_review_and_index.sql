-- Rebuildable read index; the original manifest remains authority for disk operations.
CREATE TABLE quarantine_batch_index (
    quarantine_id TEXT PRIMARY KEY REFERENCES quarantine_manifests(quarantine_id),
    created_unix_ms INTEGER NOT NULL
);
CREATE TABLE quarantine_file_index (
    quarantine_id TEXT NOT NULL REFERENCES quarantine_manifests(quarantine_id),
    ordinal INTEGER NOT NULL,
    original_path TEXT NOT NULL,
    size INTEGER NOT NULL CHECK(size >= 0),
    PRIMARY KEY (quarantine_id, ordinal)
) WITHOUT ROWID;
CREATE TABLE quarantine_operations (
    quarantine_id TEXT PRIMARY KEY REFERENCES quarantine_manifests(quarantine_id),
    operation TEXT NOT NULL CHECK(operation IN ('restoring','purging'))
);
CREATE INDEX idx_ledger_observed_id ON change_ledger(observed_at, entry_id);
CREATE INDEX idx_ledger_session_path_id ON change_ledger(session_id, path, entry_id);
CREATE TABLE application_sessions (
    session_id INTEGER PRIMARY KEY REFERENCES installation_sessions(session_id),
    program_id TEXT NOT NULL,
    program_label TEXT NOT NULL
);
CREATE INDEX idx_application_sessions_program ON application_sessions(program_id, session_id);

-- Old or corrupted manifests must not invent an empty successful batch. Index only recognizable
-- v1 objects, and leave malformed records to the explicit recovery/inspection path.
CREATE VIEW indexed_batch_input AS
SELECT quarantine_id, CASE WHEN json_valid(manifest_json) THEN manifest_json ELSE '{}' END AS payload
FROM quarantine_manifests;
INSERT INTO quarantine_batch_index
SELECT quarantine_id, json_extract(payload, '$.created_unix_ms') FROM indexed_batch_input
WHERE json_extract(payload, '$.version') = 1
  AND json_extract(payload, '$.quarantine_id') = quarantine_id
  AND json_type(payload, '$.created_unix_ms') = 'integer'
  AND json_type(payload, '$.items') = 'array';
INSERT INTO quarantine_file_index
SELECT q.quarantine_id, CAST(j.key AS INTEGER), json_extract(j.value, '$.original_path'), json_extract(j.value, '$.size')
FROM indexed_batch_input q JOIN quarantine_batch_index b USING(quarantine_id), json_each(q.payload, '$.items') j
WHERE j.type = 'object' AND json_extract(j.value, '$.type') = 'file'
  AND json_type(j.value, '$.original_path') = 'text' AND json_type(j.value, '$.size') = 'integer'
  AND json_extract(j.value, '$.size') >= 0;
DROP VIEW indexed_batch_input;
