//! Native Realtime wire DTOs. Source definitions are cited on each object.
//! Verified with Realtime.md and openai-python snapshot
//! e12b81d3bbf644ec7045e152d69bc4b68d69cd48 (2026-09-13).
//! https://github.com/openai/openai-python/tree/e12b81d3bbf644ec7045e152d69bc4b68d69cd48/src/openai/types/realtime
use super::wire::present_nullable;
use super::{RealtimeFunctionTool, ToolChoiceFunction, ToolChoiceMcp, ToolChoiceOptions};
use crate::Rest;
use serde::{Deserialize, Serialize};

/// Source: `openai/types/realtime/realtime_session_create_response.py`, `ToolChoice`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(untagged)]
pub enum RealtimeSessionCreateResponseToolChoice {
    ToolChoiceOptions(ToolChoiceOptions),
    ToolChoiceFunction(ToolChoiceFunction),
    ToolChoiceMcp(ToolChoiceMcp),
}

/// Source: `openai/types/realtime/realtime_session_create_response.py`, `Tool`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(untagged)]
#[allow(clippy::large_enum_variant)]
pub enum RealtimeSessionCreateResponseTool {
    RealtimeFunctionTool(RealtimeFunctionTool),
    RealtimeSessionCreateResponseToolMcpTool(RealtimeSessionCreateResponseToolMcpTool),
}

/// Source: `openai/types/realtime/realtime_session_create_response.py`, `ToolMcpTool`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RealtimeSessionCreateResponseToolMcpTool {
    pub server_label: String,
    #[serde(rename = "type")]
    pub type_: RealtimeSessionCreateResponseToolMcpToolType,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub allowed_callers:
        Option<Option<Vec<RealtimeSessionCreateResponseToolMcpToolAllowedCallersItem>>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub allowed_tools: Option<Option<RealtimeSessionCreateResponseToolMcpToolAllowedTools>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub authorization: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub connector_id: Option<Option<RealtimeSessionCreateResponseToolMcpToolConnectorId>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub defer_loading: Option<Option<bool>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub headers: Option<Option<std::collections::BTreeMap<String, String>>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub require_approval: Option<Option<RealtimeSessionCreateResponseToolMcpToolRequireApproval>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub server_description: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub server_url: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub tunnel_id: Option<Option<String>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/realtime_session_create_response.py`, `ToolMcpToolAllowedTools`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(untagged)]
pub enum RealtimeSessionCreateResponseToolMcpToolAllowedTools {
    Items(Vec<String>),
    RealtimeSessionCreateResponseToolMcpToolAllowedToolsMcpToolFilter(
        RealtimeSessionCreateResponseToolMcpToolAllowedToolsMcpToolFilter,
    ),
    Variant3(()),
}

/// Source: `openai/types/realtime/realtime_session_create_response.py`, `ToolMcpToolRequireApproval`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(untagged)]
pub enum RealtimeSessionCreateResponseToolMcpToolRequireApproval {
    RealtimeSessionCreateResponseToolMcpToolRequireApprovalMcpToolApprovalFilter(
        RealtimeSessionCreateResponseToolMcpToolRequireApprovalMcpToolApprovalFilter,
    ),
    Literal(RealtimeSessionCreateResponseToolMcpToolRequireApprovalVariant2),
    Variant3(()),
}

/// Source: `openai/types/realtime/realtime_session_create_response.py`, `ToolMcpToolAllowedToolsMcpToolFilter`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RealtimeSessionCreateResponseToolMcpToolAllowedToolsMcpToolFilter {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub read_only: Option<Option<bool>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub tool_names: Option<Option<Vec<String>>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/realtime_session_create_response.py`, `ToolMcpToolRequireApprovalMcpToolApprovalFilter`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RealtimeSessionCreateResponseToolMcpToolRequireApprovalMcpToolApprovalFilter {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub always: Option<
        Option<RealtimeSessionCreateResponseToolMcpToolRequireApprovalMcpToolApprovalFilterAlways>,
    >,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub never: Option<
        Option<RealtimeSessionCreateResponseToolMcpToolRequireApprovalMcpToolApprovalFilterNever>,
    >,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/realtime_session_create_response.py`, `ToolMcpToolRequireApprovalMcpToolApprovalFilterAlways`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RealtimeSessionCreateResponseToolMcpToolRequireApprovalMcpToolApprovalFilterAlways {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub read_only: Option<Option<bool>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub tool_names: Option<Option<Vec<String>>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

/// Source: `openai/types/realtime/realtime_session_create_response.py`, `ToolMcpToolRequireApprovalMcpToolApprovalFilterNever`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RealtimeSessionCreateResponseToolMcpToolRequireApprovalMcpToolApprovalFilterNever {
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub read_only: Option<Option<bool>>,
    #[serde(
        default,
        deserialize_with = "present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub tool_names: Option<Option<Vec<String>>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RealtimeSessionCreateResponseToolMcpToolType {
    #[serde(rename = "mcp")]
    Mcp,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RealtimeSessionCreateResponseToolMcpToolAllowedCallersItem {
    #[serde(rename = "direct")]
    Direct,
    #[serde(rename = "programmatic")]
    Programmatic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RealtimeSessionCreateResponseToolMcpToolConnectorId {
    #[serde(rename = "connector_dropbox")]
    ConnectorDropbox,
    #[serde(rename = "connector_gmail")]
    ConnectorGmail,
    #[serde(rename = "connector_googlecalendar")]
    ConnectorGooglecalendar,
    #[serde(rename = "connector_googledrive")]
    ConnectorGoogledrive,
    #[serde(rename = "connector_microsoftteams")]
    ConnectorMicrosoftteams,
    #[serde(rename = "connector_outlookcalendar")]
    ConnectorOutlookcalendar,
    #[serde(rename = "connector_outlookemail")]
    ConnectorOutlookemail,
    #[serde(rename = "connector_sharepoint")]
    ConnectorSharepoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum RealtimeSessionCreateResponseToolMcpToolRequireApprovalVariant2 {
    #[serde(rename = "always")]
    Always,
    #[serde(rename = "never")]
    Never,
}
