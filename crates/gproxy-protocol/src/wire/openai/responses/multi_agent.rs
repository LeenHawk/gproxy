//! Responses Multi-agent beta: hosted collaboration and in-flight tool outputs.
//! Source: https://developers.openai.com/api/reference/resources/beta/subresources/responses
//! Guide: https://developers.openai.com/api/docs/guides/responses-multi-agent
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
pub struct Agent {
    pub agent_name: String,
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
pub struct MultiAgentConfig {
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_concurrent_subagents: Option<serde_json::Number>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(rename_all = "snake_case")]
pub enum Action {
    SpawnAgent,
    InterruptAgent,
    ListAgents,
    SendMessage,
    FollowupTask,
    WaitAgent,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum MultiAgentCallType {
    #[serde(rename = "multi_agent_call")]
    MultiAgentCall,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum MultiAgentCallOutputType {
    #[serde(rename = "multi_agent_call_output")]
    MultiAgentCallOutput,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum AgentMessageType {
    #[serde(rename = "agent_message")]
    AgentMessage,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum EncryptedContentType {
    #[serde(rename = "encrypted_content")]
    EncryptedContent,
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
pub struct MultiAgentCall {
    #[serde(rename = "type")]
    pub type_: MultiAgentCallType,
    #[serde(
        default,
        deserialize_with = "super::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub id: Option<Option<String>>,
    pub action: Action,
    pub arguments: String,
    pub call_id: String,
    #[serde(
        default,
        deserialize_with = "super::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub agent: Option<Option<super::multi_agent::Agent>>,
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
pub struct MultiAgentCallOutput {
    #[serde(rename = "type")]
    pub type_: MultiAgentCallOutputType,
    #[serde(
        default,
        deserialize_with = "super::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub id: Option<Option<String>>,
    pub action: Action,
    pub call_id: String,
    pub output: Vec<ReplayOutputText>,
    #[serde(
        default,
        deserialize_with = "super::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub agent: Option<Option<super::multi_agent::Agent>>,
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
pub struct AgentMessage {
    #[serde(rename = "type")]
    pub type_: AgentMessageType,
    #[serde(
        default,
        deserialize_with = "super::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub id: Option<Option<String>>,
    pub author: String,
    pub recipient: String,
    pub content: Vec<AgentContent>,
    #[serde(
        default,
        deserialize_with = "super::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub agent: Option<Option<super::multi_agent::Agent>>,
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
pub struct ResponseMultiAgentCall {
    #[serde(rename = "type")]
    pub type_: MultiAgentCallType,
    pub id: String,
    pub action: Action,
    pub arguments: String,
    pub call_id: String,
    #[serde(
        default,
        deserialize_with = "super::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub agent: Option<Option<super::multi_agent::Agent>>,
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
pub struct ResponseMultiAgentCallOutput {
    #[serde(rename = "type")]
    pub type_: MultiAgentCallOutputType,
    pub id: String,
    pub action: Action,
    pub call_id: String,
    pub output: Vec<input::ResponseOutputText>,
    #[serde(
        default,
        deserialize_with = "super::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub agent: Option<Option<super::multi_agent::Agent>>,
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
pub struct ResponseAgentMessage {
    #[serde(rename = "type")]
    pub type_: AgentMessageType,
    pub id: String,
    pub author: String,
    pub recipient: String,
    pub content: Vec<AgentContent>,
    #[serde(
        default,
        deserialize_with = "super::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub agent: Option<Option<super::multi_agent::Agent>>,
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
pub struct ReplayOutputText {
    #[serde(rename = "type")]
    pub type_: input::ResponseOutputTextType,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub annotations: Option<Vec<input::OutputAnnotation>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<Vec<input::OutputLogprob>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(untagged)]
pub enum AgentContent {
    InputText(input::ResponseInputText),
    OutputText(ReplayOutputText),
    Text(AgentText),
    Summary(input::SummaryText),
    Reasoning(input::ReasoningContent),
    Refusal(input::ResponseOutputRefusal),
    Image(input::ResponseInputImage),
    Screenshot(input::ComputerScreenshot),
    File(input::ResponseInputFile),
    Encrypted(EncryptedContent),
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
pub struct AgentText {
    #[serde(rename = "type")]
    pub type_: AgentTextType,
    pub text: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(rename_all = "snake_case")]
pub enum AgentTextType {
    Text,
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
pub struct EncryptedContent {
    #[serde(rename = "type")]
    pub type_: EncryptedContentType,
    pub encrypted_content: String,
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
pub struct InjectRequest {
    pub response_id: String,
    pub input: Vec<input::InputItem>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[serde(tag = "type")]
pub enum InjectionEvent {
    #[serde(rename = "response.inject.created")]
    Created(InjectCreated),
    #[serde(rename = "response.inject.failed")]
    Failed(InjectFailed),
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
pub struct InjectCreated {
    pub sequence_number: i64,
    pub response_id: String,
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
pub struct InjectFailed {
    pub sequence_number: i64,
    pub response_id: String,
    pub input: Vec<input::InputItem>,
    pub error: InjectError,
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
pub struct InjectError {
    pub code: InjectErrorCode,
    pub message: String,
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
pub enum InjectErrorCode {
    ResponseAlreadyCompleted,
    ResponseNotFound,
}
