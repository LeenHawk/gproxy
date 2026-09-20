//! Pricing: which rule covers a model, what each billable quantity costs and
//! how long-context or service-tier prices override it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::double_option;
use gproxy_store::entity::pricing::{price_rate, price_rule, price_tier, price_unit::PriceUnit};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceRuleDto {
    pub id: String,
    /// None prices every provider; a provider rule wins over a global one.
    pub provider_id: Option<String>,
    /// A `*`/`?` glob over the upstream model name.
    pub model_pattern: String,
    /// None covers every operation of the matched model.
    pub operation: Option<String>,
    pub priority: i32,
    /// ISO 4217, three letters.
    pub currency: String,
    pub enabled: bool,
}

impl From<price_rule::Model> for PriceRuleDto {
    fn from(row: price_rule::Model) -> Self {
        Self {
            id: row.id,
            provider_id: row.provider_id,
            model_pattern: row.model_pattern,
            operation: row.operation,
            priority: row.priority,
            currency: row.currency,
            enabled: row.enabled,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceRuleWrite {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub provider_id: Option<String>,
    pub model_pattern: String,
    #[serde(default)]
    pub operation: Option<String>,
    #[serde(default)]
    pub priority: Option<i32>,
    pub currency: String,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceRulePatch {
    #[serde(default, deserialize_with = "double_option")]
    pub provider_id: Option<Option<String>>,
    #[serde(default)]
    pub model_pattern: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub operation: Option<Option<String>>,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

/// One billable quantity's price. `value` buys `unit_quantity` units, so a
/// per-million token price is `unit_quantity = 1000000`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceRateDto {
    pub id: String,
    pub price_rule_id: String,
    /// A `gproxy_store::entity::pricing::metric` key, or a custom one.
    pub metric: String,
    /// `token`, `count`, `second` or `character`.
    pub unit: String,
    pub unit_quantity: String,
    pub value: String,
    /// None is the fallback rate; otherwise a nonempty object of dimensions.
    pub conditions: Option<Value>,
    pub priority: i32,
}

impl From<price_rate::Model> for PriceRateDto {
    fn from(row: price_rate::Model) -> Self {
        Self {
            id: row.id,
            price_rule_id: row.price_rule_id,
            metric: row.metric,
            unit: unit_name(row.unit).to_owned(),
            unit_quantity: row.unit_quantity.to_string(),
            value: row.value.to_string(),
            conditions: row.conditions,
            priority: row.priority,
        }
    }
}

pub(crate) fn unit_name(unit: PriceUnit) -> &'static str {
    match unit {
        PriceUnit::Token => "token",
        PriceUnit::Count => "count",
        PriceUnit::Second => "second",
        PriceUnit::Character => "character",
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceRateWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub price_rule_id: String,
    pub metric: String,
    pub unit: String,
    pub unit_quantity: String,
    pub value: String,
    #[serde(default)]
    pub conditions: Option<Value>,
    #[serde(default)]
    pub priority: Option<i32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceRatePatch {
    #[serde(default)]
    pub metric: Option<String>,
    #[serde(default)]
    pub unit: Option<String>,
    #[serde(default)]
    pub unit_quantity: Option<String>,
    #[serde(default)]
    pub value: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    pub conditions: Option<Option<Value>>,
    #[serde(default)]
    pub priority: Option<i32>,
}

/// A context or service-tier override inside one rule. Every per-million
/// price is optional: None inherits, zero is explicitly free.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceTierDto {
    pub id: String,
    pub price_rule_id: String,
    /// None is a context tier; otherwise `standard`, `priority`, `flex`, … .
    pub service_tier: Option<String>,
    pub min_prompt_tokens: i64,
    pub priority: i32,
    pub multiplier: Option<String>,
    pub input_per_million: Option<String>,
    pub output_per_million: Option<String>,
    pub cache_read_per_million: Option<String>,
    pub cache_creation_5m_per_million: Option<String>,
    pub cache_creation_30m_per_million: Option<String>,
    pub cache_creation_1h_per_million: Option<String>,
    pub reasoning_per_million: Option<String>,
    pub image_input_per_million: Option<String>,
    pub image_output_per_million: Option<String>,
    pub audio_input_per_million: Option<String>,
    pub cached_audio_input_per_million: Option<String>,
    pub audio_output_per_million: Option<String>,
    pub video_input_per_million: Option<String>,
    pub video_per_million: Option<String>,
}

impl From<price_tier::Model> for PriceTierDto {
    fn from(row: price_tier::Model) -> Self {
        let text = |value: Option<gproxy_store::FixedDecimal>| value.map(|v| v.to_string());
        Self {
            id: row.id,
            price_rule_id: row.price_rule_id,
            service_tier: row.service_tier,
            min_prompt_tokens: row.min_prompt_tokens,
            priority: row.priority,
            multiplier: text(row.multiplier),
            input_per_million: text(row.input_per_million),
            output_per_million: text(row.output_per_million),
            cache_read_per_million: text(row.cache_read_per_million),
            cache_creation_5m_per_million: text(row.cache_creation_5m_per_million),
            cache_creation_30m_per_million: text(row.cache_creation_30m_per_million),
            cache_creation_1h_per_million: text(row.cache_creation_1h_per_million),
            reasoning_per_million: text(row.reasoning_per_million),
            image_input_per_million: text(row.image_input_per_million),
            image_output_per_million: text(row.image_output_per_million),
            audio_input_per_million: text(row.audio_input_per_million),
            cached_audio_input_per_million: text(row.cached_audio_input_per_million),
            audio_output_per_million: text(row.audio_output_per_million),
            video_input_per_million: text(row.video_input_per_million),
            video_per_million: text(row.video_per_million),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceTierWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub price_rule_id: String,
    #[serde(default)]
    pub service_tier: Option<String>,
    #[serde(default)]
    pub min_prompt_tokens: Option<i64>,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default)]
    pub multiplier: Option<String>,
    #[serde(default)]
    pub input_per_million: Option<String>,
    #[serde(default)]
    pub output_per_million: Option<String>,
    #[serde(default)]
    pub cache_read_per_million: Option<String>,
    #[serde(default)]
    pub cache_creation_5m_per_million: Option<String>,
    #[serde(default)]
    pub cache_creation_30m_per_million: Option<String>,
    #[serde(default)]
    pub cache_creation_1h_per_million: Option<String>,
    #[serde(default)]
    pub reasoning_per_million: Option<String>,
    #[serde(default)]
    pub image_input_per_million: Option<String>,
    #[serde(default)]
    pub image_output_per_million: Option<String>,
    #[serde(default)]
    pub audio_input_per_million: Option<String>,
    #[serde(default)]
    pub cached_audio_input_per_million: Option<String>,
    #[serde(default)]
    pub audio_output_per_million: Option<String>,
    #[serde(default)]
    pub video_input_per_million: Option<String>,
    #[serde(default)]
    pub video_per_million: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceTierPatch {
    #[serde(default, deserialize_with = "double_option")]
    pub service_tier: Option<Option<String>>,
    #[serde(default)]
    pub min_prompt_tokens: Option<i64>,
    #[serde(default)]
    pub priority: Option<i32>,
    #[serde(default, deserialize_with = "double_option")]
    pub multiplier: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub input_per_million: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub output_per_million: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub cache_read_per_million: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub cache_creation_5m_per_million: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub cache_creation_30m_per_million: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub cache_creation_1h_per_million: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub reasoning_per_million: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub image_input_per_million: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub image_output_per_million: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub audio_input_per_million: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub cached_audio_input_per_million: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub audio_output_per_million: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub video_input_per_million: Option<Option<String>>,
    #[serde(default, deserialize_with = "double_option")]
    pub video_per_million: Option<Option<String>>,
}
