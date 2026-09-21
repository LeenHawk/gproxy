//! Organization and team membership. Both tables have a composite primary key
//! and no surrogate id, so these DTOs name the pair rather than an `id`.

use gproxy_store::entity::identity::{
    membership_role::MembershipRole, organization_member, team_member,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationMemberDto {
    pub organization_id: String,
    pub user_id: String,
    /// `member` or `admin`. Scoped to this organization; it is not the user's
    /// instance role.
    pub role: String,
}

impl From<organization_member::Model> for OrganizationMemberDto {
    fn from(row: organization_member::Model) -> Self {
        Self {
            organization_id: row.organization_id,
            user_id: row.user_id,
            role: role_name(row.role).to_owned(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMemberDto {
    pub team_id: String,
    pub user_id: String,
    pub role: String,
}

impl From<team_member::Model> for TeamMemberDto {
    fn from(row: team_member::Model) -> Self {
        Self {
            team_id: row.team_id,
            user_id: row.user_id,
            role: role_name(row.role).to_owned(),
        }
    }
}

/// One user's place in one scope, whichever of the two tables it came from.
/// Used by the reads that answer "who is in here" without the caller having to
/// know which family it asked.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MembershipDto {
    /// `organization` or `team`.
    pub scope_kind: String,
    pub scope_id: String,
    pub user_id: String,
    pub role: String,
}

pub(crate) fn role_name(role: MembershipRole) -> &'static str {
    match role {
        MembershipRole::Member => "member",
        MembershipRole::Admin => "admin",
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberWrite {
    pub user_id: String,
    /// `member` or `admin`; absent means `member`.
    #[serde(default)]
    pub role: Option<String>,
}

/// The only thing a membership has to change. Moving a member to another
/// organization is a remove and an add, not a patch: the pair *is* the key.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemberPatch {
    pub role: String,
}
