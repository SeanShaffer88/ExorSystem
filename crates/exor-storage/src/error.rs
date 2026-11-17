//! Storage errors.

/// All errors surfaced by `exor-storage`.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    /// Underlying SQLite failure.
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// A migration failed and was rolled back.
    #[error("migration {version} failed: {source}")]
    Migration {
        /// Migration version.
        version: u32,
        /// Cause.
        source: rusqlite::Error,
    },
    /// The database was written by a newer ExorSystem.
    #[error("database schema version {found} is newer than supported {supported}")]
    SchemaTooNew {
        /// Version found in the file.
        found: u32,
        /// Highest version this build knows.
        supported: u32,
    },
    /// A value is outside its allowed domain.
    #[error("invalid value for {field}: {value}")]
    InvalidValue {
        /// Field or type name.
        field: &'static str,
        /// Offending value.
        value: String,
    },
    /// Adding the edge would break the acyclic property of the graph.
    #[error("edge {parent} -> {child} would create a cycle")]
    CycleDetected {
        /// Parent node id.
        parent: i64,
        /// Child node id.
        child: i64,
    },
    /// Row not found.
    #[error("not found: {0}")]
    NotFound(String),
    /// Filesystem failure (pre-migration backup).
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    /// JSON failure.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

/// Result alias.
pub type Result<T> = std::result::Result<T, StorageError>;
