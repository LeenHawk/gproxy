//! GPT-Live public session protocol, OpenAI reference verified 2026-10-20.
//! Legacy Codex quicksilver/AVAS types remain in realtime::rtc.
use crate::{Rest, WireRequest, WireResponse};
use serde::{Deserialize, Serialize};
#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum LiveVoice {
    Builtin(String),
    Custom(CustomVoice),
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct CustomVoice {
    pub id: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct LiveAudioFormat {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate: Option<u32>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct LiveAudioInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<LiveAudioFormat>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct LiveAudioOutput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<LiveVoice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<LiveAudioFormat>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct LiveAudio {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<LiveAudioInput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<LiveAudioOutput>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct LiveDecisionsConfig {
    pub model: String,
    pub questions: Vec<super::decisions::DecisionQuestion>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct LiveResponsesConfig {
    pub model: String,
    #[serde(
        default,
        deserialize_with = "crate::wire::openai::responses::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub instructions: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::wire::openai::responses::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_output_tokens: Option<Option<u64>>,
    #[serde(
        default,
        deserialize_with = "crate::wire::openai::responses::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub parallel_tool_calls: Option<Option<bool>>,
    #[serde(
        default,
        deserialize_with = "crate::wire::openai::responses::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub reasoning: Option<Option<super::responses::ReasoningConfig>>,
    #[serde(
        default,
        deserialize_with = "crate::wire::openai::responses::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub service_tier: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::wire::openai::responses::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub text: Option<Option<serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<super::responses::Tool>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<serde_json::Value>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LiveDelegation {
    Client {
        #[serde(default, flatten)]
        rest: Rest,
    },
    Responses {
        responses: Box<LiveResponsesConfig>,
        #[serde(default, flatten)]
        rest: Rest,
    },
    Decisions {
        decisions: LiveDecisionsConfig,
        #[serde(default, flatten)]
        rest: Rest,
    },
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct LiveTriggerQuestions {
    pub questions: Vec<super::decisions::DecisionQuestion>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum LiveTriggerEvent {
    #[serde(rename = "session.input_transcript.delta")]
    InputTranscriptDelta,
    #[serde(rename = "session.output_transcript.delta")]
    OutputTranscriptDelta,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum LiveTriggerType {
    Decisions,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct LiveDecisionTrigger {
    #[serde(rename = "type")]
    pub type_: LiveTriggerType,
    pub event_type: LiveTriggerEvent,
    pub decisions: LiveTriggerQuestions,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct LiveServerEventSelector {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_event: Option<serde_json::Value>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum LiveAllowedClientEvents {
    All(String),
    Events(Vec<String>),
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum LiveAllowedServerEvents {
    All(String),
    Events(Vec<LiveServerEventSelector>),
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct LiveDataChannelConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_client_events: Option<LiveAllowedClientEvents>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed_server_events: Option<LiveAllowedServerEvents>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct LiveClientConfig {
    pub data_channel: LiveDataChannelConfig,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct LiveSessionConfig {
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instructions: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audio: Option<LiveAudio>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<LiveClientConfig>,
    #[serde(
        default,
        deserialize_with = "crate::wire::openai::responses::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub delegation: Option<Option<LiveDelegation>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_items: Option<Vec<serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub triggers: Option<Vec<LiveDecisionTrigger>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct LiveSipDigest {
    #[serde(rename = "type")]
    pub type_: String,
    pub username: String,
    pub password: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct LiveSipTrunk {
    pub provider_url: String,
    pub caller_number: String,
    pub auth: LiveSipDigest,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LiveCreateTransport {
    Webrtc {
        sdp: String,
        #[serde(default, flatten)]
        rest: Rest,
    },
    Sip {
        destination: String,
        trunk: LiveSipTrunk,
        #[serde(default, flatten)]
        rest: Rest,
    },
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LiveResponseTransport {
    Webrtc {
        sdp: String,
        #[serde(default, flatten)]
        rest: Rest,
    },
    Sip {
        #[serde(default, flatten)]
        rest: Rest,
    },
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct CreateLiveSessionRequestBody {
    pub session: LiveSessionConfig,
    pub transport: LiveCreateTransport,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct LiveSessionId {
    pub id: String,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct CreateLiveSessionResponseBody {
    pub session: LiveSessionId,
    pub transport: LiveResponseTransport,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct LiveSessionUsage {
    pub seconds: f64,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct LiveDecisionResult {
    pub model: String,
    pub answers: Vec<super::decisions::DecisionAnswer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<super::responses::ResponseUsage>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct LiveDecisionStreamEvent {
    #[serde(rename = "type")]
    pub type_: String,
    pub decision: LiveDecisionResult,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum LiveDelegatedEvent {
    Decision(Box<LiveDecisionStreamEvent>),
    Response(Box<super::responses::stream::StreamEvent>),
    Other(serde_json::Value),
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct LiveServerEvent {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<LiveSessionConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "crate::wire::openai::responses::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub delegation_id: Option<Option<String>>,
    #[serde(
        default,
        deserialize_with = "crate::wire::openai::responses::input::present_nullable",
        skip_serializing_if = "Option::is_none"
    )]
    pub trigger_id: Option<Option<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_event_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_event_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<LiveDelegatedEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<LiveSessionUsage>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[derive(gproxy_protocol_macros::WireBuilder)]
pub struct LiveClientEvent {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<serde_json::Value>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

pub type CreateLiveSessionRequest = WireRequest<CreateLiveSessionRequestBody>;
pub type CreateLiveSessionResponse = WireResponse<CreateLiveSessionResponseBody>;
