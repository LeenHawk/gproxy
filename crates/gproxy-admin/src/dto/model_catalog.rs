use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DefaultModelCatalogDto {
    pub schema_version: u32,
    pub source: DefaultModelCatalogSourceDto,
    pub models: Vec<DefaultModelDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DefaultModelCatalogSourceDto {
    pub catalog: String,
    pub fetched_at: String,
    pub total_models: usize,
    pub context_models: usize,
    pub output_limit_models: usize,
    pub priced_models: usize,
    pub dynamic_price_models: usize,
    pub embedding_models: usize,
    pub rerank_models: usize,
    pub image_output_priced_models: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DefaultModelDto {
    pub model_id: String,
    pub display_name: Option<String>,
    pub context_window: Option<i64>,
    pub max_output_tokens: Option<i64>,
    pub input_modalities: Vec<String>,
    pub output_modalities: Vec<String>,
    pub supported_parameters: Vec<String>,
    pub pricing: Option<DefaultModelPricingDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DefaultModelPricingDto {
    pub model_pattern: String,
    #[ts(type = "unknown | null")]
    pub tiers: Option<Value>,
    pub priority: i64,
    pub rates: Vec<DefaultModelPriceRateDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DefaultModelPriceRateDto {
    pub metric: String,
    pub unit_size: u64,
    pub price: String,
    pub priority: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ApplyDefaultModelPricesRequest {
    pub provider_id: i64,
    pub model_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ApplyDefaultModelPricesResponse {
    pub created: usize,
    pub skipped: usize,
    pub unmatched: usize,
}
