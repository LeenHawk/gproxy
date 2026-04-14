use sea_orm::entity::prelude::*;
use time::OffsetDateTime;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "providers")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = true)]
    pub id: i64,
    #[sea_orm(unique_key = "provider_name")]
    pub name: String,
    pub channel: String,
    /// Optional human-readable display name shown in the admin console.
    /// `None` means the UI falls back to `name`.
    #[sea_orm(column_type = "Text", nullable)]
    pub label: Option<String>,
    pub settings_json: Json,
    pub dispatch_json: Json,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    #[sea_orm(has_many)]
    pub credentials: HasMany<super::credentials::Entity>,
    #[sea_orm(has_many)]
    pub upstream_requests: HasMany<super::upstream_requests::Entity>,
    #[sea_orm(has_many)]
    pub usages: HasMany<super::usages::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
