use crate::Rest;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InteractionContent {
    Text(InteractionText),
    Image(InteractionMedia),
    Audio(InteractionMedia),
    Video(InteractionMedia),
    Document(InteractionMedia),
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct InteractionText {
    pub text: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub annotations: Vec<Value>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct InteractionMedia {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub uri: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channels: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InteractionStep {
    UserInput(InteractionStepBody),
    ModelOutput(InteractionStepBody),
    Thought(InteractionStepBody),
    FunctionCall(InteractionStepBody),
    FunctionResult(InteractionStepBody),
    CodeExecutionCall(InteractionStepBody),
    CodeExecutionResult(InteractionStepBody),
    GoogleSearchCall(InteractionStepBody),
    GoogleSearchResult(InteractionStepBody),
    GoogleMapsCall(InteractionStepBody),
    GoogleMapsResult(InteractionStepBody),
    UrlContextCall(InteractionStepBody),
    UrlContextResult(InteractionStepBody),
    FileSearchCall(InteractionStepBody),
    FileSearchResult(InteractionStepBody),
    McpServerToolCall(InteractionStepBody),
    McpServerToolResult(InteractionStepBody),
    ProcessingCall(InteractionStepBody),
    ProcessingResult(InteractionStepBody),
}

/// Fields shared by native step variants; vendor-specific fields remain in rest.
#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct InteractionStepBody {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_error: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub content: Vec<InteractionContent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub summary: Vec<InteractionContent>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Native tool declaration, including hosted tools and MCP configuration.
#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct InteractionTool {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_name: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

impl InteractionStep {
    pub fn body(&self) -> &InteractionStepBody {
        match self {
            Self::UserInput(body)
            | Self::ModelOutput(body)
            | Self::Thought(body)
            | Self::FunctionCall(body)
            | Self::FunctionResult(body)
            | Self::CodeExecutionCall(body)
            | Self::CodeExecutionResult(body)
            | Self::GoogleSearchCall(body)
            | Self::GoogleSearchResult(body)
            | Self::GoogleMapsCall(body)
            | Self::GoogleMapsResult(body)
            | Self::UrlContextCall(body)
            | Self::UrlContextResult(body)
            | Self::FileSearchCall(body)
            | Self::FileSearchResult(body)
            | Self::McpServerToolCall(body)
            | Self::McpServerToolResult(body)
            | Self::ProcessingCall(body)
            | Self::ProcessingResult(body) => body,
        }
    }
}
