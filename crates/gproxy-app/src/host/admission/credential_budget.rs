use gproxy_core::{CacheBackend, ControlPlane, CoreError, Settlement, Target};
use gproxy_protocol::SettleMode;
use serde::{Deserialize, Serialize};

use super::super::AppHost;
use super::auth::unix_now;
use super::types::CounterCharge;

/// Room taken in one budget window for a request that has not settled yet.
/// The pending counter is the same one user quotas use, so a window's
/// in-flight spend is one number whoever reserved it.
#[derive(Serialize, Deserialize)]
struct Reservation {
    cache_key: String,
    estimated_cost_micros: i64,
}

/// Reserve the request's estimated cost against the credential's budget
/// before it is sent. Settled spend alone would let concurrent requests
/// overrun the limit by everything in flight; the estimate closes that gap
/// and is released when the request settles or is abandoned.
pub(super) async fn reserve(
    host: &AppHost,
    request_id: &str,
    target: &Target,
    body: &bytes::Bytes,
    settle: SettleMode,
) -> Result<(), CoreError> {
    if settle == SettleMode::Free {
        return Ok(());
    }
    let snapshot = host.services.control.current();
    let Some(quota) = snapshot.quotas.iter().find(|quota| {
        quota.enabled
            && quota.subject_kind == "credential"
            && quota.subject_id == target.credential.0
    }) else {
        return Ok(());
    };
    if quota.limits().next().is_none() {
        return Ok(());
    }
    if host
        .services
        .cache
        .get(&failure_key(quota.id))
        .await?
        .is_some()
    {
        return Err(CoreError::Store(gproxy_core::error::StoreError(
            "credential budget settlement failed; repair accounting before retrying".into(),
        )));
    }
    if host
        .services
        .control
        .pricing(&target.provider, &target.upstream_model)
        .is_none()
    {
        return Err(CoreError::Internal(
            "credential cost limit requires model pricing".into(),
        ));
    }
    let estimate = super::quota::estimated_target_cost_micros(host, body, target).await?;
    let now = unix_now();
    let mut charged = Vec::new();
    let mut reservations = Vec::new();
    for (kind, limit) in quota.limits() {
        let window = host
            .services
            .store
            .ensure_quota_window(quota.id, kind, now)
            .await
            .map_err(store_error)?;
        let cache_key = pending_key(window.id);
        let pending = match host.services.cache.incr(&cache_key, estimate, None).await {
            Ok(pending) => pending,
            Err(error) => return super::reserve::rollback_error(host, charged, error.into()).await,
        };
        charged.push(CounterCharge {
            key: cache_key.clone(),
            amount: estimate,
        });
        let before = pending.saturating_sub(estimate).max(0);
        let projected = pending.max(0);
        let exhausted = window.cost_used + gproxy_core::usage::micros_to_cost(before) >= limit;
        let exceeds = window.cost_used + gproxy_core::usage::micros_to_cost(projected) > limit;
        if exhausted || exceeds {
            return super::reserve::rollback_error(host, charged, CoreError::QuotaExceeded).await;
        }
        reservations.push(Reservation {
            cache_key,
            estimated_cost_micros: estimate,
        });
    }
    let key = reservation_key(request_id);
    let mut held = load(host, request_id).await.unwrap_or_default();
    held.extend(reservations);
    let bytes = serde_json::to_vec(&held).map_err(|error| {
        CoreError::Internal(format!("serialize credential reservation: {error}"))
    })?;
    if let Err(error) = host.services.cache.set(&key, bytes, None).await {
        return super::reserve::rollback_error(host, charged, error.into()).await;
    }
    Ok(())
}

/// Give back every reservation the request holds. Failover may have
/// reserved against more than one credential; all of them are released.
pub(super) async fn release(host: &AppHost, request_id: &str) {
    let held = match load(host, request_id).await {
        Ok(held) => held,
        Err(error) => {
            tracing::error!(request_id, error = %error, "load credential reservation failed");
            return;
        }
    };
    if held.is_empty() {
        return;
    }
    for reservation in &held {
        if let Err(error) = host
            .services
            .cache
            .incr(
                &reservation.cache_key,
                -reservation.estimated_cost_micros,
                None,
            )
            .await
        {
            tracing::error!(request_id, error = %error, "release credential reservation failed");
        }
    }
    if let Err(error) = host
        .services
        .cache
        .delete(&reservation_key(request_id))
        .await
    {
        tracing::error!(request_id, error = %error, "delete credential reservation failed");
    }
}

pub(in crate::host) async fn record(host: &AppHost, settlement: &Settlement) {
    let snapshot = host.services.control.current();
    // A disabled limit still accumulates spend, so re-enabling it cannot reset usage.
    for quota in snapshot.quotas.iter().filter(|quota| {
        quota.subject_kind == "credential" && quota.subject_id == settlement.credential_id.0
    }) {
        let now = unix_now();
        for (kind, _) in quota.limits() {
            let mut persisted = false;
            for _ in 0..3 {
                let result = async {
                    let window = host
                        .services
                        .store
                        .ensure_quota_window(quota.id, kind, now)
                        .await?;
                    host.services
                        .store
                        .add_quota_cost(&settlement.request_id, window.id, settlement.cost)
                        .await
                }
                .await;
                match result {
                    Ok(_) => {
                        persisted = true;
                        break;
                    }
                    Err(error) => {
                        tracing::error!(request_id = %settlement.request_id, quota_id = quota.id, error = %error, "persist credential budget failed")
                    }
                }
            }
            if !persisted
                && let Err(error) = host
                    .services
                    .cache
                    .set(&failure_key(quota.id), vec![1], None)
                    .await
            {
                tracing::error!(quota_id = quota.id, error = %error, "trip credential budget accounting failure failed");
            }
        }
    }
}

async fn load(host: &AppHost, request_id: &str) -> Result<Vec<Reservation>, CoreError> {
    Ok(host
        .services
        .cache
        .get(&reservation_key(request_id))
        .await?
        .map(|bytes| serde_json::from_slice(&bytes))
        .transpose()
        .map_err(|error| CoreError::Internal(format!("decode credential reservation: {error}")))?
        .unwrap_or_default())
}

fn reservation_key(request_id: &str) -> String {
    format!("gproxy:credential-admission:{request_id}")
}

fn pending_key(window_id: i64) -> String {
    format!("gproxy:quota-pending:{window_id}")
}

fn failure_key(quota_id: i64) -> String {
    format!("gproxy:credential-budget-failed:{quota_id}")
}

fn store_error(error: gproxy_store::StoreError) -> CoreError {
    CoreError::Store(gproxy_core::error::StoreError(error.to_string()))
}
