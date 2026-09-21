//! Per-provider operation overrides: what an operation is remapped to, and
//! which URL serves it.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::double_option;
use gproxy_store::entity::upstream::{operation_endpoint, operation_rule};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct OperationRuleDto {
    pub id: String,
    pub provider_id: String,
    /// A `gproxy_protocol::Operation` id, e.g. `generate_content`.
    pub operation: String,
    pub action: String,
    /// Action-specific target, e.g. the destination `OperationKey`.
    #[cfg_attr(feature = "ts", ts(type = "unknown | null"))]
    pub target: Option<Value>,
}

impl From<operation_rule::Model> for OperationRuleDto {
    fn from(row: operation_rule::Model) -> Self {
        Self {
            id: row.id,
            provider_id: row.provider_id,
            operation: row.operation,
            action: row.action,
            target: row.target,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct OperationRuleWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub provider_id: String,
    pub operation: String,
    pub action: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts", ts(type = "unknown | null"))]
    pub target: Option<Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct OperationRulePatch {
    #[serde(default)]
    pub operation: Option<String>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default, deserialize_with = "double_option")]
    #[cfg_attr(feature = "ts", ts(type = "unknown | null"))]
    pub target: Option<Option<Value>>,
}

/// A complete method URL for one `(operation, dialect, transport)` of one
/// provider. It replaces the URL the channel would build, not just its host.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct OperationEndpointDto {
    pub id: String,
    pub provider_id: String,
    pub operation: String,
    /// A `gproxy_protocol::Dialect` id, e.g. `openai`.
    pub dialect: String,
    /// `http` or `websocket`.
    pub transport: String,
    pub url: String,
    pub enabled: bool,
}

impl From<operation_endpoint::Model> for OperationEndpointDto {
    fn from(row: operation_endpoint::Model) -> Self {
        Self {
            id: row.id,
            provider_id: row.provider_id,
            operation: row.operation,
            dialect: row.dialect,
            transport: transport_name(row.transport).to_owned(),
            url: row.url,
            enabled: row.enabled,
        }
    }
}

pub(crate) fn transport_name(transport: operation_endpoint::EndpointTransport) -> &'static str {
    match transport {
        operation_endpoint::EndpointTransport::Http => "http",
        operation_endpoint::EndpointTransport::WebSocket => "websocket",
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct OperationEndpointWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub provider_id: String,
    pub operation: String,
    pub dialect: String,
    #[serde(default)]
    pub transport: Option<String>,
    pub url: String,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct OperationEndpointPatch {
    #[serde(default)]
    pub operation: Option<String>,
    #[serde(default)]
    pub dialect: Option<String>,
    #[serde(default)]
    pub transport: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}
