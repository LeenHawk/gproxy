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
pub struct PriceCatalogDto {
    pub service_tiers: Vec<String>,
    pub profiles: Vec<PriceProfileDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PriceProfileDto {
    pub kind: PriceProfileKindDto,
    pub metrics: Vec<PriceMetricDto>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(rename_all = "snake_case")]
pub enum PriceProfileKindDto {
    Generation,
    Embedding,
    Rerank,
    Image,
    Audio,
    Video,
    Tools,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PriceMetricDto {
    pub metric: String,
    pub unit_size: u64,
}
