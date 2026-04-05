//! Debounced credential health state broadcaster.
//!
//! Subscribes to SDK EngineEvent broadcasts and persists credential health
//! changes to the database with a 500ms debounce window.

use std::collections::HashMap;
use std::time::Duration;

use tokio::sync::broadcast;

use super::ShutdownRx;
use gproxy_sdk::provider::store::EngineEvent;
use gproxy_storage::{CredentialStatusWrite, SeaOrmStorage, StorageWriteEvent};

const DEBOUNCE_WINDOW: Duration = Duration::from_millis(500);

/// Spawn the health broadcaster worker.
pub fn spawn(
    mut event_rx: broadcast::Receiver<EngineEvent>,
    storage: SeaOrmStorage,
    mut shutdown: ShutdownRx,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut pending: HashMap<(String, usize), String> = HashMap::new();

        loop {
            tokio::select! {
                biased;
                _ = shutdown.changed() => break,
                event = event_rx.recv() => {
                    match event {
                        Ok(EngineEvent::CredentialHealthChanged { provider, index, status }) => {
                            pending.insert((provider, index), status);
                        }
                        Err(broadcast::error::RecvError::Lagged(n)) => {
                            tracing::warn!(n, "health broadcaster lagged");
                        }
                        Err(broadcast::error::RecvError::Closed) => break,
                        Ok(_) => {}
                    }
                }
                _ = tokio::time::sleep(DEBOUNCE_WINDOW), if !pending.is_empty() => {
                    flush_pending(&storage, &mut pending).await;
                }
            }
        }

        if !pending.is_empty() {
            flush_pending(&storage, &mut pending).await;
        }
        tracing::debug!("health broadcaster worker shut down");
    })
}

async fn flush_pending(
    storage: &SeaOrmStorage,
    pending: &mut HashMap<(String, usize), String>,
) {
    let entries: Vec<_> = pending.drain().collect();
    for ((provider, _index), status) in &entries {
        let write = CredentialStatusWrite {
            id: None,
            credential_id: 0, // TODO: resolve from SDK ProviderRegistry
            channel: provider.clone(),
            health_kind: status.clone(),
            health_json: None,
            checked_at_unix_ms: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i64,
            ),
            last_error: None,
        };
        if let Err(err) = storage
            .apply_write_event(StorageWriteEvent::UpsertCredentialStatus(write))
            .await
        {
            tracing::error!(%err, provider, "failed to persist credential health");
        }
    }
    tracing::trace!(count = entries.len(), "flushed health state changes");
}
