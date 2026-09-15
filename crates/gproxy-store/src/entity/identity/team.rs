//! A team belongs to one organization and owns its own shared credentials.

use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "teams")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub id: String,
    #[sea_orm(unique_key = "organization_name")]
    pub organization_id: String,
    #[sea_orm(unique_key = "organization_name")]
    pub name: String,
    pub created_at_ms: i64,
    #[sea_orm(belongs_to, from = "organization_id", to = "id", on_delete = "Cascade")]
    pub organization: BelongsTo<super::organization::Entity>,
    #[sea_orm(has_many)]
    pub members: HasMany<super::team_member::Entity>,
    #[sea_orm(has_many)]
    pub credentials: HasMany<crate::entity::upstream::credential::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
