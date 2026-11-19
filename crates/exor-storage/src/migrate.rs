//! Forward-only, transactional schema migrations.

use crate::error::{Result, StorageError};
use rusqlite::Connection;

/// One embedded migration.
pub struct Migration {
    /// Monotonic version, starting at 1.
    pub version: u32,
    /// Short name.
    pub name: &'static str,
    /// SQL batch.
    pub sql: &'static str,
}

/// All migrations, in order.
pub const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        name: "blueprint_schema",
        sql: include_str!("../migrations/0001_blueprint_schema.sql"),
    },
    Migration {
        version: 2,
        name: "ledger_quarantine",
        sql: include_str!("../migrations/0002_ledger_quarantine.sql"),
    },
    Migration {
        version: 3,
        name: "inventory_and_lookup",
        sql: include_str!("../migrations/0003_inventory_and_lookup.sql"),
    },
    Migration {
        version: 4,
        name: "watch_comparisons",
        sql: include_str!("../migrations/0004_watch_comparisons.sql"),
    },
    Migration {
        version: 5,
        name: "review_and_index",
        sql: include_str!("../migrations/0005_review_and_index.sql"),
    },
];

/// Highest version known to this build.
pub fn latest_version() -> u32 {
    MIGRATIONS.last().map_or(0, |m| m.version)
}

fn ensure_table(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        );",
    )?;
    Ok(())
}

/// Version currently applied (0 for a fresh database).
pub fn current_version(conn: &Connection) -> Result<u32> {
    ensure_table(conn)?;
    let v: Option<u32> = conn.query_row("SELECT MAX(version) FROM schema_migrations", [], |r| {
        r.get(0)
    })?;
    Ok(v.unwrap_or(0))
}

/// Apply every pending migration up to and including `target`. Returns applied versions.
pub fn migrate_to(conn: &mut Connection, target: u32) -> Result<Vec<u32>> {
    let current = current_version(conn)?;
    if current > latest_version() {
        return Err(StorageError::SchemaTooNew {
            found: current,
            supported: latest_version(),
        });
    }
    let mut applied = Vec::new();
    for m in MIGRATIONS
        .iter()
        .filter(|m| m.version > current && m.version <= target)
    {
        let tx = conn.transaction()?;
        let run = || -> rusqlite::Result<()> {
            tx.execute_batch(m.sql)?;
            tx.execute(
                "INSERT INTO schema_migrations(version, name) VALUES (?1, ?2)",
                rusqlite::params![m.version, m.name],
            )?;
            Ok(())
        };
        run().map_err(|source| StorageError::Migration {
            version: m.version,
            source,
        })?;
        tx.commit().map_err(|source| StorageError::Migration {
            version: m.version,
            source,
        })?;
        applied.push(m.version);
    }
    Ok(applied)
}

/// Apply all pending migrations.
pub fn migrate(conn: &mut Connection) -> Result<Vec<u32>> {
    migrate_to(conn, latest_version())
}
