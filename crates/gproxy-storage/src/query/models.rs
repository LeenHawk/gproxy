use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

use super::Scope;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ModelQuery {
    pub id: Scope<i64>,
    pub provider_id: Scope<i64>,
    pub model_id: Scope<String>,
    pub enabled: Scope<bool>,
    pub limit: Option<u64>,
    pub offset: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelQueryRow {
    pub id: i64,
    pub provider_id: i64,
    pub model_id: String,
    pub display_name: Option<String>,
    pub enabled: bool,
    pub price_each_call: Option<f64>,
    pub price_tiers_json: Option<String>,
    pub pricing_json: Option<String>,
    /// NULL = real model, Some(id) = alias pointing to another model's id.
    pub alias_of: Option<i64>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
}
