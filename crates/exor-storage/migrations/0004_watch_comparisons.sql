-- Comparison coverage is not derivable from change_ledger: a compared scope can have zero
-- changes, and an unreadable scope never contributes observations. Do not backfill old sessions.
CREATE TABLE watch_comparisons (
    session_id INTEGER PRIMARY KEY REFERENCES installation_sessions(session_id) ON DELETE CASCADE,
    summary_json TEXT NOT NULL CHECK(json_valid(summary_json))
);
