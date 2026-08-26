use std::collections::BTreeMap;

use rust_decimal::Decimal;

use super::service_tier::{normalize_service_tier, request_service_tier};
use crate::usage::NormalizedUsage;

/// Per-model rates for settlement. Token categories are per million;
/// dimensional rates are per unit.
#[derive(Debug, Clone)]
pub struct Pricing {
    pub input_per_million: Decimal,
    pub output_per_million: Decimal,
    pub cached_input_per_million: Option<Decimal>,
    pub service_tier: Option<String>,
    pub tiers: Vec<PricingTier>,
    pub metric_rates: BTreeMap<String, Decimal>,
    pub conditional_metric_rates: BTreeMap<String, Vec<ConditionalMetricRate>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConditionalMetricRate {
    pub rate_per_unit: Decimal,
    pub conditions: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PricingTier {
    pub service_tier: Option<String>,
    pub min_prompt_tokens: u64,
    pub multiplier: Option<Decimal>,
    pub input_per_million: Option<Decimal>,
    pub output_per_million: Option<Decimal>,
    pub cached_input_per_million: Option<Decimal>,
    pub cache_creation_5m_per_million: Option<Decimal>,
    pub cache_creation_30m_per_million: Option<Decimal>,
    pub cache_creation_1h_per_million: Option<Decimal>,
    pub image_output_per_million: Option<Decimal>,
}

impl Pricing {
    pub fn for_request(mut self, body: &[u8]) -> Self {
        self.service_tier = request_service_tier(body);
        self
    }

    pub fn with_service_tier(mut self, service_tier: &str) -> Self {
        self.service_tier = normalize_service_tier(service_tier);
        self
    }

    pub fn cost(&self, usage: &NormalizedUsage) -> Decimal {
        let million = Decimal::from(1_000_000_u64);
        let cached = usage.cached_input_tokens.min(usage.input_tokens);
        let uncached = usage.input_tokens - cached;
        let prompt_tokens = prompt_tokens(usage);
        let prompt_tier = self
            .tiers
            .iter()
            .enumerate()
            .filter(|(_, tier)| {
                tier.service_tier.is_none()
                    && prompt_tokens >= Decimal::from(tier.min_prompt_tokens)
            })
            .max_by_key(|(index, tier)| (tier.min_prompt_tokens, *index));
        let actual = usage
            .dimensions
            .get("speed")
            .or_else(|| usage.dimensions.get("service_tier"))
            .and_then(|value| normalize_service_tier(value));
        let selected_tier = actual.as_deref().or(self.service_tier.as_deref());
        let service_tier = selected_tier.and_then(|selected| {
            self.tiers
                .iter()
                .enumerate()
                .filter(|(_, tier)| {
                    tier.service_tier
                        .as_deref()
                        .and_then(normalize_service_tier)
                        .as_deref()
                        == Some(selected)
                        && prompt_tokens >= Decimal::from(tier.min_prompt_tokens)
                })
                .max_by_key(|(index, tier)| (tier.min_prompt_tokens, *index))
        });
        let price = |base, select: fn(&PricingTier) -> Option<Decimal>| {
            let prompt = prompt_tier
                .and_then(|(_, tier)| select(tier))
                .unwrap_or(base);
            service_tier
                .and_then(|(_, tier)| select(tier))
                .unwrap_or_else(|| {
                    prompt
                        * service_tier
                            .and_then(|(_, tier)| tier.multiplier)
                            .unwrap_or(Decimal::ONE)
                })
        };
        let input = self
            .metric_rate("input_tokens", usage)
            .map(|rate| rate * million)
            .unwrap_or(self.input_per_million);
        let cached_input = self
            .metric_rate("cached_input_tokens", usage)
            .map(|rate| rate * million)
            .or(self.cached_input_per_million)
            .unwrap_or(input);
        let output = self
            .metric_rate("output_tokens", usage)
            .map(|rate| rate * million)
            .unwrap_or(self.output_per_million);
        let mut total =
            Decimal::from(uncached) * price(input, |tier| tier.input_per_million) / million;
        total += Decimal::from(cached) * price(cached_input, |tier| tier.cached_input_per_million)
            / million;
        total += Decimal::from(usage.output_tokens) * price(output, |tier| tier.output_per_million)
            / million;
        for (metric, select) in tiered_metrics() {
            let Some(amount) = usage.metrics.get(metric) else {
                continue;
            };
            let base = self.metric_rate(metric, usage).unwrap_or_default() * million;
            total += *amount * price(base, select) / million;
        }
        for (metric, amount) in &usage.metrics {
            if tiered_metrics().iter().any(|(name, _)| metric == name) {
                continue;
            }
            if let Some(rate) = self.metric_rate(metric, usage) {
                total += *amount * rate;
            }
        }
        total
    }

    fn metric_rate(&self, metric: &str, usage: &NormalizedUsage) -> Option<Decimal> {
        self.conditional_metric_rates
            .get(metric)
            .and_then(|rates| {
                rates.iter().find(|rate| {
                    rate.conditions
                        .iter()
                        .all(|(name, expected)| usage.dimensions.get(name) == Some(expected))
                })
            })
            .map(|rate| rate.rate_per_unit)
            .or_else(|| self.metric_rates.get(metric).copied())
    }
}

type TierRate = fn(&PricingTier) -> Option<Decimal>;

fn tiered_metrics() -> [(&'static str, TierRate); 4] {
    [
        ("cache_creation_5m_tokens", |tier| {
            tier.cache_creation_5m_per_million
        }),
        ("cache_creation_30m_tokens", |tier| {
            tier.cache_creation_30m_per_million
        }),
        ("cache_creation_1h_tokens", |tier| {
            tier.cache_creation_1h_per_million
        }),
        ("image_output_tokens", |tier| tier.image_output_per_million),
    ]
}

fn prompt_tokens(usage: &NormalizedUsage) -> Decimal {
    [
        "cache_creation_5m_tokens",
        "cache_creation_30m_tokens",
        "cache_creation_1h_tokens",
    ]
    .into_iter()
    .filter_map(|name| usage.metrics.get(name))
    .fold(Decimal::from(usage.input_tokens), |total, value| {
        total + value
    })
}
