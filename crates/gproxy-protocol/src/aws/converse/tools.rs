use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::aws::{EmptyObject, Rest, ToolResultStatus, ToolUseType};

use super::{CachePointBlock, DocumentBlock, ImageBlock};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ToolUseBlock {
    pub tool_use_id: String,
    pub name: String,
    pub input: Value,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<ToolUseType>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ToolResultBlock {
    pub tool_use_id: String,
    pub content: Vec<ToolResultContentBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<ToolResultStatus>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ToolResultContentBlock {
    Json {
        json: Value,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    Text {
        text: String,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    Image {
        image: ImageBlock,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    Document {
        document: DocumentBlock,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ToolConfiguration {
    pub tools: Vec<Tool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<ToolChoice>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum Tool {
    ToolSpec {
        #[serde(rename = "toolSpec")]
        tool_spec: ToolSpecification,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    CachePoint {
        #[serde(rename = "cachePoint")]
        cache_point: CachePointBlock,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[derive(gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ToolSpecification {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub input_schema: ToolInputSchema,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strict: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ToolInputSchema {
    Json {
        json: Value,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ToolChoice {
    Auto {
        auto: EmptyObject,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    Any {
        any: EmptyObject,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    Specific {
        tool: SpecificToolChoice,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    Raw(Value),
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct SpecificToolChoice {
    pub name: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
