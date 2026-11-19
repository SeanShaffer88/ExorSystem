//! Cooperative cancellation shared between UI and workers; awaitable, so no polling.

use std::sync::Arc;
use tokio::sync::watch;

/// Cloneable cancel handle.
#[derive(Debug, Clone)]
pub struct Cancel(Arc<watch::Sender<bool>>);

impl Default for Cancel {
    fn default() -> Self {
        Self(Arc::new(watch::channel(false).0))
    }
}

impl Cancel {
    /// New, not cancelled.
    pub fn new() -> Self {
        Self::default()
    }
    /// Request cancellation (idempotent).
    pub fn cancel(&self) {
        self.0.send_replace(true);
    }
    /// Whether cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        *self.0.borrow()
    }
    /// Resolves once cancelled.
    pub async fn cancelled(&self) {
        let mut rx = self.0.subscribe();
        // The sender lives inside self, so the channel cannot close while we wait.
        let _ = rx.wait_for(|v| *v).await;
    }
    /// Return a cancellation error at a safe stopping boundary.
    pub fn check(&self) -> crate::Result<()> {
        if self.is_cancelled() {
            Err(crate::EngineError::Cancelled)
        } else {
            Ok(())
        }
    }
}
