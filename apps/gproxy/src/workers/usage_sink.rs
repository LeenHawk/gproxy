//! Batched usage log writer.
//!
//! Receives usage records via an mpsc channel and writes them to the database
//! in batches. Batches flush when either 100 records accumulate or 500ms pass,
//! whichever comes first.

use std::time::Duration;

use tokio::sync::mpsc;

use super::ShutdownRx;
use gproxy_storage::{SeaOrmStorage, StorageWriteBatch, StorageWriteEvent, UsageWrite};

const BATCH_SIZE: usize = 100;
const FLUSH_INTERVAL: Duration = Duration::from_millis(500);

/// Spawn the usage sink worker. Returns the sender handle for producers.
pub fn spawn(storage: SeaOrmStorage, shutdown: ShutdownRx) -> mpsc::Sender<UsageWrite> {
    let (tx, rx) = mpsc::channel(1024);
    tokio::spawn(run(storage, rx, shutdown));
    tx
}

async fn run(storage: SeaOrmStorage, mut rx: mpsc::Receiver<UsageWrite>, mut shutdown: ShutdownRx) {
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
    let mut write_batch = StorageWriteBatch::default();
    for record in batch {
        write_batch.apply(StorageWriteEvent::UpsertUsage(record));
    }
    if let Err(err) = storage.apply_write_batch(write_batch).await {
        tracing::error!(count, %err, "failed to flush usage batch");
    } else {
        tracing::trace!(count, "flushed usage batch");
    }
}
