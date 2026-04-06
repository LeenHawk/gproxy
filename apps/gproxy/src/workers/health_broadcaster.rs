//! Debounced credential health state broadcaster.
//!
//! Subscribes to SDK EngineEvent broadcasts and persists credential health
//! changes to the database with a 500ms debounce window.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::broadcast;

use super::ShutdownRx;
use gproxy_sdk::provider::store::EngineEvent;
use gproxy_server::AppState;
use gproxy_storage::{CredentialStatusWrite, StorageWriteEvent};

const DEBOUNCE_WINDOW: Duration = Duration::from_millis(500);

/// Spawn the health broadcaster worker.
pub fn spawn(
    mut event_rx: broadcast::Receiver<EngineEvent>,
    state: Arc<AppState>,
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
                    flush_pending(&state, &mut pending).await;
                }
            }
        }

        if !pending.is_empty() {
            flush_pending(&state, &mut pending).await;
        }
        tracing::debug!("health broadcaster worker shut down");
    })
}

async fn flush_pending(state: &AppState, pending: &mut HashMap<(String, usize), String>) {
    let entries: Vec<_> = pending.drain().collect();
    let storage = state.storage();
    for ((provider, index), status) in &entries {
        // Resolve credential DB id from provider name + index
        let credential_id = state.credential_id_for_index(provider, *index).unwrap_or(0);
        if credential_id == 0 {
            tracing::debug!(
                provider,
                index,
                "skipping health persist: credential_id not found"
            );
            continue;
        }
        let write = CredentialStatusWrite {
            id: None,
            credential_id,
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
    if !entries.is_empty() {
        tracing::trace!(count = entries.len(), "flushed health state changes");
    }
}
