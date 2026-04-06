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
                // Sync if DB has different quota total (admin changed it)
                // OR if DB cost_used differs from memory in either direction:
                // - DB > memory: another instance charged more
                // - DB < memory: usage_sink failed to persist, memory is inflated
                //   (prevents permanently locking users out after transient DB errors)
                let quota_changed = (row.quota - current_quota).abs() > f64::EPSILON;
                let cost_diverged = (row.cost_used - current_used).abs() > f64::EPSILON;
                if quota_changed || cost_diverged {
                    state.upsert_user_quota_in_memory(row.user_id, row.quota, row.cost_used);
                    updated += 1;
                }
            }
            if updated > 0 {
                tracing::debug!(
                    updated,
                    total = rows.len(),
                    "quota reconciler synced from DB"
                );
            } else {
                tracing::trace!(total = rows.len(), "quota reconciler tick (no changes)");
            }
        }
        Err(err) => {
            tracing::warn!(%err, "quota reconciler failed to read DB");
        }
    }
}
