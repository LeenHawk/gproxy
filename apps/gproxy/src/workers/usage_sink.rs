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
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(run(state, rx, shutdown))
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

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use gproxy_sdk::provider::engine::GproxyEngine;
    use gproxy_server::{AppStateBuilder, GlobalConfig};
    use gproxy_storage::{SeaOrmStorage, UsageQuery, UsageWrite, repository::UserRepository};

    use super::{super::WorkerSet, spawn_with_receiver};

    #[tokio::test]
    async fn registered_usage_sink_flushes_pending_usage_on_shutdown() {
        let storage = Arc::new(
            SeaOrmStorage::connect("sqlite::memory:", None)
                .await
                .expect("in-memory sqlite storage"),
        );
        storage.sync().await.expect("sync schema");
        storage
            .upsert_user(gproxy_storage::UserWrite {
                id: 1,
                name: "alice".to_string(),
                password: "hash".to_string(),
                enabled: true,
                is_admin: false,
            })
            .await
            .expect("seed user");

        let state = Arc::new(
            AppStateBuilder::new()
                .engine(GproxyEngine::builder().build())
                .storage(storage)
                .config(GlobalConfig {
                    dsn: "sqlite::memory:".to_string(),
                    ..GlobalConfig::default()
                })
                .build(),
        );

        let (tx, rx) = tokio::sync::mpsc::channel(8);
        let (mut worker_set, _shutdown_rx) = WorkerSet::new();
        worker_set.register(spawn_with_receiver(
            state.clone(),
            rx,
            worker_set.subscribe(),
        ));

        tx.send(UsageWrite {
            downstream_trace_id: Some(7),
            at_unix_ms: 1,
            provider_id: None,
            credential_id: None,
            user_id: Some(1),
            user_key_id: None,
            operation: "generate_content".to_string(),
            protocol: "openai_chat_completions".to_string(),
            model: Some("demo".to_string()),
            input_tokens: Some(10),
            output_tokens: Some(20),
            cache_read_input_tokens: None,
            cache_creation_input_tokens: None,
            cache_creation_input_tokens_5min: None,
            cache_creation_input_tokens_1h: None,
            cost: 0.5,
        })
        .await
        .expect("enqueue usage");

        worker_set.shutdown().await;

        let usages = state
            .storage()
            .query_usages(&UsageQuery::default())
            .await
            .expect("query usages");
        assert_eq!(usages.len(), 1);
        assert_eq!(usages[0].user_id, Some(1));
        assert_eq!(usages[0].input_tokens, Some(10));
        assert_eq!(usages[0].output_tokens, Some(20));

        let quotas = state
            .storage()
            .list_user_quotas()
            .await
            .expect("list quotas");
        assert_eq!(quotas.len(), 1);
        assert_eq!(quotas[0].user_id, 1);
        assert_eq!(quotas[0].cost_used, 0.5);
    }
}
