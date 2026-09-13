//! Native Realtime wire DTOs. Source definitions are cited on each object.
//! Verified with Realtime.md and openai-python snapshot
//! e12b81d3bbf644ec7045e152d69bc4b68d69cd48 (2026-09-13).
//! https://github.com/openai/openai-python/tree/e12b81d3bbf644ec7045e152d69bc4b68d69cd48/src/openai/types/realtime
use super::ConversationItem;
use super::wire::present_nullable;
use crate::Rest;
use serde::{Deserialize, Serialize};

/// Source: `openai/types/realtime/conversation_created_event.py`, `ConversationCreatedEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ConversationCreatedEvent {
    pub conversation: ConversationCreatedEventConversation,
    pub event_id: String,
    #[serde(rename = "type")]
    pub type_: ConversationCreatedEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/conversation_item_created_event.py`, `ConversationItemCreatedEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ConversationItemCreatedEvent {
    pub event_id: String,
    pub item: ConversationItem,
    #[serde(rename = "type")]
    pub type_: ConversationItemCreatedEventType,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub previous_item_id: Option<Option<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/conversation_item_deleted_event.py`, `ConversationItemDeletedEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ConversationItemDeletedEvent {
    pub event_id: String,
    pub item_id: String,
    #[serde(rename = "type")]
    pub type_: ConversationItemDeletedEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/conversation_item_truncated_event.py`, `ConversationItemTruncatedEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ConversationItemTruncatedEvent {
    pub audio_end_ms: i64,
    pub content_index: i64,
    pub event_id: String,
    pub item_id: String,
    #[serde(rename = "type")]
    pub type_: ConversationItemTruncatedEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/conversation_item_added.py`, `ConversationItemAdded`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ConversationItemAdded {
    pub event_id: String,
    pub item: ConversationItem,
    #[serde(rename = "type")]
    pub type_: ConversationItemAddedType,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub previous_item_id: Option<Option<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/conversation_item_done.py`, `ConversationItemDone`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ConversationItemDone {
    pub event_id: String,
    pub item: ConversationItem,
    #[serde(rename = "type")]
    pub type_: ConversationItemDoneType,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub previous_item_id: Option<Option<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/conversation_created_event.py`, `Conversation`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ConversationCreatedEventConversation {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub id: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub object: Option<Option<ConversationCreatedEventConversationObject>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ConversationCreatedEventType {
    #[serde(rename = "conversation.created")]
    ConversationCreated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ConversationItemCreatedEventType {
    #[serde(rename = "conversation.item.created")]
    ConversationItemCreated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ConversationItemDeletedEventType {
    #[serde(rename = "conversation.item.deleted")]
    ConversationItemDeleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ConversationItemTruncatedEventType {
    #[serde(rename = "conversation.item.truncated")]
    ConversationItemTruncated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ConversationItemAddedType {
    #[serde(rename = "conversation.item.added")]
    ConversationItemAdded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ConversationItemDoneType {
    #[serde(rename = "conversation.item.done")]
    ConversationItemDone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ConversationCreatedEventConversationObject {
    #[serde(rename = "realtime.conversation")]
    RealtimeConversation,
}
