//! Claude Messages SSE event data; transport framing is deliberately separate.
//! Sources: Streaming Messages.md; Beta types.md RawMessageStreamEvent and deltas.
//! Optional/nullability omitted by the rendered Beta table is corroborated by
//! anthropic-sdk-python types/beta/{beta_message,beta_raw_message_delta_event,
//! beta_message_delta_usage,beta_compaction_content_block_delta}.py.
use super::generate_content as response;
use crate::Rest;
use serde::{Deserialize, Serialize};

pub type StreamResponse = crate::WireResponse<crate::connection::ByteStream>;
pub type StreamUsage = response::Usage;
pub type StreamStopReason = response::StopReason;
pub type StreamMessageType = response::GenerateContentResponseBodyType;
fn present_nullable<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum StreamEvent {
    #[serde(rename = "message_start")]
    MessageStart(Box<MessageStartEvent>),
    #[serde(rename = "content_block_start")]
    ContentBlockStart(ContentBlockStartEvent),
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta(ContentBlockDeltaEvent),
    #[serde(rename = "content_block_stop")]
    ContentBlockStop(ContentBlockStopEvent),
    #[serde(rename = "message_delta")]
    MessageDelta(Box<MessageDeltaEvent>),
    #[serde(rename = "message_stop")]
    MessageStop(MessageStopEvent),
    #[serde(rename = "ping")]
    Ping(PingEvent),
    #[serde(rename = "error")]
    Error(ErrorEvent),
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct MessageStartEvent {
    pub message: StreamMessage,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct StreamMessage {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub input_transformations: Option<Option<Vec<response::InputTransformation>>>,

    #[serde(rename = "type")]
    pub type_: StreamMessageType,
    pub id: String,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub container: Option<Option<response::ResponseContainer>>,
    pub content: Vec<response::ResponseContentBlock>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub context_management: Option<Option<response::ContextManagementResponse>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub diagnostics: Option<Option<response::DiagnosticsResponse>>,
    pub model: String,
    pub role: response::ResponseRole,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub stop_details: Option<Option<response::RefusalStopDetails>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub stop_reason: Option<Option<StreamStopReason>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub stop_sequence: Option<Option<String>>,
    pub usage: StreamUsage,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ContentBlockStartEvent {
    pub index: i64,
    pub content_block: response::ResponseContentBlock,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ContentBlockDeltaEvent {
    pub index: i64,
    pub delta: ContentBlockDelta,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ContentBlockDelta {
    #[serde(rename = "text_delta")]
    Text(TextDelta),
    #[serde(rename = "input_json_delta")]
    InputJson(InputJsonDelta),
    #[serde(rename = "thinking_delta")]
    Thinking(ThinkingDelta),
    #[serde(rename = "signature_delta")]
    Signature(SignatureDelta),
    #[serde(rename = "citations_delta")]
    Citations(CitationsDelta),
    #[serde(rename = "compaction_delta")]
    Compaction(CompactionDelta),
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct TextDelta {
    pub text: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct InputJsonDelta {
    pub partial_json: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ThinkingDelta {
    pub thinking: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct SignatureDelta {
    pub signature: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct CitationsDelta {
    pub citation: response::ResponseTextCitation,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct CompactionDelta {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub content: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub encrypted_content: Option<Option<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ContentBlockStopEvent {
    pub index: i64,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct MessageDeltaEvent {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub input_transformations: Option<Option<Vec<response::InputTransformation>>>,

    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub context_management: Option<Option<response::ContextManagementResponse>>,
    pub delta: MessageDelta,
    pub usage: MessageDeltaUsage,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct MessageDelta {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub container: Option<Option<response::ResponseContainer>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub stop_details: Option<Option<response::RefusalStopDetails>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub stop_reason: Option<Option<StreamStopReason>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub stop_sequence: Option<Option<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct MessageDeltaUsage {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub cache_creation_input_tokens: Option<Option<i64>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub cache_read_input_tokens: Option<Option<i64>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub fallback_credit: Option<Option<response::FallbackCreditUsage>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub input_tokens: Option<Option<i64>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub iterations: Option<Option<Vec<response::IterationUsage>>>,
    pub output_tokens: i64,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub output_tokens_details: Option<Option<response::OutputTokensDetails>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub server_tool_use: Option<Option<response::ServerToolUsage>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct MessageStopEvent {
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct PingEvent {
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ErrorEvent {
    pub error: StreamError,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub request_id: Option<Option<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct StreamError {
    #[serde(rename = "type")]
    pub type_: StreamErrorType,
    pub message: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum StreamErrorType {
    ApiError,
    AuthenticationError,
    BillingError,
    InvalidRequestError,
    NotFoundError,
    OverloadedError,
    PermissionError,
    RateLimitError,
    TimeoutError,
    ConflictError,
    RequestTooLarge,
}
