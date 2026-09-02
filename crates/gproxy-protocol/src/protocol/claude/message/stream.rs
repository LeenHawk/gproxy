use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::protocol::claude::common::{
    AssistantRole, Citation, ClaudeModel, Container, ContentBlock, ContextManagementResponse,
    InputTransformation, JsonObject, MessageObjectType, StopDetails, StopReason, TypedObject,
    Usage,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
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
#[non_exhaustive]
pub enum KnownStreamEvent {
    #[serde(rename = "message_start")]
    MessageStart {
        message: Box<CreateMessageStartBody>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: JsonObject,
    },
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        index: u64,
        content_block: Box<ContentBlock>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: JsonObject,
    },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta {
        index: u64,
        delta: Box<EventDelta>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: JsonObject,
    },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop {
        index: u64,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: JsonObject,
    },
    #[serde(rename = "message_delta")]
    MessageDelta {
        #[serde(skip_serializing_if = "Option::is_none")]
        context_management: Option<Box<ContextManagementResponse>>,
        delta: Box<MessageDelta>,
        #[serde(skip_serializing_if = "Option::is_none")]
        input_transformations: Option<Vec<InputTransformation>>,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<Box<Usage>>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: JsonObject,
    },
    #[serde(rename = "message_stop")]
    MessageStop {
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: JsonObject,
    },
    #[serde(rename = "ping")]
    Ping {
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: JsonObject,
    },
    #[serde(rename = "error")]
    Error {
        error: StreamError,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: JsonObject,
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
#[non_exhaustive]
pub enum EventDelta {
    Known(Box<KnownEventDelta>),
    Unknown(TypedObject),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[non_exhaustive]
pub enum KnownEventDelta {
    #[serde(rename = "text_delta")]
    Text {
        text: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: JsonObject,
    },
    #[serde(rename = "input_json_delta")]
    InputJson {
        partial_json: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: JsonObject,
    },
    #[serde(rename = "citations_delta")]
    Citations {
        citation: Box<Citation>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: JsonObject,
    },
    #[serde(rename = "thinking_delta")]
    Thinking {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        estimated_tokens: Option<u64>,
        thinking: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: JsonObject,
    },
    #[serde(rename = "signature_delta")]
    Signature {
        signature: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: JsonObject,
    },
    #[serde(rename = "compaction_delta")]
    Compaction {
        content: String,
        encrypted_content: String,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: JsonObject,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct CreateMessageStartBody {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: MessageObjectType,
    pub role: AssistantRole,
    pub content: Vec<ContentBlock>,
    pub model: ClaudeModel,
    pub stop_reason: Option<StopReason>,
    pub stop_sequence: Option<String>,
    pub usage: Usage,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_transformations: Option<Vec<InputTransformation>>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct MessageDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container: Option<Container>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<StopReason>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_details: Option<StopDetails>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct StreamError {
    #[serde(rename = "type")]
    pub type_: String,
    pub message: String,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression: `event_name()` must equal the serialized `type` tag.
    #[test]
    fn event_name_matches_serialized_type_tag() {
        let events: Vec<StreamEvent> = [
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}}"#,
            r#"{"type":"message_stop"}"#,
            r#"{"type":"some_future_event","x":1}"#,
        ]
        .iter()
        .map(|raw| serde_json::from_str(raw).unwrap())
        .collect();
        for event in events {
            let value = serde_json::to_value(&event).unwrap();
            assert_eq!(
                event.event_name(),
                value.get("type").and_then(serde_json::Value::as_str),
                "{value}"
            );
        }
    }
}
