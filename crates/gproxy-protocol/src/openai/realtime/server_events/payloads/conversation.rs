use serde::{Deserialize, Serialize};

use crate::openai::common::Rest;

use super::super::super::RealtimeItem;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RealtimeConversationItemEvent {
    pub item: RealtimeItem,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_item_id: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RealtimeConversationItemRetrievedEvent {
    pub item: RealtimeItem,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RealtimeConversationItemTruncatedEvent {
    pub item_id: String,
    pub content_index: u32,
    pub audio_end_ms: u64,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RealtimeConversationItemDeletedEvent {
    pub item_id: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

/// `conversation.created` carries only the resource it announces.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RealtimeConversationCreatedEvent {
    pub conversation: RealtimeConversationRef,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RealtimeConversationRef {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}
