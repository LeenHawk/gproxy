//! Batched usage log writer with durable quota persistence.
//!
//! Receives usage records via an mpsc channel and writes them to the database
//! in batches. Each record's cost is atomically applied to the user's quota
//! in the same DB transaction via `record_usage_and_quota_cost`, ensuring
//! billing durability across restarts.

use std::time::Duration;

use tokio::sync::mpsc;

use super::ShutdownRx;
use gproxy_storage::{SeaOrmStorage, UsageWrite};

const BATCH_SIZE: usize = 100;
const FLUSH_INTERVAL: Duration = Duration::from_millis(500);

/// Spawn the usage sink worker. Returns the sender handle for producers.
pub fn spawn(storage: SeaOrmStorage, shutdown: ShutdownRx) -> mpsc::Sender<UsageWrite> {
    let (tx, rx) = mpsc::channel(1024);
    tokio::spawn(run(storage, rx, shutdown));
    tx
}

async fn run(
    storage: SeaOrmStorage,
    mut rx: mpsc::Receiver<UsageWrite>,
    mut shutdown: ShutdownRx,
) {
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
                            flush(&storage, &mut buffer).await;
                        }
                    }
                    None => break,
                }
            }
            _ = tokio::time::sleep(FLUSH_INTERVAL) => {
                if !buffer.is_empty() {
                    flush(&storage, &mut buffer).await;
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
        flush(&storage, &mut buffer).await;
    }
    tracing::debug!("usage sink worker shut down");
}

async fn flush(storage: &SeaOrmStorage, buffer: &mut Vec<UsageWrite>) {
    let batch = std::mem::take(buffer);
    let count = batch.len();
    let mut success = 0usize;
    // Use record_usage_and_quota_cost to atomically persist both the
    // usage log entry AND the quota cost_used increment in one transaction.
    for record in batch {
        let cost = record.cost;
        if let Err(err) = storage.record_usage_and_quota_cost(record, cost).await {
            tracing::error!(%err, "failed to persist usage+quota record");
        } else {
            success += 1;
        }
    }
    if success > 0 {
        tracing::trace!(success, count, "flushed usage+quota batch");
    }
}
