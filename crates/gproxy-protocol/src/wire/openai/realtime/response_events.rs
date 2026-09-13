//! Native Realtime wire DTOs. Source definitions are cited on each object.
//! Verified with Realtime.md and openai-python snapshot
//! e12b81d3bbf644ec7045e152d69bc4b68d69cd48 (2026-09-13).
//! https://github.com/openai/openai-python/tree/e12b81d3bbf644ec7045e152d69bc4b68d69cd48/src/openai/types/realtime
use super::wire::present_nullable;
use super::{ConversationItem, RealtimeError, RealtimeResponse};
use crate::Rest;
use serde::{Deserialize, Serialize};

/// Source: `openai/types/realtime/realtime_error_event.py`, `RealtimeErrorEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct RealtimeErrorEvent {
    pub error: RealtimeError,
    pub event_id: String,
    #[serde(rename = "type")]
    pub type_: RealtimeErrorEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/rate_limits_updated_event.py`, `RateLimitsUpdatedEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct RateLimitsUpdatedEvent {
    pub event_id: String,
    pub rate_limits: Vec<RateLimitsUpdatedEventRateLimit>,
    #[serde(rename = "type")]
    pub type_: RateLimitsUpdatedEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/response_content_part_added_event.py`, `ResponseContentPartAddedEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseContentPartAddedEvent {
    pub content_index: i64,
    pub event_id: String,
    pub item_id: String,
    pub output_index: i64,
    pub part: ResponseContentPartAddedEventPart,
    pub response_id: String,
    #[serde(rename = "type")]
    pub type_: ResponseContentPartAddedEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/response_content_part_done_event.py`, `ResponseContentPartDoneEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseContentPartDoneEvent {
    pub content_index: i64,
    pub event_id: String,
    pub item_id: String,
    pub output_index: i64,
    pub part: ResponseContentPartDoneEventPart,
    pub response_id: String,
    #[serde(rename = "type")]
    pub type_: ResponseContentPartDoneEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/response_created_event.py`, `ResponseCreatedEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseCreatedEvent {
    pub event_id: String,
    pub response: RealtimeResponse,
    #[serde(rename = "type")]
    pub type_: ResponseCreatedEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/response_done_event.py`, `ResponseDoneEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseDoneEvent {
    pub event_id: String,
    pub response: RealtimeResponse,
    #[serde(rename = "type")]
    pub type_: ResponseDoneEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/response_function_call_arguments_delta_event.py`, `ResponseFunctionCallArgumentsDeltaEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseFunctionCallArgumentsDeltaEvent {
    pub call_id: String,
    pub delta: String,
    pub event_id: String,
    pub item_id: String,
    pub output_index: i64,
    pub response_id: String,
    #[serde(rename = "type")]
    pub type_: ResponseFunctionCallArgumentsDeltaEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/response_function_call_arguments_done_event.py`, `ResponseFunctionCallArgumentsDoneEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseFunctionCallArgumentsDoneEvent {
    pub arguments: String,
    pub call_id: String,
    pub event_id: String,
    pub item_id: String,
    pub name: String,
    pub output_index: i64,
    pub response_id: String,
    #[serde(rename = "type")]
    pub type_: ResponseFunctionCallArgumentsDoneEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/response_output_item_added_event.py`, `ResponseOutputItemAddedEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseOutputItemAddedEvent {
    pub event_id: String,
    pub item: ConversationItem,
    pub output_index: i64,
    pub response_id: String,
    #[serde(rename = "type")]
    pub type_: ResponseOutputItemAddedEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/response_output_item_done_event.py`, `ResponseOutputItemDoneEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseOutputItemDoneEvent {
    pub event_id: String,
    pub item: ConversationItem,
    pub output_index: i64,
    pub response_id: String,
    #[serde(rename = "type")]
    pub type_: ResponseOutputItemDoneEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/response_text_delta_event.py`, `ResponseTextDeltaEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseTextDeltaEvent {
    pub content_index: i64,
    pub delta: String,
    pub event_id: String,
    pub item_id: String,
    pub output_index: i64,
    pub response_id: String,
    #[serde(rename = "type")]
    pub type_: ResponseTextDeltaEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/response_text_done_event.py`, `ResponseTextDoneEvent`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseTextDoneEvent {
    pub content_index: i64,
    pub event_id: String,
    pub item_id: String,
    pub output_index: i64,
    pub response_id: String,
    pub text: String,
    #[serde(rename = "type")]
    pub type_: ResponseTextDoneEventType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/rate_limits_updated_event.py`, `RateLimit`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct RateLimitsUpdatedEventRateLimit {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub limit: Option<Option<i64>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub name: Option<Option<RateLimitsUpdatedEventRateLimitName>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub remaining: Option<Option<i64>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub reset_seconds: Option<Option<serde_json::Number>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/response_content_part_added_event.py`, `Part`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseContentPartAddedEventPart {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub audio: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub text: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub transcript: Option<Option<String>>,
    #[serde(rename = "type")]
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub type_: Option<Option<ResponseContentPartAddedEventPartType>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/response_content_part_done_event.py`, `Part`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseContentPartDoneEventPart {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub audio: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub text: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub transcript: Option<Option<String>>,
    #[serde(rename = "type")]
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub type_: Option<Option<ResponseContentPartDoneEventPartType>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum RealtimeErrorEventType {
    #[serde(rename = "error")]
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum RateLimitsUpdatedEventType {
    #[serde(rename = "rate_limits.updated")]
    RateLimitsUpdated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseContentPartAddedEventType {
    #[serde(rename = "response.content_part.added")]
    ResponseContentPartAdded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseContentPartDoneEventType {
    #[serde(rename = "response.content_part.done")]
    ResponseContentPartDone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseCreatedEventType {
    #[serde(rename = "response.created")]
    ResponseCreated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseDoneEventType {
    #[serde(rename = "response.done")]
    ResponseDone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseFunctionCallArgumentsDeltaEventType {
    #[serde(rename = "response.function_call_arguments.delta")]
    ResponseFunctionCallArgumentsDelta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseFunctionCallArgumentsDoneEventType {
    #[serde(rename = "response.function_call_arguments.done")]
    ResponseFunctionCallArgumentsDone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseOutputItemAddedEventType {
    #[serde(rename = "response.output_item.added")]
    ResponseOutputItemAdded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseOutputItemDoneEventType {
    #[serde(rename = "response.output_item.done")]
    ResponseOutputItemDone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseTextDeltaEventType {
    #[serde(rename = "response.output_text.delta")]
    ResponseOutputTextDelta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseTextDoneEventType {
    #[serde(rename = "response.output_text.done")]
    ResponseOutputTextDone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum RateLimitsUpdatedEventRateLimitName {
    #[serde(rename = "requests")]
    Requests,
    #[serde(rename = "tokens")]
    Tokens,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseContentPartAddedEventPartType {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "audio")]
    Audio,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseContentPartDoneEventPartType {
    #[serde(rename = "text")]
    Text,
    #[serde(rename = "audio")]
    Audio,
}
