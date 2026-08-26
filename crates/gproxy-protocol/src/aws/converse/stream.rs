use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::aws::{ConversationRole, Rest, StopReason};

use super::{
    ContentBlockDelta, ContentBlockStart, ConverseMetrics, ConverseStreamTrace,
    PerformanceConfiguration, ServiceTier, TokenUsage,
};

/// A decoded Smithy event-stream item. The discriminant comes from the
/// `:event-type` or `:exception-type` header; the inner struct is the frame's
/// JSON payload.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ConverseStreamEvent {
    MessageStart(MessageStartEvent),
    ContentBlockStart(ContentBlockStartEvent),
    ContentBlockDelta(ContentBlockDeltaEvent),
    ContentBlockStop(ContentBlockStopEvent),
    MessageStop(MessageStopEvent),
    Metadata(Box<ConverseStreamMetadataEvent>),
    InternalServerException(StreamException),
    ModelStreamErrorException(ModelStreamErrorException),
    ValidationException(StreamException),
    ThrottlingException(StreamException),
    ServiceUnavailableException(StreamException),
    Unknown { event_type: String, payload: Value },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageStartEvent {
    pub role: ConversationRole,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentBlockStartEvent {
    pub start: ContentBlockStart,
    pub content_block_index: u64,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentBlockDeltaEvent {
    pub delta: ContentBlockDelta,
    pub content_block_index: u64,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContentBlockStopEvent {
    pub content_block_index: u64,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageStopEvent {
    pub stop_reason: StopReason,
    /// `upstream_docs/aws/docs/ConverseStream.md`,
    /// `messageStop.additionalModelResponseFields`: model-specific fields as a JSON value.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_model_response_fields: Option<Value>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConverseStreamMetadataEvent {
    pub usage: TokenUsage,
    pub metrics: ConverseMetrics,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub performance_config: Option<PerformanceConfiguration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub service_tier: Option<ServiceTier>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<ConverseStreamTrace>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamException {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStreamErrorException {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_status_code: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_message: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
