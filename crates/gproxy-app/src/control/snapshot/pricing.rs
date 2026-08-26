use std::collections::{BTreeMap, BTreeSet};

use gproxy_core::{Pricing, PricingTier, normalize_service_tier};
use gproxy_store::StoreError;
use gproxy_store::records::{PriceRateRecord, PriceRuleRecord, parse_price_tiers};
use rust_decimal::Decimal;

use super::types::CompiledPriceRule;

pub(super) fn compile(
    rules: &[PriceRuleRecord],
    rates: &[PriceRateRecord],
) -> Result<Vec<CompiledPriceRule>, StoreError> {
    let mut by_rule: BTreeMap<i64, Vec<&PriceRateRecord>> = BTreeMap::new();
    for rate in rates.iter().filter(|rate| rate.conditions.is_none()) {
        by_rule.entry(rate.rule_id).or_default().push(rate);
    }

    let mut compiled = Vec::new();
    for rule in rules.iter().filter(|rule| rule.enabled) {
        let Some(mut candidates) = by_rule.remove(&rule.id) else {
            continue;
        };
        candidates.sort_by_key(|rate| (rate.priority, rate.id));
        let rates = compile_rates(candidates)?;
        compiled.push(CompiledPriceRule {
            id: rule.id,
            provider_id: rule.provider_id,
            model_pattern: rule.model_pattern.clone(),
            priority: rule.priority,
            rates: Pricing {
                tiers: parse_tiers(rule.tiers.as_ref())?,
                ..rates
            },
        });
    }
    compiled.sort_by_key(|rule| (rule.priority, rule.id));
    Ok(compiled)
}

fn compile_rates(rates: Vec<&PriceRateRecord>) -> Result<Pricing, StoreError> {
    let mut input = Decimal::ZERO;
    let mut output = Decimal::ZERO;
    let mut cached = None;
    let mut metric_rates = BTreeMap::new();
    let mut seen = BTreeSet::new();
    for rate in rates {
        if !seen.insert(rate.metric.as_str()) {
            continue;
        }
        if rate.unit_size == 0 {
            return Err(StoreError::InvalidData {
                field: "unit_size",
                message: "price rate unit size must be positive".into(),
            });
        }
        let unit = Decimal::from(rate.unit_size);
        match rate.metric.as_str() {
            "input_tokens" => input = per_million(rate.price, unit),
            "output_tokens" => output = per_million(rate.price, unit),
            "cached_input_tokens" => cached = Some(per_million(rate.price, unit)),
            metric => {
                metric_rates.insert(metric.to_owned(), rate.price / unit);
            }
        }
    }
    Ok(Pricing {
        input_per_million: input,
        output_per_million: output,
        cached_input_per_million: cached,
        service_tier: None,
        tiers: Vec::new(),
        metric_rates,
    })
}

fn parse_tiers(value: Option<&serde_json::Value>) -> Result<Vec<PricingTier>, StoreError> {
    parse_price_tiers(value)?
        .into_iter()
        .map(|tier| {
            Ok(PricingTier {
                service_tier: tier
                    .service_tier
                    .as_deref()
                    .and_then(normalize_service_tier),
                min_prompt_tokens: tier.min_prompt_tokens,
                multiplier: tier.multiplier,
                input_per_million: tier.input,
                output_per_million: tier.output,
                cached_input_per_million: tier.cache_read,
                cache_creation_5m_per_million: tier.cache_creation_5m,
                cache_creation_30m_per_million: tier.cache_creation_30m,
                cache_creation_1h_per_million: tier.cache_creation_1h,
                image_output_per_million: tier.image_output,
            })
        })
        .collect()
}

fn per_million(price: Decimal, unit: Decimal) -> Decimal {
    price * Decimal::from(1_000_000_u64) / unit
}

pub(super) fn resolve(
    rules: &[CompiledPriceRule],
    provider_id: i64,
    model: &str,
) -> Option<Pricing> {
    for scope in [Some(provider_id), None] {
        if let Some(rule) = rules
            .iter()
            .find(|rule| rule.provider_id == scope && glob_matches(&rule.model_pattern, model))
        {
            return Some(rule.rates.clone());
        }
    }
    None
}

fn glob_matches(pattern: &str, value: &str) -> bool {
    let pattern = pattern.as_bytes();
    let value = value.as_bytes();
    let (mut pattern_index, mut value_index) = (0, 0);
    let (mut star, mut retry_value) = (None, 0);
    while value_index < value.len() {
        if pattern.get(pattern_index) == Some(&b'*') {
            star = Some(pattern_index);
            pattern_index += 1;
            retry_value = value_index;
        } else if pattern.get(pattern_index) == value.get(value_index) {
            pattern_index += 1;
            value_index += 1;
        } else if let Some(star_index) = star {
            retry_value += 1;
            value_index = retry_value;
            pattern_index = star_index + 1;
        } else {
            return false;
        }
    }
    while pattern.get(pattern_index) == Some(&b'*') {
        pattern_index += 1;
    }
    pattern_index == pattern.len()
}
