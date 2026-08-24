use gproxy_channel_api::CallerIdentity;
use gproxy_core::{CacheBackend, ControlPlane, CoreError, NormalizedUsage, Plan, RequestCtx};
use gproxy_protocol::{OperationKey, SettleMode};
use gproxy_store::records::{QuotaRecord, QuotaWindowKind};

use super::super::AppHost;
use super::auth::subject_matches;
use super::types::{CounterCharge, QuotaReservation};

pub(super) async fn reserve(
    host: &AppHost,
    identity: &CallerIdentity,
    request: &RequestCtx,
    operation: Option<OperationKey>,
    plan: &Plan,
    now: i64,
    charged: &mut Vec<CounterCharge>,
) -> Result<Vec<QuotaReservation>, CoreError> {
    if !operation.is_some_and(|key| key.operation.spec().settle != SettleMode::Free) {
        return Ok(Vec::new());
    }
    let estimate = estimated_cost_micros(host, request, plan)?;
    let mut reservations = Vec::new();
    let snapshot = host.services.control.current();
    for quota in snapshot
        .quotas
        .iter()
        .filter(|quota| subject_matches(&quota.subject_kind, quota.subject_id, identity))
    {
        for (kind, limit) in limits(quota) {
            let window = host
                .services
                .store
                .ensure_quota_window(quota.id, kind, now)
                .await
                .map_err(|error| {
                    CoreError::Store(gproxy_core::error::StoreError(error.to_string()))
                })?;
            let key = format!("gproxy:quota-pending:{}", window.id);
            let pending = host.services.cache.incr(&key, estimate, None).await?;
            charged.push(CounterCharge {
                key: key.clone(),
                amount: estimate,
            });
            let live = host
                .services
                .store
                .quota_window(window.id)
                .await
                .map_err(store_error)?
                .ok_or_else(|| {
                    CoreError::Store(gproxy_core::error::StoreError(
                        "quota window vanished after reservation".into(),
                    ))
                })?;
            let before = pending.saturating_sub(estimate).max(0);
            let projected = pending.max(0);
            let exhausted = live.cost_used + gproxy_core::usage::micros_to_cost(before) >= limit;
            let exceeds = live.cost_used + gproxy_core::usage::micros_to_cost(projected) > limit;
            if exhausted || exceeds {
                return Err(CoreError::QuotaExceeded);
            }
            reservations.push(QuotaReservation {
                window_id: window.id,
                cache_key: key,
                estimated_cost_micros: estimate,
                cost_recorded: false,
                released: false,
            });
        }
    }
    Ok(reservations)
}

fn store_error(error: gproxy_store::StoreError) -> CoreError {
    CoreError::Store(gproxy_core::error::StoreError(error.to_string()))
}

fn estimated_cost_micros(
    host: &AppHost,
    request: &RequestCtx,
    plan: &Plan,
) -> Result<i64, CoreError> {
    let usage = NormalizedUsage {
        input_tokens: gproxy_core::usage::estimate_input_tokens(&request.body),
        ..Default::default()
    };
    let cost = plan
        .targets
        .iter()
        .filter_map(|target| {
            host.services
                .control
                .pricing(&target.provider, &target.upstream_model)
        })
        .map(|pricing| pricing.cost(&usage))
        .max()
        .unwrap_or_default();
    gproxy_core::usage::cost_to_micros(cost)
        .ok_or_else(|| CoreError::Internal("admission cost estimate exceeds counter".into()))
}

fn limits(quota: &QuotaRecord) -> impl Iterator<Item = (QuotaWindowKind, rust_decimal::Decimal)> {
    [
        Some((QuotaWindowKind::Total, quota.quota_total)),
        quota
            .quota_daily
            .map(|limit| (QuotaWindowKind::Daily, limit)),
        quota
            .quota_weekly
            .map(|limit| (QuotaWindowKind::Weekly, limit)),
        quota
            .quota_monthly
            .map(|limit| (QuotaWindowKind::Monthly, limit)),
        quota
            .quota_5h
            .map(|limit| (QuotaWindowKind::FiveHour, limit)),
        quota
            .quota_7d
            .map(|limit| (QuotaWindowKind::SevenDay, limit)),
    ]
    .into_iter()
    .flatten()
}
