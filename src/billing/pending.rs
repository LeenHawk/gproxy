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

/// Resolved pricing and the matching rule id.
#[derive(Debug, Clone, Default)]
pub struct ResolvedPricing {
    pub pricing: Pricing,
    pub rule_id: Option<i64>,
}

/// Pricing of `model_id` on `provider_id`; default (all-zero) when no price
/// rule matches.
pub fn model_pricing(cp: &ControlPlaneSnapshot, provider_id: i64, model_id: &str) -> Pricing {
    resolve_pricing(cp, provider_id, model_id).pricing
}

pub fn resolve_pricing(
    cp: &ControlPlaneSnapshot,
    provider_id: i64,
    model_id: &str,
) -> ResolvedPricing {
    if let Some(rule) = cp
        .price_rules
        .iter()
        .filter_map(|rule| match_rank(rule, provider_id, model_id))
        .min_by_key(|(_, key)| *key)
        .map(|(rule, _)| rule)
    {
        return ResolvedPricing {
            pricing: price::pricing_from_rule(rule),
            rule_id: Some(rule.id),
        };
    }

    ResolvedPricing {
        pricing: Pricing::default(),
        rule_id: None,
    }
}

/// Sort key implements the four agreed ranks:
/// provider exact → global exact → provider contains → global contains.
/// Within a rank, longer model fragments win, then older id for deterministic
/// ties.
fn match_rank<'a>(
    rule: &'a crate::store::persistence::records::PriceRule,
    provider_id: i64,
    model_id: &str,
) -> Option<(
    &'a crate::store::persistence::records::PriceRule,
    (i64, i64, i64),
)> {
    if !rule.enabled {
        return None;
    }
    if let Some(rule_provider) = rule.provider_id
        && rule_provider != provider_id
    {
        return None;
    }

    let provider_rank = if rule.provider_id.is_some() { 0 } else { 1 };
    let match_rank = match rule.match_type.as_str() {
        "exact" if rule.model_match == model_id => 0,
        "contains" if model_id.contains(&rule.model_match) => 2,
        _ => return None,
    };
    let rank = match_rank + provider_rank;
    Some((rule, (rank, -(rule.model_match.len() as i64), rule.id)))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use crate::store::persistence::records::PriceRule;

    fn rule(
        id: i64,
        provider_id: Option<i64>,
        match_type: &str,
        model_match: &str,
        input_rate: &str,
    ) -> PriceRule {
        PriceRule {
            id,
            provider_id,
            match_type: match_type.into(),
            model_match: model_match.into(),
            input_price: input_rate.parse().unwrap(),
            output_price: Decimal::ZERO,
            cache_read_price: Decimal::ZERO,
            cache_creation_5m_price: Decimal::ZERO,
            cache_creation_1h_price: Decimal::ZERO,
            image_price: Decimal::ZERO,
            enabled: true,
            created_at: id,
            updated_at: id,
        }
    }

    fn decimal(s: &str) -> Decimal {
        s.parse().unwrap()
    }

    #[test]
    fn price_rule_resolver_uses_scope_match_rank_and_longest_fragment() {
        let mut cp = ControlPlaneSnapshot::empty(1);
        cp.price_rules = Arc::new(vec![
            rule(1, None, "contains", "gpt", "40"),
            rule(2, Some(7), "contains", "gpt", "30"),
            rule(3, None, "exact", "gpt-4o", "20"),
            rule(4, Some(7), "exact", "gpt-4o", "10"),
            rule(5, Some(7), "contains", "claude", "50"),
            rule(6, None, "exact", "claude-3", "60"),
            rule(7, Some(7), "contains", "claude-sonnet-4", "70"),
            rule(8, Some(7), "contains", "claude-sonnet-4.5", "80"),
        ]);

        let resolved = resolve_pricing(&cp, 7, "gpt-4o");
        assert_eq!(resolved.rule_id, Some(4)); // provider exact
        assert_eq!(resolved.pricing.input, decimal("10"));

        let resolved = resolve_pricing(&cp, 7, "claude-3");
        assert_eq!(resolved.rule_id, Some(6)); // global exact beats provider contains
        assert_eq!(resolved.pricing.input, decimal("60"));

        let resolved = resolve_pricing(&cp, 7, "my-gpt-test");
        assert_eq!(resolved.rule_id, Some(2)); // provider contains beats global contains
        assert_eq!(resolved.pricing.input, decimal("30"));

        let resolved = resolve_pricing(&cp, 8, "my-gpt-test");
        assert_eq!(resolved.rule_id, Some(1)); // global contains when provider rule misses
        assert_eq!(resolved.pricing.input, decimal("40"));

        let resolved = resolve_pricing(&cp, 7, "claude-sonnet-4.5-20250929");
        assert_eq!(resolved.rule_id, Some(8)); // longest provider contains
        assert_eq!(resolved.pricing.input, decimal("80"));
    }

    #[test]
    fn unmatched_price_rule_resolves_to_zero_pricing() {
        let mut cp = ControlPlaneSnapshot::empty(1);
        cp.price_rules = Arc::new(vec![rule(1, Some(7), "exact", "gpt-4o", "10")]);

        let resolved = resolve_pricing(&cp, 8, "other-model");
        assert_eq!(resolved.rule_id, None);
        assert_eq!(resolved.pricing, Pricing::default());
    }
}
