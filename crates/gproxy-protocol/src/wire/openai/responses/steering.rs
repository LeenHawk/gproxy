//! Native Responses WebSocket steering events. Acceptance is not continuation commit.
use super::input;
use crate::Rest;
use serde::{Deserialize, Serialize};

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SteerRequest {
    pub previous_response_id: String,
    pub input: SteerInput,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum SteerInput {
    Text(String),
    Messages(Vec<SteerMessage>),
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SteerMessage {
    pub role: SteerRole,
    pub content: input::MessageContent,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<input::MessageType>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum SteerRole {
    User,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum SteeringEvent {
    #[serde(rename = "response.steer.accepted")]
    Accepted(SteerAccepted),
    #[serde(rename = "response.steer.pending")]
    Pending(SteerPending),
    #[serde(rename = "response.steer.failed")]
    Failed(SteerFailed),
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SteeringReference {
    pub id: String,
    pub previous_response_id: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SteerAccepted {
    pub sequence_number: i64,
    pub steer: SteeringReference,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SteerPending {
    pub sequence_number: i64,
    pub steer: SteeringReference,
    pub reason: String,
    pub required_input: Vec<RequiredSteeringInput>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SteerFailed {
    pub sequence_number: i64,
    pub steer: FailedSteeringSubmission,
    pub error: SteerError,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct FailedSteeringSubmission {
    pub previous_response_id: String,
    pub input: SteerInput,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SteerError {
    pub code: String,
    pub message: String,
    #[serde(rename = "type")]
    pub type_: SteerErrorType,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum SteerErrorType {
    InvalidRequestError,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum RequiredSteeringInput {
    FunctionCallOutput {
        call_id: String,
        name: String,
    },
    CustomToolCallOutput {
        call_id: String,
    },
    ComputerCallOutput {
        call_id: String,
    },
    ShellCallOutput {
        call_id: String,
    },
    ApplyPatchCallOutput {
        call_id: String,
    },
    ToolSearchOutput {
        call_id: String,
        execution: SteeringToolSearchExecution,
    },
    McpApprovalResponse {
        approval_request_id: String,
    },
}

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::DeclaredFields,
)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum SteeringToolSearchExecution {
    Client,
}
