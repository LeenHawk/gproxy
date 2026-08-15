//! Pricing rules. A rule is either provider-scoped (`provider_id = Some`) or
//! global (`provider_id = None`) and matches a model by exact id or substring.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

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
    pub enabled: bool,
}
