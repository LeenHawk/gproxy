use serde::{Deserialize, Serialize};

use super::TypedObject;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct DiagnosticsParam {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_message_id: Option<Option<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct Diagnostics {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_miss_reason: Option<CacheMissReason>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum CacheMissReason {
    Known(KnownCacheMissReason),
    Unknown(TypedObject),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum KnownCacheMissReason {
    #[serde(rename = "model_changed")]
    ModelChanged {
        cache_missed_input_tokens: u64,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
    #[serde(rename = "system_changed")]
    SystemChanged {
        cache_missed_input_tokens: u64,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
    #[serde(rename = "tools_changed")]
    ToolsChanged {
        cache_missed_input_tokens: u64,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
    #[serde(rename = "messages_changed")]
    MessagesChanged {
        cache_missed_input_tokens: u64,
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
    #[serde(rename = "previous_message_not_found")]
    PreviousMessageNotFound {
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
    #[serde(rename = "unavailable")]
    Unavailable {
        #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
        rest: serde_json::Map<String, serde_json::Value>,
    },
}
