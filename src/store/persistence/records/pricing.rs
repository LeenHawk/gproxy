//! Pricing rules. A rule is either provider-scoped (`provider_id = Some`) or
//! global (`provider_id = None`) and matches a model by exact id or substring.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// One independently stored pricing dimension. `price_usd` applies per
/// `unit_size` units; optional conditions select service/media variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PriceRate {
    pub metric: String,
    pub unit: String,
    #[serde(default = "default_unit_size")]
    pub unit_size: u64,
    #[serde(with = "rust_decimal::serde::str")]
    pub price_usd: Decimal,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conditions_json: Option<serde_json::Value>,
    #[serde(default)]
    pub sort_order: i64,
}

const fn default_unit_size() -> u64 {
    1
}

/// One pricing rule used by the billing resolver.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceRule {
    pub id: i64,
    pub provider_id: Option<i64>,
    /// `exact` | `contains`.
    pub match_type: String,
    pub model_match: String,
    /// Per-million input-token price.
    #[serde(with = "rust_decimal::serde::str")]
    pub input_price: Decimal,
    /// Per-million output-token price.
    #[serde(with = "rust_decimal::serde::str")]
    pub output_price: Decimal,
    /// Per-million cache-read-token price.
    #[serde(with = "rust_decimal::serde::str")]
    pub cache_read_price: Decimal,
    /// Per-million 5-minute cache-creation-token price.
    #[serde(with = "rust_decimal::serde::str")]
    pub cache_creation_5m_price: Decimal,
    /// Per-million 30-minute cache-creation-token price.
    #[serde(default, with = "rust_decimal::serde::str")]
    pub cache_creation_30m_price: Decimal,
    /// Per-million 1-hour cache-creation-token price.
    #[serde(with = "rust_decimal::serde::str")]
    pub cache_creation_1h_price: Decimal,
    /// Per-million image-output-token price.
    #[serde(with = "rust_decimal::serde::str")]
    pub image_output_price: Decimal,
    /// Ordered pricing tiers. Entries can select by `min_prompt_tokens`, by
    /// `service_tier`, or both, with an optional `multiplier` and per-million
    /// rate overrides using the same `*_price` names as this row.
    #[serde(default)]
    pub pricing_tiers_json: Option<serde_json::Value>,
    #[serde(default)]
    pub rates: Vec<PriceRate>,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Upsert input for a pricing rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PriceRuleInput {
    pub id: Option<i64>,
    pub provider_id: Option<i64>,
    pub match_type: String,
    pub model_match: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub input_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub output_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub cache_read_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub cache_creation_5m_price: Decimal,
    #[serde(default, with = "rust_decimal::serde::str")]
    pub cache_creation_30m_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub cache_creation_1h_price: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub image_output_price: Decimal,
    #[serde(default)]
    pub pricing_tiers_json: Option<serde_json::Value>,
    #[serde(default)]
    pub rates: Vec<PriceRate>,
    pub enabled: bool,
}

impl PriceRuleInput {
    pub fn validate_rates(&self) -> anyhow::Result<()> {
        for rate in &self.rates {
            if rate.metric.trim().is_empty() || rate.unit.trim().is_empty() {
                anyhow::bail!("price rate metric and unit are required");
            }
            if rate.unit_size == 0 {
                anyhow::bail!("price rate unit_size must be positive");
            }
            if rate.price_usd.is_sign_negative() {
                anyhow::bail!("price rate price_usd must not be negative");
            }
            if rate
                .conditions_json
                .as_ref()
                .is_some_and(|conditions| !conditions.is_object())
            {
                anyhow::bail!("price rate conditions_json must be an object");
            }
        }
        Ok(())
    }

    pub fn effective_rates(&self) -> Vec<PriceRate> {
        if !self.rates.is_empty() {
            return self.rates.clone();
        }
        legacy_token_rates(
            self.input_price,
            self.output_price,
            self.cache_read_price,
            self.cache_creation_5m_price,
            self.cache_creation_30m_price,
            self.cache_creation_1h_price,
            self.image_output_price,
        )
    }
}

impl PriceRule {
    pub fn apply_rate_projections(&mut self) {
        let projected = |metric: &str, fallback: Decimal| {
            self.rates
                .iter()
                .enumerate()
                .filter(|(_, rate)| rate.metric == metric && rate.conditions_json.is_none())
                .max_by_key(|(index, rate)| (rate.sort_order, *index))
                .map(|(_, rate)| {
                    rate.price_usd * Decimal::from(1_000_000u64)
                        / Decimal::from(rate.unit_size.max(1))
                })
                .unwrap_or(fallback)
        };
        self.input_price = projected("input_tokens", self.input_price);
        self.output_price = projected("output_tokens", self.output_price);
        self.cache_read_price = projected("cache_read_tokens", self.cache_read_price);
        self.cache_creation_5m_price =
            projected("cache_creation_5m_tokens", self.cache_creation_5m_price);
        self.cache_creation_30m_price =
            projected("cache_creation_30m_tokens", self.cache_creation_30m_price);
        self.cache_creation_1h_price =
            projected("cache_creation_1h_tokens", self.cache_creation_1h_price);
        self.image_output_price = projected("image_output_tokens", self.image_output_price);
    }

    pub fn effective_rates(&self) -> Vec<PriceRate> {
        if !self.rates.is_empty() {
            return self.rates.clone();
        }
        legacy_token_rates(
            self.input_price,
            self.output_price,
            self.cache_read_price,
            self.cache_creation_5m_price,
            self.cache_creation_30m_price,
            self.cache_creation_1h_price,
            self.image_output_price,
        )
    }
}

fn legacy_token_rates(
    input: Decimal,
    output: Decimal,
    cache_read: Decimal,
    cache_creation_5m: Decimal,
    cache_creation_30m: Decimal,
    cache_creation_1h: Decimal,
    image_output: Decimal,
) -> Vec<PriceRate> {
    [
        ("input_tokens", input),
        ("output_tokens", output),
        ("cache_read_tokens", cache_read),
        ("cache_creation_5m_tokens", cache_creation_5m),
        ("cache_creation_30m_tokens", cache_creation_30m),
        ("cache_creation_1h_tokens", cache_creation_1h),
        ("image_output_tokens", image_output),
    ]
    .into_iter()
    .enumerate()
    .map(|(sort_order, (metric, price_usd))| PriceRate {
        metric: metric.into(),
        unit: "token".into(),
        unit_size: 1_000_000,
        price_usd,
        conditions_json: None,
        sort_order: sort_order as i64,
    })
    .collect()
}
