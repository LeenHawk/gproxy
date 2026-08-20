use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::super::common::{DeletedSkillVersionObjectType, JsonObject, SkillVersionObjectType};

/// Multipart form fields for `POST /v1/skills/{skill_id}/versions`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct CreateSkillVersionRequestBody {
    pub files: Vec<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct SkillVersionPath {
    pub skill_id: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct ListSkillVersionsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct SkillVersion {
    pub id: String,
    pub created_at: String,
    pub description: String,
    pub name: String,
    pub skill_id: String,
    #[serde(rename = "type")]
    pub type_: SkillVersionObjectType,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

pub type CreateSkillVersionResponseBody = SkillVersion;
pub type RetrieveSkillVersionResponseBody = SkillVersion;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct ListSkillVersionsResponseBody {
    pub data: Vec<SkillVersion>,
    pub next_page: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct DeleteSkillVersionResponseBody {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: DeletedSkillVersionObjectType,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}
