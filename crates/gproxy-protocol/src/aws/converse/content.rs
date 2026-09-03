use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::aws::{CachePointType, CacheTtl, ConversationRole, Rest};

use super::{DocumentBlock, ImageBlock, ToolResultBlock, ToolUseBlock};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct Message {
    pub role: ConversationRole,
    pub content: Vec<ContentBlock>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ContentBlock {
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
    ToolUse {
        #[serde(rename = "toolUse")]
        tool_use: ToolUseBlock,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    ToolResult {
        #[serde(rename = "toolResult")]
        tool_result: ToolResultBlock,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    ReasoningContent {
        #[serde(rename = "reasoningContent")]
        reasoning_content: ReasoningContentBlock,
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
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum SystemContentBlock {
    Text {
        text: String,
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
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ReasoningContentBlock {
    ReasoningText {
        #[serde(rename = "reasoningText")]
        reasoning_text: ReasoningTextBlock,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    RedactedContent {
        #[serde(rename = "redactedContent")]
        redacted_content: String,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: Rest,
    },
    Raw(Value),
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ReasoningTextBlock {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct CachePointBlock {
    #[serde(rename = "type")]
    pub type_: CachePointType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<CacheTtl>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
