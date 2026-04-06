//! Periodic quota reconciler.
//!
//! Every 30 seconds, reads the authoritative quota state from the database
//! and updates the in-memory QuotaService to account for external changes
//! (e.g. admin top-ups, cross-instance sync).

use std::sync::Arc;
use std::time::Duration;

use super::ShutdownRx;
use gproxy_server::AppState;

const RECONCILE_INTERVAL: Duration = Duration::from_secs(30);

/// Spawn the quota reconciler worker.
pub fn spawn(state: Arc<AppState>, mut shutdown: ShutdownRx) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        loop {
            tokio::select! {
                biased;
                _ = shutdown.changed() => break,
                _ = tokio::time::sleep(RECONCILE_INTERVAL) => {
                    reconcile(&state).await;
                }
            }
        }
        tracing::debug!("quota reconciler worker shut down");
    })
}

async fn reconcile(state: &AppState) {
    match state.storage().list_user_quotas().await {
        Ok(rows) => {
            let mut updated = 0usize;
            for row in &rows {
                let (current_quota, current_used) = state.get_user_quota(row.user_id);
                // Only update if DB has a different quota total (admin changed it)
                // or if DB cost_used is higher (another instance charged more)
                if (row.quota - current_quota).abs() > f64::EPSILON
                    || row.cost_used > current_used
                {
                    state.upsert_user_quota_in_memory(row.user_id, row.quota, row.cost_used);
                    updated += 1;
                }
            }
            if updated > 0 {
                tracing::debug!(updated, total = rows.len(), "quota reconciler synced from DB");
            } else {
                tracing::trace!(total = rows.len(), "quota reconciler tick (no changes)");
            }
        }
        Err(err) => {
            tracing::warn!(%err, "quota reconciler failed to read DB");
        }
    }
}
