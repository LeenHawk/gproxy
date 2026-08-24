use serde::{Deserialize, Serialize};

use crate::claude::common::{
    AssistantRole, Citation, ClaudeModel, Container, ContentBlock, ContextManagementResponse,
    MessageObjectType, StopDetails, StopReason, TypedObject, Usage,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum StreamEvent {
    Known(Box<KnownStreamEvent>),
    Unknown(TypedObject),
}

impl StreamEvent {
    /// SSE event name: the wire `type` of this event, if any.
    pub fn event_name(&self) -> Option<&str> {
        match self {
            Self::Known(event) => Some(event.event_name()),
            Self::Unknown(object) => Some(object.type_.as_str()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum KnownStreamEvent {
    #[serde(rename = "message_start")]
    MessageStart {
        message: Box<CreateMessageStartBody>,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: u64,
        content_block: Box<ContentBlock>,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta {
        index: u64,
        delta: Box<EventDelta>,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop {
        index: u64,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
    #[serde(rename = "message_delta")]
    MessageDelta {
        #[serde(skip_serializing_if = "Option::is_none")]
        context_management: Option<Box<ContextManagementResponse>>,
        delta: Box<MessageDelta>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Box<Usage>>,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
    #[serde(rename = "message_stop")]
    MessageStop {
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
    #[serde(rename = "ping")]
    Ping {
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
    #[serde(rename = "error")]
    Error {
        error: StreamError,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
}

impl KnownStreamEvent {
    /// SSE event name: the exact serde rename of this variant.
    pub fn event_name(&self) -> &'static str {
        match self {
            Self::MessageStart { .. } => "message_start",
            Self::ContentBlockStart { .. } => "content_block_start",
            Self::ContentBlockDelta { .. } => "content_block_delta",
            Self::ContentBlockStop { .. } => "content_block_stop",
            Self::MessageDelta { .. } => "message_delta",
            Self::MessageStop { .. } => "message_stop",
            Self::Ping { .. } => "ping",
            Self::Error { .. } => "error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum EventDelta {
    Known(Box<KnownEventDelta>),
    Unknown(TypedObject),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum KnownEventDelta {
    #[serde(rename = "text_delta")]
    Text {
        text: String,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
    #[serde(rename = "input_json_delta")]
    InputJson {
        partial_json: String,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
    #[serde(rename = "citations_delta")]
    Citations {
        citation: Box<Citation>,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
    #[serde(rename = "thinking_delta")]
    Thinking {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        estimated_tokens: Option<u64>,
        thinking: String,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
    #[serde(rename = "signature_delta")]
    Signature {
        signature: String,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
    #[serde(rename = "compaction_delta")]
    Compaction {
        content: String,
        encrypted_content: String,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateMessageStartBody {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: MessageObjectType,
    pub role: AssistantRole,
    pub content: Vec<ContentBlock>,
    pub model: ClaudeModel,
    pub stop_reason: Option<StopReason>,
    pub stop_sequence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<Usage>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<Container>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<StopReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_details: Option<StopDetails>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamError {
    #[serde(rename = "type")]
    pub type_: String,
    pub message: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}
