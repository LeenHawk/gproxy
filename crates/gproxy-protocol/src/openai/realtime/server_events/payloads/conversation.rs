use serde::{Deserialize, Serialize};

use crate::openai::common::Rest;

use super::super::super::RealtimeItem;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeConversationItemEvent {
    pub item: RealtimeItem,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_item_id: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeConversationItemRetrievedEvent {
    pub item: RealtimeItem,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeConversationItemTruncatedEvent {
    pub item_id: String,
    pub content_index: u32,
    pub audio_end_ms: u64,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeConversationItemDeletedEvent {
    pub item_id: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}
