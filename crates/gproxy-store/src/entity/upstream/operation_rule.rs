//! User overrides for a provider operation. Channel defaults remain in code.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "operation_rules")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique_key = "operation")]
    pub provider_id: String,
    #[sea_orm(unique_key = "operation")]
    pub operation: String,
    pub action: String,
    /// Optional target OperationKey/configuration; action variants remain for review.
    pub target: Option<Json>,
    #[sea_orm(belongs_to, from = "provider_id", to = "id", on_delete = "Cascade")]
    pub provider: BelongsTo<super::provider::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
