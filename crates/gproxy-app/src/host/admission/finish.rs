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
        super::credential_budget::release(host, request_id).await;
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
        'retry: for _ in 0..3 {
            for index in 0..state.reservations.len() {
                if state.reservations[index].released {
                    continue;
                }
                if let Some(settlement) = settlement
                    && let Err(error) = host
                        .services
                        .store
                        .add_quota_cost(
                            request_id,
                            state.reservations[index].window_id,
                            settlement.cost,
                        )
                        .await
                {
                    tracing::error!(request_id, error = %error, "persist quota cost failed");
                    continue;
                }
                let expected = match serde_json::to_vec(&state) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        tracing::error!(request_id, error = %error, "serialize quota state failed");
                        return;
                    }
                };
                state.reservations[index].cost_recorded = true;
                state.reservations[index].released = true;
                let updated = match serde_json::to_vec(&state) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        tracing::error!(request_id, error = %error, "serialize quota release failed");
                        return;
                    }
                };
                match host
                    .services
                    .cache
                    .compare_incr_and_set(
                        &state.reservations[index].cache_key,
                        -state.reservations[index].estimated_cost_micros,
                        &key,
                        expected,
                        updated,
                    )
                    .await
                {
                    Ok(Some(_)) => {}
                    Ok(None) => {
                        let Some(latest) = load(host, request_id).await.ok().flatten() else {
                            return;
                        };
                        state = latest;
                        continue 'retry;
                    }
                    Err(error) => {
                        tracing::error!(request_id, error = %error, "release quota reservation failed");
                        if let Ok(Some(latest)) = load(host, request_id).await {
                            state = latest;
                        }
                        continue 'retry;
                    }
                }
            }
            if state
                .reservations
                .iter()
                .all(|reservation| reservation.released)
            {
                break;
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
