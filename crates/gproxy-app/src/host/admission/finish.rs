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
        let Some(state) = state else {
            return;
        };
        for reservation in state.reservations {
            let durable = if let Some(settlement) = settlement {
                match host
                    .services
                    .store
                    .add_quota_cost(reservation.window_id, settlement.cost)
                    .await
                {
                    Ok(_) => true,
                    Err(error) => {
                        tracing::error!(request_id, error = %error, "persist quota cost failed");
                        false
                    }
                }
            } else {
                true
            };
            if durable
                && let Err(error) = host
                    .services
                    .cache
                    .incr(
                        &reservation.cache_key,
                        -reservation.estimated_cost_micros,
                        Some(super::types::RESERVATION_TTL),
                    )
                    .await
            {
                tracing::error!(request_id, error = %error, "release quota reservation failed");
            }
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
