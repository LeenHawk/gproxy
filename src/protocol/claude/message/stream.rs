use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::protocol::claude::common::{
    AssistantRole, Citation, ClaudeModel, Container, ContentBlock, ContextManagementResponse,
    JsonObject, MessageObjectType, StopDetails, StopReason, TypedObject, Usage,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StreamEvent {
    Known(Box<KnownStreamEvent>),
    Unknown(TypedObject),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum EventDelta {
    Known(Box<KnownEventDelta>),
    Unknown(TypedObject),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
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
    pub usage: Usage,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
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
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamError {
    #[serde(rename = "type")]
    pub type_: String,
    pub message: String,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}
