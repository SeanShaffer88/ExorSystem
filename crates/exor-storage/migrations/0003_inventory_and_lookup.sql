-- An opt-in list of portable apps, not a source of executable commands.
CREATE TABLE portable_programs (
    executable TEXT PRIMARY KEY COLLATE NOCASE NOT NULL,
    name TEXT NOT NULL CHECK(length(trim(name)) > 0)
);

-- Only read-side lookup is case-insensitive. Preserve existing path identity and evidence.
CREATE INDEX idx_nodes_path_nocase ON system_nodes(path COLLATE NOCASE);
CREATE INDEX idx_ledger_path_nocase ON change_ledger(path COLLATE NOCASE);
