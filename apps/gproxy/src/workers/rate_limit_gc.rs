//! Periodic garbage collector for expired rate-limit counters.
//!
//! The in-memory rate-limit counters accumulate entries per user+model.
//! This worker runs every 60 seconds to remove stale entries.

use std::sync::Arc;
use std::time::Duration;

use super::ShutdownRx;
use gproxy_server::AppState;

const GC_INTERVAL: Duration = Duration::from_secs(60);

/// Spawn the rate-limit GC worker.
pub fn spawn(state: Arc<AppState>, mut shutdown: ShutdownRx) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                biased;
                _ = shutdown.changed() => break,
                _ = tokio::time::sleep(GC_INTERVAL) => {
                    state.rate_counters.purge_expired();
                    tracing::trace!("rate-limit GC sweep completed");
                }
            }
        }
        tracing::debug!("rate-limit GC worker shut down");
    })
}
