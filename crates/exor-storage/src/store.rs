//! `Store`: connection handling plus the query handlers.

use crate::error::{Result, StorageError};
use crate::migrate;
use crate::model::*;
use rusqlite::{Connection, OptionalExtension, Row, params};
use std::path::{Path, PathBuf};
use std::str::FromStr;

fn col<T: FromStr<Err = StorageError>>(row: &Row<'_>, idx: usize) -> rusqlite::Result<T> {
    let s: String = row.get(idx)?;
    T::from_str(&s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn opt_col<T: FromStr<Err = StorageError>>(
    row: &Row<'_>,
    idx: usize,
) -> rusqlite::Result<Option<T>> {
    let s: Option<String> = row.get(idx)?;
    s.map(|s| {
        T::from_str(&s).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(idx, rusqlite::types::Type::Text, Box::new(e))
        })
    })
    .transpose()
}

const NODE_COLS: &str = "node_id, node_type, path, hash, acl_fingerprint, first_detected_in";
const EDGE_COLS: &str =
    "edge_id, parent_node_id, child_node_id, relationship_type, confidence_score, evidence_source";
const SESSION_COLS: &str =
    "session_id, app_name, installer_path, fingerprint_family, started_at, ended_at, status";
const LEDGER_COLS: &str =
    "entry_id, session_id, node_id, path, change_kind, old_hash, new_hash, source, observed_at";

fn node_row(r: &Row<'_>) -> rusqlite::Result<Node> {
    Ok(Node {
        node_id: r.get(0)?,
        node_type: col(r, 1)?,
        path: r.get(2)?,
        hash: r.get(3)?,
        acl_fingerprint: r.get(4)?,
        first_detected_in: r.get(5)?,
    })
}
fn edge_row(r: &Row<'_>) -> rusqlite::Result<Edge> {
    Ok(Edge {
        edge_id: r.get(0)?,
        parent_node_id: r.get(1)?,
        child_node_id: r.get(2)?,
        relationship_type: col(r, 3)?,
        confidence_score: r.get(4)?,
        evidence_source: r.get(5)?,
    })
}
fn session_row(r: &Row<'_>) -> rusqlite::Result<Session> {
    Ok(Session {
        session_id: r.get(0)?,
        app_name: r.get(1)?,
        installer_path: r.get(2)?,
        fingerprint_family: r.get(3)?,
        started_at: r.get(4)?,
        ended_at: r.get(5)?,
        status: opt_col(r, 6)?,
    })
}
fn ledger_row(r: &Row<'_>) -> rusqlite::Result<LedgerEntry> {
    Ok(LedgerEntry {
        entry_id: r.get(0)?,
        entry: NewLedgerEntry {
            session_id: r.get(1)?,
            node_id: r.get(2)?,
            path: r.get(3)?,
            change_kind: col(r, 4)?,
            old_hash: r.get(5)?,
            new_hash: r.get(6)?,
            source: col(r, 7)?,
        },
        observed_at: r.get(8)?,
    })
}
fn rule_row(r: &Row<'_>) -> rusqlite::Result<PolicyRule> {
    let json: String = r.get(2)?;
    let condition = serde_json::from_str(&json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
    })?;
    Ok(PolicyRule {
        rule_id: r.get(0)?,
        rule_name: r.get(1)?,
        condition,
        action: col(r, 3)?,
    })
}

/// Database handle. Not `Sync`; wrap in a `Mutex` or use one per thread.
pub struct Store {
    conn: Connection,
}

impl Store {
    fn configure(conn: &Connection) -> Result<()> {
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA synchronous = NORMAL;
             PRAGMA busy_timeout = 5000;",
        )?;
        Ok(())
    }

    /// Open (creating if needed) a file database, migrate it, and keep a
    /// `<db>.pre-v<N>.bak` copy first when an existing database must be upgraded.
    pub fn open(path: &Path) -> Result<Self> {
        let mut conn = Connection::open(path)?;
        Self::configure(&conn)?;
        let current = migrate::current_version(&conn)?;
        if current > 0 && current < migrate::latest_version() {
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
            let mut bak: PathBuf = path.as_os_str().to_owned().into();
            bak.set_extension(format!("pre-v{current}.bak"));
            std::fs::copy(path, bak)?;
        }
        migrate::migrate(&mut conn)?;
        Ok(Self { conn })
    }

    /// In-memory database (tests, simulations).
    pub fn open_in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        Self::configure(&conn)?;
        migrate::migrate(&mut conn)?;
        Ok(Self { conn })
    }

    /// Applied schema version.
    pub fn schema_version(&self) -> Result<u32> {
        migrate::current_version(&self.conn)
    }

    /// Run `PRAGMA wal_checkpoint(TRUNCATE)`.
    pub fn checkpoint(&self) -> Result<()> {
        self.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }

    // ---- sessions ----

    /// Start a monitoring session.
    pub fn begin_session(
        &self,
        app_name: &str,
        installer_path: Option<&str>,
        fingerprint_family: Option<&str>,
    ) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO installation_sessions(app_name, installer_path, fingerprint_family, status)
             VALUES (?1, ?2, ?3, 'monitoring')",
            params![app_name, installer_path, fingerprint_family],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Finish a session as completed or failed.
    pub fn end_session(&self, session_id: i64, status: SessionStatus) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE installation_sessions SET ended_at = CURRENT_TIMESTAMP, status = ?2 WHERE session_id = ?1",
            params![session_id, status.as_str()],
        )?;
        if n == 0 {
            return Err(StorageError::NotFound(format!("session {session_id}")));
        }
        Ok(())
    }

    /// Fetch one session.
    pub fn session(&self, session_id: i64) -> Result<Option<Session>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {SESSION_COLS} FROM installation_sessions WHERE session_id = ?1"),
                [session_id],
                session_row,
            )
            .optional()?)
    }

    /// All sessions, newest first.
    pub fn sessions(&self) -> Result<Vec<Session>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {SESSION_COLS} FROM installation_sessions ORDER BY session_id DESC"
        ))?;
        Ok(st
            .query_map([], session_row)?
            .collect::<rusqlite::Result<_>>()?)
    }

    // ---- nodes ----

    fn upsert_node_on(conn: &Connection, n: &NewNode) -> Result<i64> {
        Ok(conn.query_row(
            "INSERT INTO system_nodes(node_type, path, hash, acl_fingerprint, first_detected_in)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(path) DO UPDATE SET
                hash = COALESCE(excluded.hash, hash),
                acl_fingerprint = COALESCE(excluded.acl_fingerprint, acl_fingerprint),
                first_detected_in = COALESCE(first_detected_in, excluded.first_detected_in)
             RETURNING node_id",
            params![
                n.node_type.as_str(),
                n.path,
                n.hash,
                n.acl_fingerprint,
                n.first_detected_in
            ],
            |r| r.get(0),
        )?)
    }

    /// Insert a node or refresh hash/ACL of an existing path. `first_detected_in` is never overwritten.
    pub fn upsert_node(&self, node: &NewNode) -> Result<i64> {
        Self::upsert_node_on(&self.conn, node)
    }

    /// Batch upsert in one transaction.
    pub fn upsert_nodes(&mut self, nodes: &[NewNode]) -> Result<Vec<i64>> {
        let tx = self.conn.transaction()?;
        let mut ids = Vec::with_capacity(nodes.len());
        for n in nodes {
            ids.push(Self::upsert_node_on(&tx, n)?);
        }
        tx.commit()?;
        Ok(ids)
    }

    /// Node by id.
    pub fn node(&self, node_id: i64) -> Result<Option<Node>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {NODE_COLS} FROM system_nodes WHERE node_id = ?1"),
                [node_id],
                node_row,
            )
            .optional()?)
    }

    /// Node by path.
    pub fn node_by_path(&self, path: &str) -> Result<Option<Node>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {NODE_COLS} FROM system_nodes WHERE path = ?1"),
                [path],
                node_row,
            )
            .optional()?)
    }

    /// Nodes first detected in a session.
    pub fn nodes_for_session(&self, session_id: i64) -> Result<Vec<Node>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {NODE_COLS} FROM system_nodes WHERE first_detected_in = ?1 ORDER BY node_id"
        ))?;
        Ok(st
            .query_map([session_id], node_row)?
            .collect::<rusqlite::Result<_>>()?)
    }

    // ---- DAG ----

    fn would_create_cycle(&self, parent: i64, child: i64) -> Result<bool> {
        let hit: Option<i64> = self
            .conn
            .query_row(
                "WITH RECURSIVE reach(id) AS (
                    SELECT ?1
                    UNION
                    SELECT e.child_node_id FROM node_relationships e JOIN reach r ON e.parent_node_id = r.id
                    WHERE e.relationship_type IN ('created_by','modified_by','depends_on')
                 ) SELECT 1 FROM reach WHERE id = ?2 LIMIT 1",
                params![child, parent],
                |r| r.get(0),
            )
            .optional()?;
        Ok(hit.is_some())
    }

    /// Add (or re-score) an edge. Rejects cycles for every type except the symmetric `shared_with`.
    /// Re-adding keeps the higher confidence.
    pub fn add_edge(
        &self,
        parent: i64,
        child: i64,
        kind: RelationshipType,
        evidence: Evidence,
    ) -> Result<i64> {
        if kind != RelationshipType::SharedWith && self.would_create_cycle(parent, child)? {
            return Err(StorageError::CycleDetected { parent, child });
        }
        Ok(self.conn.query_row(
            "INSERT INTO node_relationships(parent_node_id, child_node_id, relationship_type, confidence_score, evidence_source)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(parent_node_id, child_node_id, relationship_type) DO UPDATE SET
                confidence_score = MAX(confidence_score, excluded.confidence_score),
                evidence_source = CASE WHEN excluded.confidence_score > confidence_score
                                       THEN excluded.evidence_source ELSE evidence_source END
             RETURNING edge_id",
            params![parent, child, kind.as_str(), evidence.score(), evidence.label()],
            |r| r.get(0),
        )?)
    }

    /// Edges where `node_id` is the child.
    pub fn edges_as_child(&self, node_id: i64) -> Result<Vec<Edge>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {EDGE_COLS} FROM node_relationships WHERE child_node_id = ?1 ORDER BY edge_id"
        ))?;
        Ok(st
            .query_map([node_id], edge_row)?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Edges where `node_id` is the parent.
    pub fn edges_as_parent(&self, node_id: i64) -> Result<Vec<Edge>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {EDGE_COLS} FROM node_relationships WHERE parent_node_id = ?1 ORDER BY edge_id"
        ))?;
        Ok(st
            .query_map([node_id], edge_row)?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Trace an artifact back to its installation session, with edges and ledger.
    pub fn provenance_for_path(&self, path: &str) -> Result<Option<Provenance>> {
        let Some(node) = self.node_by_path(path)? else {
            return Ok(None);
        };
        let session = match node.first_detected_in {
            Some(id) => self.session(id)?,
            None => None,
        };
        let origins = self.edges_as_child(node.node_id)?;
        let dependents = self.edges_as_parent(node.node_id)?;
        let ledger = self.ledger_for_path(path)?;
        let confidence = origins
            .iter()
            .filter(|e| e.relationship_type == RelationshipType::CreatedBy)
            .map(|e| e.confidence_score)
            .fold(None, |acc: Option<f64>, c| {
                Some(acc.map_or(c, |a| a.max(c)))
            })
            .unwrap_or(Evidence::Heuristic.score());
        Ok(Some(Provenance {
            node,
            session,
            origins,
            dependents,
            ledger,
            confidence,
        }))
    }

    /// Shared-component impact map: everything that depends on or shares `node_id`.
    pub fn shared_component_impact(&self, node_id: i64) -> Result<SharedImpact> {
        let node = self
            .node(node_id)?
            .ok_or_else(|| StorageError::NotFound(format!("node {node_id}")))?;
        let mut st = self.conn.prepare(&format!(
            "SELECT {cols} FROM node_relationships e JOIN system_nodes n ON n.node_id = e.child_node_id
               WHERE e.parent_node_id = ?1 AND e.relationship_type IN ('depends_on','shared_with')
             UNION
             SELECT {cols} FROM node_relationships e JOIN system_nodes n ON n.node_id = e.parent_node_id
               WHERE e.child_node_id = ?1 AND e.relationship_type = 'shared_with'
             ORDER BY 1",
            cols = "n.node_id, n.node_type, n.path, n.hash, n.acl_fingerprint, n.first_detected_in"
        ))?;
        let dependents: Vec<Node> = st
            .query_map([node_id], node_row)?
            .collect::<rusqlite::Result<_>>()?;
        let mut sessions: Vec<i64> = dependents
            .iter()
            .filter_map(|d| d.first_detected_in)
            .chain(node.first_detected_in)
            .collect();
        sessions.sort_unstable();
        sessions.dedup();
        Ok(SharedImpact {
            node,
            dependents,
            sessions,
        })
    }

    /// Dependencies that only `session_id` relies on and nobody shares: removal candidates for review.
    pub fn orphaned_dependencies(&self, session_id: i64) -> Result<Vec<Node>> {
        let mut st = self.conn.prepare(
            "SELECT DISTINCT p.node_id, p.node_type, p.path, p.hash, p.acl_fingerprint, p.first_detected_in
             FROM system_nodes p
             JOIN node_relationships e ON e.parent_node_id = p.node_id AND e.relationship_type = 'depends_on'
             JOIN system_nodes c ON c.node_id = e.child_node_id
             WHERE c.first_detected_in = ?1
               AND NOT EXISTS (
                   SELECT 1 FROM node_relationships e2 JOIN system_nodes c2 ON c2.node_id = e2.child_node_id
                   WHERE e2.parent_node_id = p.node_id AND e2.relationship_type = 'depends_on'
                     AND c2.first_detected_in IS NOT ?1)
               AND NOT EXISTS (
                   SELECT 1 FROM node_relationships e3
                   WHERE e3.relationship_type = 'shared_with'
                     AND (e3.parent_node_id = p.node_id OR e3.child_node_id = p.node_id))
             ORDER BY p.node_id",
        )?;
        Ok(st
            .query_map([session_id], node_row)?
            .collect::<rusqlite::Result<_>>()?)
    }

    // ---- policy rules ----

    /// Store a rule. The condition must be a JSON object (JSONLogic expression).
    pub fn add_rule(
        &self,
        name: &str,
        condition: &serde_json::Value,
        action: PolicyAction,
    ) -> Result<i64> {
        if !condition.is_object() {
            return Err(StorageError::InvalidValue {
                field: "condition_json",
                value: condition.to_string(),
            });
        }
        self.conn.execute(
            "INSERT INTO policy_rules(rule_name, condition_json, action) VALUES (?1, ?2, ?3)",
            params![name, serde_json::to_string(condition)?, action.as_str()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// All rules by id.
    pub fn rules(&self) -> Result<Vec<PolicyRule>> {
        let mut st = self.conn.prepare(
            "SELECT rule_id, rule_name, condition_json, action FROM policy_rules ORDER BY rule_id",
        )?;
        Ok(st
            .query_map([], rule_row)?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Delete a rule; returns whether it existed.
    pub fn delete_rule(&self, rule_id: i64) -> Result<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM policy_rules WHERE rule_id = ?1", [rule_id])?
            > 0)
    }

    // ---- ledger ----

    /// Append to the immutable change ledger.
    pub fn append_ledger(&self, e: &NewLedgerEntry) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO change_ledger(session_id, node_id, path, change_kind, old_hash, new_hash, source)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![e.session_id, e.node_id, e.path, e.change_kind.as_str(), e.old_hash, e.new_hash, e.source.as_str()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Ledger of a session, oldest first.
    pub fn ledger_for_session(&self, session_id: i64) -> Result<Vec<LedgerEntry>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {LEDGER_COLS} FROM change_ledger WHERE session_id = ?1 ORDER BY entry_id"
        ))?;
        Ok(st
            .query_map([session_id], ledger_row)?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Ledger of a path, oldest first.
    pub fn ledger_for_path(&self, path: &str) -> Result<Vec<LedgerEntry>> {
        let mut st = self.conn.prepare(&format!(
            "SELECT {LEDGER_COLS} FROM change_ledger WHERE path = ?1 COLLATE NOCASE ORDER BY entry_id"
        ))?;
        Ok(st
            .query_map([path], ledger_row)?
            .collect::<rusqlite::Result<_>>()?)
    }

    // ---- quarantine ----

    /// Record a quarantine manifest.
    pub fn record_quarantine(&self, q: &QuarantineRecord) -> Result<()> {
        self.conn.execute(
            "INSERT INTO quarantine_manifests(quarantine_id, rule_id, session_id, status, manifest_path, manifest_json)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![q.quarantine_id, q.rule_id, q.session_id, q.status.as_str(), q.manifest_path, q.manifest_json],
        )?;
        Ok(())
    }

    /// Change a quarantine's status.
    pub fn set_quarantine_status(
        &self,
        quarantine_id: &str,
        status: QuarantineStatus,
    ) -> Result<()> {
        let n = self.conn.execute(
            "UPDATE quarantine_manifests SET status = ?2 WHERE quarantine_id = ?1",
            params![quarantine_id, status.as_str()],
        )?;
        if n == 0 {
            return Err(StorageError::NotFound(format!(
                "quarantine {quarantine_id}"
            )));
        }
        Ok(())
    }

    /// All quarantine records, newest first.
    pub fn quarantines(&self) -> Result<Vec<QuarantineRecord>> {
        let mut st = self.conn.prepare(
            "SELECT quarantine_id, rule_id, session_id, status, manifest_path, manifest_json
             FROM quarantine_manifests ORDER BY created_at DESC, rowid DESC",
        )?;
        let rows = st.query_map([], |r| {
            Ok(QuarantineRecord {
                quarantine_id: r.get(0)?,
                rule_id: r.get(1)?,
                session_id: r.get(2)?,
                status: col(r, 3)?,
                manifest_path: r.get(4)?,
                manifest_json: r.get(5)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}
