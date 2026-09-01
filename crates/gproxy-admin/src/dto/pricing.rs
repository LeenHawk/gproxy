use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PriceRuleDto {
    pub id: i64,
    pub provider_id: Option<i64>,
    pub model_pattern: String,
    #[ts(type = "unknown | null")]
    pub tiers: Option<Value>,
    pub priority: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PriceRuleWriteRequest {
    pub provider_id: Option<i64>,
    pub model_pattern: String,
    #[ts(type = "unknown | null")]
    pub tiers: Option<Value>,
    pub priority: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PriceRateDto {
    pub id: i64,
    pub rule_id: i64,
    pub metric: String,
    pub unit_size: u64,
    pub price: String,
    #[ts(type = "unknown | null")]
    pub conditions: Option<Value>,
    pub priority: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PriceRateWriteRequest {
    pub rule_id: i64,
    pub metric: String,
    pub unit_size: u64,
    pub price: String,
    #[ts(type = "unknown | null")]
    pub conditions: Option<Value>,
    pub priority: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DefaultPriceCatalogDto {
    pub schema_version: u32,
    pub source: DefaultPriceCatalogSourceDto,
    pub price_rules: Vec<DefaultPriceRuleDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DefaultPriceCatalogSourceDto {
    pub catalog: String,
    pub fetched_at: String,
    pub total_models: usize,
    pub supported_output_models: usize,
    pub dynamic_price_models: usize,
    pub included_models: usize,
    pub embedding_models: usize,
    pub rerank_models: usize,
    pub image_output_priced_models: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DefaultPriceRuleDto {
    pub model_id: String,
    pub model_pattern: String,
    #[ts(type = "unknown | null")]
    pub tiers: Option<Value>,
    pub priority: i64,
    pub rates: Vec<DefaultPriceRateDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DefaultPriceRateDto {
    pub metric: String,
    pub unit_size: u64,
    pub price: String,
    pub priority: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ApplyDefaultPricesRequest {
    pub provider_id: i64,
    pub model_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ApplyDefaultPricesResponse {
    pub created: usize,
    pub skipped: usize,
    pub unmatched: usize,
}
