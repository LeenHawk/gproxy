//! Public Responses compaction body; source: Compact a response.md.
//! Model is required-nullable, input optional-nullable; output is the same 28-item
//! response union, with required usage accounting. Model resources remain open strings.
//! The Codex client compaction body follows, with the client conversation items
//! it and native /search carry. Sources: samples/codex/codex-rs/codex-api/src/
//! common.rs (ResponsesApiRequest), endpoint/responses.rs and protocol/src/models.rs.
use super::responses::{input::Input, response::ResponseOutputItem};
use crate::Rest;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CompactRequestBody {
    #[serde(
        default,
        deserialize_with = "super::responses::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt_cache_options: Option<Option<super::responses::generate::PromptCacheOptions>>,
    #[serde(
        default,
        deserialize_with = "super::responses::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt_cache_retention: Option<Option<super::responses::generate::PromptCacheRetention>>,
    #[wire(required)]
    #[cfg_attr(feature = "schema", schemars(extend("x-gproxy-required" = true)))]
    #[serde(deserialize_with = "super::responses::input::required_nullable")]
    pub model: Option<String>,
    #[serde(
        default,
        deserialize_with = "super::responses::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub input: Option<Option<Input>>,
    #[serde(
        default,
        deserialize_with = "super::responses::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub instructions: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "super::responses::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub previous_response_id: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "super::responses::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub prompt_cache_key: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "super::responses::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub service_tier: Option<Option<ServiceTier>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
pub type CompactRequest = crate::WireRequest<CompactRequestBody>;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ServiceTier {
    Auto,
    Default,
    Fast,
    Flex,
    Priority,
}
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CompactResponseBody {
    pub usage: super::responses::response::ResponseUsage,
    pub id: String,
    pub created_at: i64,
    pub object: CompactObject,
    pub output: Vec<ResponseOutputItem>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
pub type CompactResponse = crate::WireResponse<CompactResponseBody>;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum CompactObject {
    #[serde(rename = "response.compaction")]
    ResponseCompaction,
}

/// Codex client compaction request from codex-api CompactionInput. This is
/// distinct from the public /responses/compact resource schema above.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientCompactRequestBody {
    pub model: String,
    pub input: Vec<ClientResponseItem>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub instructions: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tools: Option<serde_json::Value>,
    pub parallel_tool_calls: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<super::memory::Reasoning>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_cache_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<ClientTextControls>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_programs: Option<AccessPrograms>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
/// The client consumes this complete replacement array verbatim; no invented
/// usage, timestamp, response ID or foreign ciphertext is required.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientCompactResponseBody {
    pub output: Vec<ClientResponseItem>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
pub type ClientCompactRequest = crate::WireRequest<ClientCompactRequestBody>;
pub type ClientCompactResponse = crate::WireResponse<ClientCompactResponseBody>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientAdditionalTools {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub role: String,
    pub tools: Vec<serde_json::Value>,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientMessage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub role: String,
    pub content: Vec<ContentItem>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<MessagePhase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal_chat_message_metadata_passthrough: Option<InternalChatMessageMetadataPassthrough>,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientAgentMessage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub author: String,
    pub recipient: String,
    pub content: Vec<AgentMessageInputContent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal_chat_message_metadata_passthrough: Option<InternalChatMessageMetadataPassthrough>,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientReasoning {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub summary: Vec<ReasoningItemReasoningSummary>,
    #[serde(
        default,
        deserialize_with = "super::responses::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub content: Option<Option<Vec<ReasoningItemContent>>>,
    #[wire(required)]
    #[cfg_attr(feature = "schema", schemars(extend("x-gproxy-required" = true)))]
    #[serde(deserialize_with = "super::responses::input::required_nullable")]
    pub encrypted_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal_chat_message_metadata_passthrough: Option<InternalChatMessageMetadataPassthrough>,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientLocalShellCall {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    #[wire(required)]
    #[cfg_attr(feature = "schema", schemars(extend("x-gproxy-required" = true)))]
    #[serde(deserialize_with = "super::responses::input::required_nullable")]
    pub call_id: Option<String>,
    pub status: LocalShellStatus,
    pub action: LocalShellAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal_chat_message_metadata_passthrough: Option<InternalChatMessageMetadataPassthrough>,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientFunctionCall {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,

    pub arguments: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypted_function_args: Option<Vec<String>>,
    pub call_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal_chat_message_metadata_passthrough: Option<InternalChatMessageMetadataPassthrough>,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientToolSearchCall {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[wire(required)]
    #[cfg_attr(feature = "schema", schemars(extend("x-gproxy-required" = true)))]
    #[serde(deserialize_with = "super::responses::input::required_nullable")]
    pub call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    pub execution: String,
    pub arguments: serde_json::Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal_chat_message_metadata_passthrough: Option<InternalChatMessageMetadataPassthrough>,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientFunctionCallOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    pub output: FunctionCallOutputBody,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal_chat_message_metadata_passthrough: Option<InternalChatMessageMetadataPassthrough>,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientCustomToolCall {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,

    pub call_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
    pub input: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal_chat_message_metadata_passthrough: Option<InternalChatMessageMetadataPassthrough>,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientCustomToolCallOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub call_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub output: FunctionCallOutputBody,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal_chat_message_metadata_passthrough: Option<InternalChatMessageMetadataPassthrough>,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientToolSearchOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[wire(required)]
    #[cfg_attr(feature = "schema", schemars(extend("x-gproxy-required" = true)))]
    #[serde(deserialize_with = "super::responses::input::required_nullable")]
    pub call_id: Option<String>,
    pub status: String,
    pub execution: String,
    pub tools: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal_chat_message_metadata_passthrough: Option<InternalChatMessageMetadataPassthrough>,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientWebSearchCall {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<WebSearchAction>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal_chat_message_metadata_passthrough: Option<InternalChatMessageMetadataPassthrough>,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientImageGenerationCall {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revised_prompt: Option<String>,
    pub result: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal_chat_message_metadata_passthrough: Option<InternalChatMessageMetadataPassthrough>,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientCompaction {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub encrypted_content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal_chat_message_metadata_passthrough: Option<InternalChatMessageMetadataPassthrough>,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientConfigurationUpdate {
    pub reasoning: ConfigurationReasoning,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientCompactionTrigger {
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientContextCompaction {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encrypted_content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub internal_chat_message_metadata_passthrough: Option<InternalChatMessageMetadataPassthrough>,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ResponseItemOther {
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ClientResponseItem {
    AdditionalTools(ClientAdditionalTools),
    Message(ClientMessage),
    AgentMessage(ClientAgentMessage),
    Reasoning(ClientReasoning),
    LocalShellCall(ClientLocalShellCall),
    FunctionCall(ClientFunctionCall),
    ToolSearchCall(ClientToolSearchCall),
    FunctionCallOutput(ClientFunctionCallOutput),
    CustomToolCall(ClientCustomToolCall),
    CustomToolCallOutput(ClientCustomToolCallOutput),
    ToolSearchOutput(ClientToolSearchOutput),
    WebSearchCall(ClientWebSearchCall),
    ImageGenerationCall(ClientImageGenerationCall),
    #[serde(alias = "compaction_summary")]
    Compaction(ClientCompaction),
    ConfigurationUpdate(ClientConfigurationUpdate),
    CompactionTrigger(ClientCompactionTrigger),
    ContextCompaction(ClientContextCompaction),
    Other(ResponseItemOther),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ContentItemInputText {
    pub text: String,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ContentItemInputImage {
    pub image_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<ImageDetail>,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Audio input supplied inline or by uploaded file ID.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum AudioReference {
    Inline { audio_url: String },
    File { file_id: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ContentItemInputAudio {
    #[serde(flatten)]
    pub audio: AudioReference,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

// A flattened untagged enum leaves its keys available to another flattened
// map in serde. Consume the selected reference explicitly so known audio
// fields are not duplicated in `rest` or serialized twice.
impl<'de> Deserialize<'de> for ContentItemInputAudio {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let mut rest = Rest::deserialize(deserializer)?;
        let audio = if rest
            .get("audio_url")
            .is_some_and(serde_json::Value::is_string)
        {
            AudioReference::Inline {
                audio_url: serde_json::from_value(rest.remove("audio_url").unwrap())
                    .map_err(serde::de::Error::custom)?,
            }
        } else if rest
            .get("file_id")
            .is_some_and(serde_json::Value::is_string)
        {
            AudioReference::File {
                file_id: serde_json::from_value(rest.remove("file_id").unwrap())
                    .map_err(serde::de::Error::custom)?,
            }
        } else {
            return Err(serde::de::Error::custom("expected audio_url or file_id"));
        };
        Ok(Self { audio, rest })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ContentItemOutputText {
    pub text: String,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ContentItem {
    InputText(ContentItemInputText),
    InputImage(ContentItemInputImage),
    InputAudio(ContentItemInputAudio),
    OutputText(ContentItemOutputText),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AgentMessageInputContentInputText {
    pub text: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AgentMessageInputContentEncryptedContent {
    pub encrypted_content: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum AgentMessageInputContent {
    InputText(AgentMessageInputContentInputText),
    EncryptedContent(AgentMessageInputContentEncryptedContent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WebSearchActionSearch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queries: Option<Vec<String>>,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WebSearchActionOpenPage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WebSearchActionFindInPage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct WebSearchActionOther {
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum WebSearchAction {
    Search(WebSearchActionSearch),
    OpenPage(WebSearchActionOpenPage),
    FindInPage(WebSearchActionFindInPage),
    Other(WebSearchActionOther),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ReasoningItemReasoningSummarySummaryText {
    pub text: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ReasoningItemReasoningSummary {
    SummaryText(ReasoningItemReasoningSummarySummaryText),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ReasoningItemContentReasoningText {
    pub text: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ReasoningItemContentText {
    pub text: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ReasoningItemContent {
    ReasoningText(ReasoningItemContentReasoningText),
    Text(ReasoningItemContentText),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct FunctionCallOutputContentItemInputText {
    pub text: String,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct FunctionCallOutputContentItemInputImage {
    pub image_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<ImageDetail>,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct FunctionCallOutputContentItemInputAudio {
    pub audio_url: String,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct FunctionCallOutputContentItemEncryptedContent {
    pub encrypted_content: String,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum FunctionCallOutputContentItem {
    InputText(FunctionCallOutputContentItemInputText),
    InputImage(FunctionCallOutputContentItemInputImage),
    InputAudio(FunctionCallOutputContentItemInputAudio),
    EncryptedContent(FunctionCallOutputContentItemEncryptedContent),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ImageDetail {
    Auto,
    Low,
    High,
    Original,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum MessagePhase {
    Commentary,

    FinalAnswer,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum LocalShellStatus {
    Completed,
    InProgress,
    Incomplete,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LocalShellExecAction {
    pub command: Vec<String>,
    #[wire(required)]
    #[cfg_attr(feature = "schema", schemars(extend("x-gproxy-required" = true)))]
    #[serde(deserialize_with = "super::responses::input::required_nullable")]
    pub timeout_ms: Option<u64>,
    #[wire(required)]
    #[cfg_attr(feature = "schema", schemars(extend("x-gproxy-required" = true)))]
    #[serde(deserialize_with = "super::responses::input::required_nullable")]
    pub working_directory: Option<String>,
    #[wire(required)]
    #[cfg_attr(feature = "schema", schemars(extend("x-gproxy-required" = true)))]
    #[serde(deserialize_with = "super::responses::input::required_nullable")]
    pub env: Option<BTreeMap<String, String>>,
    #[wire(required)]
    #[cfg_attr(feature = "schema", schemars(extend("x-gproxy-required" = true)))]
    #[serde(deserialize_with = "super::responses::input::required_nullable")]
    pub user: Option<String>,

    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum LocalShellAction {
    Exec(LocalShellExecAction),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum FunctionCallOutputBody {
    Text(String),
    ContentItems(Vec<FunctionCallOutputContentItem>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ConfigurationReasoning {
    pub effort: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct InternalChatMessageMetadataPassthrough {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time: Option<serde_json::Number>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_item_kinds: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cell_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub executed_tool_calls: Option<Vec<ExecutedToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls_complete: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ExecutedToolCall {
    pub name: String,
    pub arguments: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_result_sources: Option<ToolResultSources>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ToolResultSources {
    Sources(Vec<ToolResultSource>),
    ParseFailed(ParseFailed),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ParseFailed {
    ParseFailed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ToolResultSource {
    #[serde(rename = "type")]
    pub type_: String,
    pub id: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientTextControls {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verbosity: Option<ClientVerbosity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<ClientTextFormat>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ClientVerbosity {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientTextFormat {
    #[serde(rename = "type")]
    pub type_: ClientTextFormatType,
    pub strict: bool,
    pub schema: serde_json::Value,
    pub name: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ClientTextFormatType {
    JsonSchema,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct AccessPrograms {
    pub cyber: CyberAccessProgram,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

pub use super::responses::CyberAccessProgram;
