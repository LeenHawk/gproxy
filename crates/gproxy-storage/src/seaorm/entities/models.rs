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
    /// Full serialized `gproxy_sdk::channel::billing::ModelPrice` (minus
    /// `model_id` / `display_name` which live in their own columns).
    /// Covers every billing mode (default / flex / scale / priority).
    /// `NULL` means "no custom pricing" — the row falls through to the
    /// provider's `default` built-in price entry.
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
