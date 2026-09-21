//! Roles are scoped to an organization or team membership, not to the user globally.

use sea_orm::entity::prelude::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[sea_orm(rs_type = "String", db_type = "String(StringLen::N(16))")]
pub enum MembershipRole {
    #[sea_orm(string_value = "member")]
    Member,
    #[sea_orm(string_value = "admin")]
    Admin,
}
