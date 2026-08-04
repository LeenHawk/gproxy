use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::{JsonObject, TypedObject};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct DiagnosticsParam {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_message_id: Option<Option<String>>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct Diagnostics {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_miss_reason: Option<CacheMissReason>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: JsonObject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum CacheMissReason {
    Known(KnownCacheMissReason),
    Unknown(TypedObject),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[non_exhaustive]
pub enum KnownCacheMissReason {
    #[serde(rename = "model_changed")]
    ModelChanged { cache_missed_input_tokens: u64 },
    #[serde(rename = "system_changed")]
    SystemChanged { cache_missed_input_tokens: u64 },
    #[serde(rename = "tools_changed")]
    ToolsChanged { cache_missed_input_tokens: u64 },
    #[serde(rename = "messages_changed")]
    MessagesChanged { cache_missed_input_tokens: u64 },
    #[serde(rename = "previous_message_not_found")]
    PreviousMessageNotFound,
    #[serde(rename = "unavailable")]
    Unavailable,
}
