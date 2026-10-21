//! Tool errors from upstream_docs/openai/docs/Create a model response.md.
use super::input::{present_optional, required_nullable};
use crate::Rest;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum McpToolCallError {
    Detailed(McpToolCallErrorDetails),
    /// Earlier Responses references represented MCP errors as strings.
    Legacy(String),
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum McpToolCallErrorDetails {
    #[serde(rename = "mcp_protocol_error")]
    Protocol {
        code: i64,
        message: String,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    #[serde(rename = "mcp_tool_execution_error")]
    Execution {
        content: serde_json::Value,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    #[serde(rename = "http_error")]
    Http {
        code: i64,
        message: String,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ImageGenerationCallError {
    #[wire(required)]
    #[cfg_attr(feature = "schema", schemars(extend("x-gproxy-required" = true)))]
    #[serde(deserialize_with = "required_nullable")]
    pub code: Option<String>,
    pub message: String,
    #[wire(required)]
    #[cfg_attr(feature = "schema", schemars(extend("x-gproxy-required" = true)))]
    #[serde(deserialize_with = "required_nullable")]
    pub param: Option<String>,
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(
        default,
        deserialize_with = "present_optional",
        skip_serializing_if = "Option::is_none"
    )]
    pub moderation_details: Option<ImageModerationDetails>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ImageModerationDetails {
    pub categories: Vec<String>,
    pub moderation_stage: ImageModerationStage,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ImageModerationStage {
    Input,
    Output,
    Unknown,
}
