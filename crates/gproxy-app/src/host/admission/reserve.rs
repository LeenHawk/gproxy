use std::time::Duration;

use gproxy_channel_api::{BoxFuture, CallerIdentity};
use gproxy_core::{CacheBackend, CoreError, Plan, RequestCtx};
use gproxy_protocol::OperationKey;

use super::super::AppHost;
use super::auth::{authorize, subject_matches, unix_now};
use super::types::{AdmissionState, CounterCharge, IdentityState, reservation_key};

pub(in crate::host) fn admit<'a>(
    host: &'a AppHost,
    identity: &'a CallerIdentity,
    request: &'a RequestCtx,
    operation: Option<OperationKey>,
    plan: &'a Plan,
) -> BoxFuture<'a, Result<Plan, CoreError>> {
    Box::pin(async move {
        let snapshot = host.services.control.current();
        let oauth_identity = super::auth::oauth_admission(host, identity, operation).await?;
        let identity = oauth_identity.as_ref().unwrap_or(identity);
        let plan = authorize(&snapshot, identity, operation, plan)?;
        let now = unix_now();
        let mut charged = Vec::new();
        let reservations = match super::quota::reserve(
            host,
            identity,
            request,
            operation,
            &plan,
            now,
            &mut charged,
        )
        .await
        {
            Ok(reservations) => reservations,
            Err(error) => return rollback_error(host, charged, error).await,
        };

        for limit in snapshot
            .rate_limits
            .iter()
            .filter(|limit| subject_matches(&limit.subject_kind, limit.subject_id, identity))
        {
            let start = window_start(now, limit.window_seconds);
            let key = format!("gproxy:rate:{}:{start}", limit.id);
            let count = match increment_window(host, &key, 1, limit.window_seconds, now).await {
                Ok(count) => count,
                Err(error) => return rollback_error(host, charged, error).await,
            };
            charged.push(CounterCharge { key, amount: 1 });
            if count > i64::try_from(limit.requests).expect("stored rate limit fits i64") {
                return rollback_error(
                    host,
                    charged,
                    CoreError::RateLimited {
                        retry_after_secs: u32::try_from(limit.window_seconds).unwrap_or(u32::MAX),
                    },
                )
                .await;
            }
        }

        let state = AdmissionState {
            identity: IdentityState::from(identity),
            operation: operation.map(|key| key.operation().id().to_owned()),
            reservations,
        };
        let bytes = match serde_json::to_vec(&state) {
            Ok(bytes) => bytes,
            Err(error) => {
                return rollback_error(
                    host,
                    charged,
                    CoreError::Internal(format!("serialize admission: {error}")),
                )
                .await;
            }
        };
        let key = reservation_key(&request.request_id);
        if let Err(error) = host.services.cache.set(&key, bytes, None).await {
            let _ = host.services.cache.delete(&key).await;
            return rollback_error(host, charged, error.into()).await;
        }
        Ok(plan)
    })
}

pub(super) async fn increment_window(
    host: &AppHost,
    key: &str,
    amount: i64,
    window_seconds: u64,
    now: i64,
) -> Result<i64, CoreError> {
    let start = window_start(now, window_seconds);
    let seconds = i64::try_from(window_seconds).expect("stored window fits i64");
    let end = start.saturating_add(seconds);
    let ttl = u64::try_from(end.saturating_sub(now)).unwrap_or(1).max(1);
    Ok(host
        .services
        .cache
        .incr(key, amount, Some(Duration::from_secs(ttl)))
        .await?)
}

pub(super) async fn rollback_error<T>(
    host: &AppHost,
    charges: Vec<CounterCharge>,
    error: CoreError,
) -> Result<T, CoreError> {
    for charge in charges.into_iter().rev() {
        if let Err(rollback) = host
            .services
            .cache
            .incr(&charge.key, -charge.amount, None)
            .await
        {
            tracing::error!(error = %rollback, "admission rollback failed");
        }
    }
    Err(error)
}

fn window_start(now: i64, seconds: u64) -> i64 {
    let seconds = i64::try_from(seconds).expect("stored window fits i64");
    now - now.rem_euclid(seconds)
}
