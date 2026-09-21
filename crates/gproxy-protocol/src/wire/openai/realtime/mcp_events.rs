//! Native Realtime wire DTOs. Source definitions are cited on each object.
//! Verified with Realtime.md and openai-python snapshot
//! e12b81d3bbf644ec7045e152d69bc4b68d69cd48 (2026-09-13).
//! https://github.com/openai/openai-python/tree/e12b81d3bbf644ec7045e152d69bc4b68d69cd48/src/openai/types/realtime
use super::wire::present_nullable;
use crate::Rest;
use serde::{Deserialize, Serialize};

/// Source: `openai/types/realtime/mcp_list_tools_in_progress.py`, `McpListToolsInProgress`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct McpListToolsInProgress {
    pub event_id: String,
    pub item_id: String,
    #[serde(rename = "type")]
    pub type_: McpListToolsInProgressType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/mcp_list_tools_completed.py`, `McpListToolsCompleted`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct McpListToolsCompleted {
    pub event_id: String,
    pub item_id: String,
    #[serde(rename = "type")]
    pub type_: McpListToolsCompletedType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/mcp_list_tools_failed.py`, `McpListToolsFailed`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct McpListToolsFailed {
    pub event_id: String,
    pub item_id: String,
    #[serde(rename = "type")]
    pub type_: McpListToolsFailedType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/response_mcp_call_arguments_delta.py`, `ResponseMcpCallArgumentsDelta`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseMcpCallArgumentsDelta {
    pub delta: String,
    pub event_id: String,
    pub item_id: String,
    pub output_index: i64,
    pub response_id: String,
    #[serde(rename = "type")]
    pub type_: ResponseMcpCallArgumentsDeltaType,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub obfuscation: Option<Option<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/response_mcp_call_arguments_done.py`, `ResponseMcpCallArgumentsDone`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseMcpCallArgumentsDone {
    pub arguments: String,
    pub event_id: String,
    pub item_id: String,
    pub output_index: i64,
    pub response_id: String,
    #[serde(rename = "type")]
    pub type_: ResponseMcpCallArgumentsDoneType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/response_mcp_call_in_progress.py`, `ResponseMcpCallInProgress`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseMcpCallInProgress {
    pub event_id: String,
    pub item_id: String,
    pub output_index: i64,
    #[serde(rename = "type")]
    pub type_: ResponseMcpCallInProgressType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/response_mcp_call_completed.py`, `ResponseMcpCallCompleted`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseMcpCallCompleted {
    pub event_id: String,
    pub item_id: String,
    pub output_index: i64,
    #[serde(rename = "type")]
    pub type_: ResponseMcpCallCompletedType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/response_mcp_call_failed.py`, `ResponseMcpCallFailed`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub struct ResponseMcpCallFailed {
    pub event_id: String,
    pub item_id: String,
    pub output_index: i64,
    #[serde(rename = "type")]
    pub type_: ResponseMcpCallFailedType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum McpListToolsInProgressType {
    #[serde(rename = "mcp_list_tools.in_progress")]
    McpListToolsInProgress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum McpListToolsCompletedType {
    #[serde(rename = "mcp_list_tools.completed")]
    McpListToolsCompleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum McpListToolsFailedType {
    #[serde(rename = "mcp_list_tools.failed")]
    McpListToolsFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseMcpCallArgumentsDeltaType {
    #[serde(rename = "response.mcp_call_arguments.delta")]
    ResponseMcpCallArgumentsDelta,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseMcpCallArgumentsDoneType {
    #[serde(rename = "response.mcp_call_arguments.done")]
    ResponseMcpCallArgumentsDone,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseMcpCallInProgressType {
    #[serde(rename = "response.mcp_call.in_progress")]
    ResponseMcpCallInProgress,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseMcpCallCompletedType {
    #[serde(rename = "response.mcp_call.completed")]
    ResponseMcpCallCompleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[derive(gproxy_protocol_macros::DeclaredFields)]
pub enum ResponseMcpCallFailedType {
    #[serde(rename = "response.mcp_call.failed")]
    ResponseMcpCallFailed,
}
