use serde::{Deserialize, Serialize};

use crate::openai::common::Rest;

use super::super::super::{RealtimeError, RealtimeRateLimit, RealtimeResponse, RealtimeSession};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeErrorEvent {
    pub error: RealtimeError,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeSessionEvent {
    pub session: Box<RealtimeSession>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeRateLimitsEvent {
    pub rate_limits: Vec<RealtimeRateLimit>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RealtimeResponseEvent {
    pub response: Box<RealtimeResponse>,
    #[serde(default, flatten)]
    pub rest: Rest,
}
