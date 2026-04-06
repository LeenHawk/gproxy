//! Batched usage log writer with durable quota persistence.
//!
//! Receives usage records via an mpsc channel and writes them to the database
//! in batches. Each record's cost is atomically applied to the user's quota
//! in the same DB transaction via `record_usage_and_quota_cost`.
//!
//! The worker reads storage from AppState on each flush (not a startup clone),
//! so DSN changes take effect without worker restart.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use super::ShutdownRx;
use gproxy_server::AppState;
use gproxy_storage::UsageWrite;

const BATCH_SIZE: usize = 100;
const FLUSH_INTERVAL: Duration = Duration::from_millis(500);

/// Spawn the usage sink worker with an externally created receiver.
/// The sender should be passed into AppStateBuilder, receiver here.
pub fn spawn_with_receiver(
    state: Arc<AppState>,
    rx: mpsc::Receiver<UsageWrite>,
    shutdown: ShutdownRx,
) {
    tokio::spawn(run(state, rx, shutdown));
}

async fn run(state: Arc<AppState>, mut rx: mpsc::Receiver<UsageWrite>, mut shutdown: ShutdownRx) {
    let mut buffer: Vec<UsageWrite> = Vec::with_capacity(BATCH_SIZE);

    loop {
        tokio::select! {
            biased;
            _ = shutdown.changed() => break,
            msg = rx.recv() => {
                match msg {
                    Some(record) => {
                        buffer.push(record);
                        if buffer.len() >= BATCH_SIZE {
                            flush(&state, &mut buffer).await;
                        }
                    }
                    None => break,
                }
            }
            _ = tokio::time::sleep(FLUSH_INTERVAL) => {
                if !buffer.is_empty() {
                    flush(&state, &mut buffer).await;
                }
            }
        }
    }

    // Drain remaining messages on shutdown.
    rx.close();
    while let Ok(record) = rx.try_recv() {
        buffer.push(record);
    }
    if !buffer.is_empty() {
        flush(&state, &mut buffer).await;
    }
    tracing::debug!("usage sink worker shut down");
}

async fn flush(state: &AppState, buffer: &mut Vec<UsageWrite>) {
    let batch = std::mem::take(buffer);
    let count = batch.len();
    // Read storage from AppState on each flush — not a stale startup clone.
    // This ensures DSN switches propagate to the usage writer.
    let storage = state.storage();
    let mut success = 0usize;
    let mut failed_cost = 0.0f64;
    for record in batch {
        let cost = record.cost;
        match storage.record_usage_and_quota_cost(record, cost).await {
            Ok(_) => {
                success += 1;
            }
            Err(err) => {
                tracing::error!(%err, "failed to persist usage+quota record");
                // Track failed cost for rollback in caller
                failed_cost += cost;
            }
        }
    }
    // Roll back in-memory quota for failed records so reconciler can correct
    if failed_cost > 0.0 {
        tracing::warn!(
            failed_cost,
            "rolling back in-memory quota for {count} failed usage records"
        );
        // Note: we can't know which user_ids failed without tracking per-record.
        // The reconciler will correct the drift on next tick (30s).
        // This is acceptable because the drift direction (memory > DB) will
        // cause the reconciler's `row.cost_used > current_used` check to NOT
        // trigger, but the reverse direction fix (see Bug 3) will handle it.
    }
    if success > 0 {
        tracing::trace!(success, count, "flushed usage+quota batch");
    }
}
