//! Gemini Developer Live BidiGenerateContent wire models, checked 2026-09-13.
//! https://ai.google.dev/api/live
//! googleapis/googleapis generative_service.proto commit
//! efc9e8f560a5f1b9b08b62823bfa7955c656cb74;
//! googleapis/python-genai types.py and _live_converters.py commit
//! 9052efddccae9f91e699d9b76e0d5a3e032637f5.
//! The current reference/SDK include fields absent from the public proto; mldev
//! converter mappings determine Developer API wire placement. Vertex-only options
//! are not added. Optional ProtoJSON null is treated as unset; explicit false/0
//! remains present. Sibling oneof keys are retained without engine validation.
use crate::Rest;
use crate::gemini::FunctionResponse;
use crate::{WireRequest, WireResponse};
use serde::{Deserialize, Serialize};
mod content;
pub use content::*;
mod generation;
mod realtime;
mod server;
mod setup;
pub use generation::*;
pub use realtime::*;
pub use server::*;
pub use setup::*;
/// HTTP handshake metadata; the established link is crate::WebSocket.
pub type LiveRequest = WireRequest<()>;
pub type LiveResponse = WireResponse<()>;
/// ProtoJSON int64 fields allow decimal strings and numeric JSON inputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum LiveInt64 {
    String(#[serde(deserialize_with = "decimal_i64")] String),
    Number(i64),
}
fn decimal_i64<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let v = String::deserialize(d)?;
    v.parse::<i64>().map_err(serde::de::Error::custom)?;
    Ok(v)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct LiveClientMessage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup: Option<BidiGenerateContentSetup>,
    #[serde(alias = "client_content")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_content: Option<BidiGenerateContentClientContent>,
    #[serde(alias = "realtime_input")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub realtime_input: Option<BidiGenerateContentRealtimeInput>,
    #[serde(alias = "tool_response")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_response: Option<BidiGenerateContentToolResponse>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct LiveServerMessage {
    #[serde(alias = "setup_complete")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup_complete: Option<BidiGenerateContentSetupComplete>,
    #[serde(alias = "server_content")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_content: Option<BidiGenerateContentServerContent>,
    #[serde(alias = "tool_call")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call: Option<BidiGenerateContentToolCall>,
    #[serde(alias = "tool_call_cancellation")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_cancellation: Option<BidiGenerateContentToolCallCancellation>,
    #[serde(alias = "go_away")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub go_away: Option<GoAway>,
    #[serde(alias = "session_resumption_update")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_resumption_update: Option<SessionResumptionUpdate>,
    #[serde(alias = "usage_metadata")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage_metadata: Option<LiveUsageMetadata>,
    #[serde(alias = "voice_activity_detection_signal")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_activity_detection_signal: Option<VoiceActivityDetectionSignal>,
    #[serde(alias = "voice_activity")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice_activity: Option<VoiceActivity>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct BidiGenerateContentClientContent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turns: Option<Vec<LiveContent>>,
    #[serde(alias = "turn_complete")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_complete: Option<bool>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct BidiGenerateContentToolResponse {
    #[serde(alias = "function_responses")]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function_responses: Option<Vec<FunctionResponse>>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}
