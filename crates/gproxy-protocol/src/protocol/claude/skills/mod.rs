use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::common::{
    AnthropicBetaHeaders, DeletedSkillObjectType, JsonObject, SkillObjectType, SkillSourceType,
};

mod versions;

pub use versions::*;

pub type SkillRequestHeaders = AnthropicBetaHeaders;

/// Multipart form fields for `POST /v1/skills`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct CreateSkillRequestBody {
    pub files: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct SkillPath {
    pub skill_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct ListSkillsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<SkillSourceType>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct SkillSource {
    #[serde(rename = "type")]
    pub type_: SkillSourceType,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct SkillObject {
    pub id: String,
    pub created_at: String,
    pub display_name: String,
    pub latest_version_id: String,
    pub source: SkillSource,
    #[serde(rename = "type")]
    pub type_: SkillObjectType,
    pub updated_at: String,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

pub type CreateSkillResponseBody = SkillObject;
pub type RetrieveSkillResponseBody = SkillObject;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct ListSkillsResponseBody {
    pub data: Vec<SkillObject>,
    pub next_page: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct DeleteSkillResponseBody {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: DeletedSkillObjectType,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn decodes_ga_skill_resource_shape() {
        let skill: SkillObject = serde_json::from_value(json!({
            "id": "skill_123",
            "created_at": "2026-08-19T00:00:00Z",
            "display_name": "Release notes",
            "latest_version_id": "skillver_456",
            "source": {"type": "anthropic_example"},
            "type": "skill",
            "updated_at": "2026-08-19T01:00:00Z"
        }))
        .unwrap();

        assert!(matches!(
            skill.source.type_,
            SkillSourceType::Known(super::super::common::SkillSourceTypeKnown::AnthropicExample)
        ));
    }
}
