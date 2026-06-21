//! §17 quota pending pre-deduct: an estimated cost is charged to
//! `qp:{scope}:{id}` cache counters at authz time and refunded by the exact
//! same amount at settle (or on the pipeline error path). Cache counters are
//! i64, so cost is stored in MICRO-dollars. A crash between charge and refund
//! self-heals via the 15-minute TTL.

use std::time::Duration;

use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

use crate::app::snapshot::ControlPlaneSnapshot;
use crate::billing::price::{self, Pricing};
use crate::store::cache::{CacheBackend, CounterError};
use crate::store::persistence::records::Scope;
use crate::usage::NormalizedUsage;

/// Pending entries self-heal after 15 minutes if a crash loses the refund.
pub const PENDING_TTL: Duration = Duration::from_secs(15 * 60);

const MICROS: i64 = 1_000_000;

/// Cache key of one scope's in-flight pending cost (micro-dollars).
pub fn key(scope: Scope, scope_id: i64) -> String {
    format!("qp:{}:{}", scope.as_str(), scope_id)
}

/// Decimal dollars → integer micro-dollars (rounded).
pub fn to_micros(cost: Decimal) -> i64 {
    (cost * Decimal::from(MICROS)).round().to_i64().unwrap_or(0)
}

/// Integer micro-dollars → Decimal dollars.
pub fn micros_to_cost(micros: i64) -> Decimal {
    Decimal::from(micros) / Decimal::from(MICROS)
}

/// Pricing of `model_id` on `provider_id`; default (all-zero) when the model
/// or its `pricing_json` is absent.
pub fn model_pricing(cp: &ControlPlaneSnapshot, provider_id: i64, model_id: &str) -> Pricing {
    cp.models_by_provider
        .get(&provider_id)
        .and_then(|ms| ms.iter().find(|m| m.model_id == model_id))
        .map(|m| price::pricing_from(m.pricing_json.as_ref()))
        .unwrap_or_default()
}

/// Best-effort request estimate in micro-dollars: estimated tokens = full
/// body char count ×1, priced as input tokens. Absent/zero pricing → 0
/// (pre-deduct is skipped entirely).
pub fn estimate_micros(pricing: &Pricing, body_len: usize) -> i64 {
    let est = NormalizedUsage {
        input: body_len as u64,
        ..Default::default()
    };
    to_micros(price::cost(&est, pricing))
}

/// Read one scope's pending total (creates the key at 0 with TTL if absent).
/// Backend failure propagates — the quota gate fails closed on it.
pub async fn read(
    cache: &dyn CacheBackend,
    scope: Scope,
    scope_id: i64,
) -> Result<i64, CounterError> {
    cache
        .incr(&key(scope, scope_id), 0, Some(PENDING_TTL))
        .await
}

/// Pre-deduct `micros` on every quota-bearing scope.
pub async fn charge(cache: &dyn CacheBackend, scopes: &[(Scope, i64)], micros: i64) {
    adjust(cache, scopes, micros).await;
}

/// Refund the exact pre-deducted amount (never recomputed).
pub async fn refund(cache: &dyn CacheBackend, scopes: &[(Scope, i64)], micros: i64) {
    adjust(cache, scopes, -micros).await;
}

/// Best-effort: a failed adjust is logged by the backend and self-heals via
/// the pending TTL (admission already failed closed if the backend is down).
async fn adjust(cache: &dyn CacheBackend, scopes: &[(Scope, i64)], delta: i64) {
    if delta == 0 {
        return;
    }
    for &(scope, scope_id) in scopes {
        let _ = cache
            .incr(&key(scope, scope_id), delta, Some(PENDING_TTL))
            .await;
    }
}
