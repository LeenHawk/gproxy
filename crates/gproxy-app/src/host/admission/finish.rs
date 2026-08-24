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
        let actual = settlement.map(|settlement| {
            let tokens = settlement
                .usage
                .input_tokens
                .saturating_add(settlement.usage.output_tokens);
            i64::try_from(tokens).unwrap_or(i64::MAX)
        });
        for reservation in state.reservations {
            let delta = actual.unwrap_or_default() - reservation.estimated_tokens;
            if let Err(error) = host
                .services
                .cache
                .incr(&reservation.cache_key, delta, None)
                .await
            {
                tracing::error!(request_id, error = %error, "quota reconciliation failed");
            }
            if let Some(tokens) = actual
                && let Err(error) = host
                    .services
                    .store
                    .add_quota_usage(reservation.quota_id, reservation.window_start, tokens)
                    .await
            {
                tracing::error!(request_id, error = %error, "persist quota usage failed");
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
