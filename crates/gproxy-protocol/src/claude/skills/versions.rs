use serde::{Deserialize, Serialize};

use super::super::common::{DeletedSkillVersionObjectType, SkillVersionObjectType};

/// Multipart form fields for `POST /v1/skills/{skill_id}/versions`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateSkillVersionRequestBody {
    pub files: Vec<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillVersionPath {
    pub skill_id: String,
    pub version: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListSkillVersionsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SkillVersion {
    pub id: String,
    pub created_at: String,
    pub description: String,
    pub directory: String,
    pub name: String,
    pub skill_id: String,
    #[serde(rename = "type")]
    pub type_: SkillVersionObjectType,
    pub version: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

pub type CreateSkillVersionResponseBody = SkillVersion;
pub type RetrieveSkillVersionResponseBody = SkillVersion;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListSkillVersionsResponseBody {
    pub data: Vec<SkillVersion>,
    pub has_more: bool,
    pub next_page: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeleteSkillVersionResponseBody {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: DeletedSkillVersionObjectType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}
