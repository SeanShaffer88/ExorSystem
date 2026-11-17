//! ExorSystem storage layer: SQLite in WAL mode holding filesystem/registry nodes,
//! the relationship DAG with confidence scores, the change ledger and quarantine records.

mod error;
mod migrate;
mod model;
mod store;

pub use error::{Result, StorageError};
pub use migrate::{MIGRATIONS, Migration, current_version, latest_version, migrate, migrate_to};
pub use model::*;
pub use store::Store;
