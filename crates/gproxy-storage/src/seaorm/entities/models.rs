use sea_orm::entity::prelude::*;
use time::OffsetDateTime;

/// Model registry — tracks available models per provider with pricing.
///
/// Currently maintained manually by admin. Frontend will add auto-discovery
/// from upstream model list endpoints in a future release.
#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "models")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = true)]
    pub id: i64,
    #[sea_orm(unique_key = "model_provider_model_id")]
    pub provider_id: i64,
    #[sea_orm(unique_key = "model_provider_model_id")]
    pub model_id: String,
    pub display_name: Option<String>,
    pub enabled: bool,
    pub price_each_call: Option<f64>,
    /// JSON array of price tiers: `[{"input_tokens_up_to":200000,"price_input_tokens":3.0,...}]`.
    ///
    /// Deprecated in favour of `pricing_json`; kept nullable so existing rows
    /// can be backfilled on first boot and then ignored at runtime.
    #[sea_orm(column_type = "Text", nullable)]
    pub price_tiers_json: Option<String>,
    /// Full serialized `gproxy_sdk::provider::billing::ModelPrice` (minus
    /// `model_id` / `display_name` which live in their own columns). When
    /// populated, this is the authoritative pricing source and covers every
    /// billing mode (default / flex / scale / priority) plus
    /// `tool_call_prices`.
    #[sea_orm(column_type = "Text", nullable)]
    pub pricing_json: Option<String>,
    /// NULL = real model, Some(id) = alias pointing to another model's id.
    #[sea_orm(nullable)]
    pub alias_of: Option<i64>,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    #[sea_orm(belongs_to, from = "provider_id", to = "id", on_delete = "Cascade")]
    pub provider: HasOne<super::providers::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
