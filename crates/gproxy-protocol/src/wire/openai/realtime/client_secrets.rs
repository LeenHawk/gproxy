//! OpenAI Realtime client secrets, current API reference (2026-10-20).
use crate::{Rest, WireRequest, WireResponse};
use serde::{Deserialize, Serialize};
#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum ClientSecretSession {
    Realtime(Box<super::RealtimeSessionCreateRequest>),
    Transcription(Box<super::RealtimeTranscriptionSessionCreateRequest>),
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ClientSecretExpiresAfter {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seconds: Option<u64>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CreateRealtimeClientSecretRequestBody {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_after: Option<ClientSecretExpiresAfter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<ClientSecretSession>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

#[derive(
    Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(untagged)]
pub enum ClientSecretResponseSession {
    Realtime(Box<super::RealtimeSessionCreateResponse>),
    Transcription(Box<super::RealtimeTranscriptionSessionCreateResponse>),
}

#[derive(
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    gproxy_protocol_macros::WireBuilder,
    gproxy_protocol_macros::DeclaredFields,
)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CreateRealtimeClientSecretResponseBody {
    pub value: String,
    pub expires_at: u64,
    pub session: ClientSecretResponseSession,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

pub type CreateRealtimeClientSecretRequest = WireRequest<CreateRealtimeClientSecretRequestBody>;
pub type CreateRealtimeClientSecretResponse = WireResponse<CreateRealtimeClientSecretResponseBody>;

impl std::fmt::Debug for CreateRealtimeClientSecretResponseBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CreateRealtimeClientSecretResponseBody")
            .field("value", &"[redacted]")
            .field("expires_at", &self.expires_at)
            .finish_non_exhaustive()
    }
}
