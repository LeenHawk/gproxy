//! Periodic quota reconciler.
//!
//! Every 30 seconds, reads the authoritative quota state from the database
//! and updates the in-memory QuotaBackend to account for external changes.

use std::time::Duration;

use super::ShutdownRx;

const RECONCILE_INTERVAL: Duration = Duration::from_secs(30);

/// Spawn the quota reconciler worker.
pub fn spawn(mut shutdown: ShutdownRx) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                biased;
                _ = shutdown.changed() => break,
                _ = tokio::time::sleep(RECONCILE_INTERVAL) => {
                    // TODO: read DB quota state, update in-memory QuotaBackend
                    tracing::trace!("quota reconciler tick");
                }
            }
        }
        tracing::debug!("quota reconciler worker shut down");
    })
}
