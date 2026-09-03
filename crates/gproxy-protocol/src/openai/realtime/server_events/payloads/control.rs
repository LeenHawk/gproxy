use serde::{Deserialize, Serialize};

use crate::openai::common::Rest;

use super::super::super::{RealtimeError, RealtimeRateLimit, RealtimeResponse, RealtimeSession};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RealtimeErrorEvent {
    pub error: RealtimeError,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RealtimeSessionEvent {
    pub session: Box<RealtimeSession>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RealtimeRateLimitsEvent {
    pub rate_limits: Vec<RealtimeRateLimit>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RealtimeResponseEvent {
    pub response: Box<RealtimeResponse>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

/// A telephony keypad press relayed by the server; `received_at` is a UTC Unix
/// timestamp, not a session-relative offset.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct RealtimeDtmfEvent {
    pub event: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub received_at: Option<f64>,
    #[serde(default, flatten)]
    pub rest: Rest,
}
