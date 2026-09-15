//! Organization membership and administrator role.
//! Members may use organization credentials; viewing/editing/deleting those
//! shared credentials requires an administrator role for that organization.

use super::membership_role::MembershipRole;
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "organization_members")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub organization_id: String,
    #[sea_orm(primary_key, auto_increment = false, indexed)]
    pub user_id: String,
    #[sea_orm(default_value = "member")]
    pub role: MembershipRole,
    #[sea_orm(belongs_to, from = "organization_id", to = "id", on_delete = "Cascade")]
    pub organization: BelongsTo<super::organization::Entity>,
    #[sea_orm(belongs_to, from = "user_id", to = "id", on_delete = "Cascade")]
    pub user: BelongsTo<super::user::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
