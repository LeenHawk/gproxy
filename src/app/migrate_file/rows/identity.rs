use serde::Deserialize;

use super::default_true;
use crate::store::persistence::records::{OrgInput, TeamInput, UserInput};

#[derive(Deserialize)]
pub(crate) struct LegacyOrg {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub description: Option<String>,
}

impl From<LegacyOrg> for OrgInput {
    fn from(x: LegacyOrg) -> Self {
        Self {
            id: Some(x.id),
            name: x.name,
            enabled: x.enabled,
            description: x.description,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct LegacyTeam {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub org_id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl From<LegacyTeam> for TeamInput {
    fn from(x: LegacyTeam) -> Self {
        Self {
            id: Some(x.id),
            org_id: x.org_id,
            name: x.name,
            enabled: x.enabled,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct LegacyUser {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub org_id: i64,
    #[serde(default)]
    pub team_id: Option<i64>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub is_admin: bool,
}

impl From<LegacyUser> for UserInput {
    fn from(x: LegacyUser) -> Self {
        Self {
            id: Some(x.id),
            name: x.name,
            org_id: x.org_id,
            team_id: x.team_id,
            password: x.password,
            enabled: x.enabled,
            is_admin: x.is_admin,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct LegacyUserKey {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub user_id: i64,
    #[serde(default)]
    pub api_key_ciphertext: String,
    #[serde(default)]
    pub api_key_digest: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default = "default_true")]
    pub enabled: bool,
}
