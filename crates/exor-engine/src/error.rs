//! Engine errors.

use std::path::PathBuf;

/// All errors surfaced by `exor-engine`.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// Filesystem failure with the offending path.
    #[error("io error at {path}: {source}")]
    Io {
        /// Path involved.
        path: PathBuf,
        /// Cause.
        source: std::io::Error,
    },
    /// Storage failure.
    #[error(transparent)]
    Storage(#[from] exor_storage::StorageError),
    /// JSON failure.
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    /// Worker task panicked or was aborted.
    #[error("task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
    /// Policy expression is invalid.
    #[error("policy: {0}")]
    Policy(String),
    /// Quarantine could not complete safely.
    #[error("quarantine: {0}")]
    Quarantine(String),
    /// Operation was cancelled by the caller.
    #[error("cancelled")]
    Cancelled,
    /// Internal lock poisoned by a panicking thread.
    #[error("internal lock poisoned")]
    Poisoned,
}

/// Result alias.
pub type Result<T> = std::result::Result<T, EngineError>;

pub(crate) fn io_err(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> EngineError {
    let path = path.into();
    move |source| EngineError::Io { path, source }
}
