use gproxy_channel_api::CallerIdentity;
use gproxy_core::{
    CacheBackend, ControlPlane, CoreError, NormalizedUsage, Plan, Pricing, RequestCtx,
};
use gproxy_protocol::{OperationKey, SettleMode};

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
    if !operation.is_some_and(|key| key.operation().spec().settle != SettleMode::Free) {
        return Ok(Vec::new());
    }
    let estimate = estimated_cost_micros(host, request, plan).await?;
    reserve_cost(host, identity, estimate, now, charged).await
}

pub(super) async fn reserve_retry(
    host: &AppHost,
    identity: &CallerIdentity,
    body: &bytes::Bytes,
    target: &gproxy_core::Target,
    charged: &mut Vec<CounterCharge>,
) -> Result<Vec<QuotaReservation>, CoreError> {
    let estimate = estimated_target_cost_micros(host, body, target).await?;
    reserve_cost(host, identity, estimate, super::auth::unix_now(), charged).await
}

async fn reserve_cost(
    host: &AppHost,
    identity: &CallerIdentity,
    estimate: i64,
    now: i64,
    charged: &mut Vec<CounterCharge>,
) -> Result<Vec<QuotaReservation>, CoreError> {
    let mut reservations = Vec::new();
    let snapshot = host.services.control.current();
    for quota in snapshot.quotas.iter().filter(|quota| {
        quota.enabled && subject_matches(&quota.subject_kind, quota.subject_id, identity)
    }) {
        for (kind, limit) in quota.limits() {
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

async fn estimated_cost_micros(
    host: &AppHost,
    request: &RequestCtx,
    plan: &Plan,
) -> Result<i64, CoreError> {
    let mut seen = std::collections::BTreeSet::new();
    let candidates = plan
        .targets
        .iter()
        .filter(|target| seen.insert((target.provider.id, target.upstream_model.clone())))
        .filter_map(|target| candidate(host, target))
        .collect::<Vec<_>>();
    estimate_micros(host, &request.body, candidates).await
}

/// The cost this request would settle at on one target, in micro-units.
pub(super) async fn estimated_target_cost_micros(
    host: &AppHost,
    body: &bytes::Bytes,
    target: &gproxy_core::Target,
) -> Result<i64, CoreError> {
    let candidates = candidate(host, target).into_iter().collect();
    estimate_micros(host, body, candidates).await
}

fn candidate(
    host: &AppHost,
    target: &gproxy_core::Target,
) -> Option<(String, Option<serde_json::Value>, Pricing)> {
    let pricing = host
        .services
        .control
        .pricing(&target.provider, &target.upstream_model)?;
    Some((
        target.upstream_model.clone(),
        target.provider.settings.get("tokenizer_map").cloned(),
        pricing,
    ))
}

async fn estimate_micros(
    host: &AppHost,
    body: &bytes::Bytes,
    candidates: Vec<(String, Option<serde_json::Value>, Pricing)>,
) -> Result<i64, CoreError> {
    let cost = host
        .maximum_candidate_cost(body.clone(), candidates)
        .await?;
    gproxy_core::usage::cost_to_micros(cost)
        .ok_or_else(|| CoreError::Internal("admission cost estimate exceeds counter".into()))
}

impl AppHost {
    /// Tokenizing blocks, so a native host hands it to the blocking pool; the
    /// edge host has no vocabularies and no pool to hand it to.
    async fn maximum_candidate_cost(
        &self,
        body: bytes::Bytes,
        candidates: Vec<(String, Option<serde_json::Value>, Pricing)>,
    ) -> Result<rust_decimal::Decimal, CoreError> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            let registry = self.services.tokenizers.clone();
            tokio::task::spawn_blocking(move || maximum_cost(&body, &candidates, &registry))
                .await
                .map_err(|error| CoreError::Internal(format!("tokenizer task failed: {error}")))
        }
        #[cfg(target_arch = "wasm32")]
        {
            Ok(maximum_cost(&body, &candidates, ()))
        }
    }
}

fn maximum_cost(
    body: &[u8],
    candidates: &[(String, Option<serde_json::Value>, Pricing)],
    registry: gproxy_tokenize::RegistryHandle<'_>,
) -> rust_decimal::Decimal {
    candidates
        .iter()
        .map(|(model, map, pricing)| {
            let usage = NormalizedUsage {
                input_tokens: gproxy_tokenize::count(model, body, map.as_ref(), registry),
                ..Default::default()
            };
            pricing.clone().for_request(body).cost(&usage)
        })
        .max()
        .unwrap_or_default()
}
