use gproxy_channel_api::BoxFuture;
use gproxy_core::{CacheBackend, CoreError, Settlement};

use super::super::AppHost;
use super::types::{AdmissionState, reservation_key};

pub(in crate::host) fn finish<'a>(
    host: &'a AppHost,
    request_id: &'a str,
    settlement: Option<&'a Settlement>,
) -> BoxFuture<'a, ()> {
    Box::pin(async move {
        let key = reservation_key(request_id);
        let state = match load(host, request_id).await {
            Ok(state) => state,
            Err(error) => {
                tracing::error!(request_id, error = %error, "load admission reservation failed");
                return;
            }
        };
        let Some(mut state) = state else {
            return;
        };
        for index in 0..state.reservations.len() {
            if !state.reservations[index].cost_recorded {
                if let Some(settlement) = settlement
                    && let Err(error) = host
                        .services
                        .store
                        .add_quota_cost(state.reservations[index].window_id, settlement.cost)
                        .await
                {
                    tracing::error!(request_id, error = %error, "persist quota cost failed");
                    continue;
                }
                state.reservations[index].cost_recorded = true;
                if let Err(error) = save(host, &key, &state).await {
                    tracing::error!(request_id, error = %error, "checkpoint quota reconciliation failed");
                    return;
                }
            }
            if !state.reservations[index].released
                && let Err(error) = host
                    .services
                    .cache
                    .incr(
                        &state.reservations[index].cache_key,
                        -state.reservations[index].estimated_cost_micros,
                        None,
                    )
                    .await
            {
                tracing::error!(request_id, error = %error, "release quota reservation failed");
                continue;
            }
            if !state.reservations[index].released {
                state.reservations[index].released = true;
                if let Err(error) = save(host, &key, &state).await {
                    tracing::error!(request_id, error = %error, "checkpoint quota release failed");
                    return;
                }
            }
        }
        if state
            .reservations
            .iter()
            .any(|reservation| !reservation.released)
        {
            return;
        }
        if let Err(error) = host.services.cache.delete(&key).await {
            tracing::error!(request_id, error = %error, "delete admission reservation failed");
        }
    })
}

async fn save(host: &AppHost, key: &str, state: &AdmissionState) -> Result<(), CoreError> {
    let bytes = serde_json::to_vec(state)
        .map_err(|error| CoreError::Internal(format!("serialize admission: {error}")))?;
    host.services.cache.set(key, bytes, None).await?;
    Ok(())
}

pub(in crate::host) async fn load(
    host: &AppHost,
    request_id: &str,
) -> Result<Option<AdmissionState>, CoreError> {
    host.services
        .cache
        .get(&reservation_key(request_id))
        .await?
        .map(|bytes| {
            serde_json::from_slice(&bytes)
                .map_err(|error| CoreError::Internal(format!("decode admission: {error}")))
        })
        .transpose()
}
