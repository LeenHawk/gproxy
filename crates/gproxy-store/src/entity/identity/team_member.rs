//! Team membership and administrator role.
//! Members may use team credentials; viewing/editing/deleting those shared
//! credentials requires an administrator role for that team.

use super::membership_role::MembershipRole;
use sea_orm::entity::prelude::*;

#[sea_orm::model]
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[sea_orm(table_name = "team_members")]
pub struct Model {
    #[sea_orm(primary_key, auto_increment = false)]
    pub team_id: String,
    #[sea_orm(primary_key, auto_increment = false, indexed)]
    pub user_id: String,
    #[sea_orm(default_value = "member")]
    pub role: MembershipRole,
    #[sea_orm(belongs_to, from = "team_id", to = "id", on_delete = "Cascade")]
    pub team: BelongsTo<super::team::Entity>,
    #[sea_orm(belongs_to, from = "user_id", to = "id", on_delete = "Cascade")]
    pub user: BelongsTo<super::user::Entity>,
}

impl ActiveModelBehavior for ActiveModel {}
