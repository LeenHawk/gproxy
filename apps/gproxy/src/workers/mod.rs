//! Background workers for asynchronous data plane operations.
//!
//! These workers handle fire-and-forget writes that should not block the
//! request processing path: usage logging, quota reconciliation, health
//! state persistence, and rate-limit counter garbage collection.

pub mod health_broadcaster;
pub mod quota_reconciler;
pub mod rate_limit_gc;
pub mod usage_sink;

use tokio::sync::watch;
use tokio::task::JoinHandle;

/// Shared shutdown signal. Workers listen on the receiver; the owner
/// drops or sends on the sender to trigger shutdown.
pub type ShutdownTx = watch::Sender<bool>;
/// Receiver half of the shutdown signal.
pub type ShutdownRx = watch::Receiver<bool>;

/// Create a new shutdown signal pair.
pub fn shutdown_signal() -> (ShutdownTx, ShutdownRx) {
    watch::channel(false)
}

/// Manages the lifecycle of all background workers.
pub struct WorkerSet {
    handles: Vec<JoinHandle<()>>,
    shutdown_tx: ShutdownTx,
}

impl WorkerSet {
    /// Create a new worker set with a shared shutdown signal.
    pub fn new() -> (Self, ShutdownRx) {
        let (tx, rx) = shutdown_signal();
        (
            Self {
                handles: Vec::new(),
                shutdown_tx: tx,
            },
            rx,
        )
    }

    /// Get a new shutdown receiver for a worker.
    pub fn subscribe(&self) -> ShutdownRx {
        self.shutdown_tx.subscribe()
    }

    /// Register a worker handle.
    pub fn register(&mut self, handle: JoinHandle<()>) {
        self.handles.push(handle);
    }

    /// Signal all workers to shut down and wait for them to finish.
    ///
    /// Workers have up to 5 seconds to drain their buffers.
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(true);
        let timeout = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            futures_util::future::join_all(self.handles),
        );
        if timeout.await.is_err() {
            tracing::warn!("background workers did not shut down within 5s timeout");
        }
    }
}

impl Default for WorkerSet {
    fn default() -> Self {
        Self::new().0
    }
}
