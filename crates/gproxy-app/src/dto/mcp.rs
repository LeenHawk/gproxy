//! MCP scope rules, tool nature overrides and a server's tools as an
//! administrator reads them.

use gproxy_store::entity::{identity::mcp_scope, upstream::mcp_tool_nature};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct McpScopeDto {
    pub id: String,
    /// The rule's subject: exactly one of the two is set.
    pub user_id: Option<String>,
    pub api_key_id: Option<String>,
    /// `mcp:<server>:<tool>`; either part may be `*`, the tool part may be
    /// `@read`, `@write` or `@destructive`, and `mcp:*` is everything.
    pub scope: String,
    /// `allow` or `deny`.
    pub action: String,
    /// Higher wins; at one priority a refusal wins.
    pub priority: i32,
}

impl From<mcp_scope::Model> for McpScopeDto {
    fn from(row: mcp_scope::Model) -> Self {
        Self {
            id: row.id,
            user_id: row.user_id,
            api_key_id: row.api_key_id,
            scope: row.scope,
            action: row.action,
            priority: row.priority,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct McpScopeWrite {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub api_key_id: Option<String>,
    pub scope: String,
    pub action: String,
    #[serde(default)]
    pub priority: Option<i32>,
}

/// The subject of a rule does not change; delete the rule and write another.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct McpScopePatch {
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub priority: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct McpToolNatureDto {
    pub id: String,
    pub provider_id: String,
    /// The tool's name as the server lists it.
    pub tool: String,
    /// `read`, `write` or `destructive`.
    pub nature: String,
}

impl From<mcp_tool_nature::Model> for McpToolNatureDto {
    fn from(row: mcp_tool_nature::Model) -> Self {
        Self {
            id: row.id,
            provider_id: row.provider_id,
            tool: row.tool,
            nature: row.nature,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct McpToolNatureWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub provider_id: String,
    pub tool: String,
    pub nature: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct McpToolNaturePatch {
    #[serde(default)]
    pub nature: Option<String>,
}

/// One tool of a server, with what the server says it does and what the
/// gateway treats it as.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct McpToolDto {
    pub name: String,
    pub title: Option<String>,
    pub description: Option<String>,
    /// From the server's annotations: `read`, `write` or `destructive`.
    pub declared_nature: String,
    /// The override when there is one, else `declared_nature`.
    pub nature: String,
    /// The override's id, when the administrator set one.
    pub override_id: Option<String>,
}
