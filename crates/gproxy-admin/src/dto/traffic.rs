use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TrafficPolicyDto {
    pub request_headers: Vec<String>,
    pub response_headers: Vec<String>,
    pub request_query: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TrafficBlacklistDto {
    pub request_headers: Vec<String>,
    pub response_headers: Vec<String>,
    pub request_query: Vec<String>,
}

impl From<gproxy_channel_api::ChannelTrafficPolicy> for TrafficPolicyDto {
    fn from(value: gproxy_channel_api::ChannelTrafficPolicy) -> Self {
        gproxy_channel_api::TrafficPolicyConfig::from(value).into()
    }
}

impl From<gproxy_channel_api::TrafficPolicyConfig> for TrafficPolicyDto {
    fn from(value: gproxy_channel_api::TrafficPolicyConfig) -> Self {
        Self {
            request_headers: value.request_headers,
            response_headers: value.response_headers,
            request_query: value.request_query,
        }
    }
}

impl From<TrafficPolicyDto> for gproxy_channel_api::TrafficPolicyConfig {
    fn from(value: TrafficPolicyDto) -> Self {
        Self {
            request_headers: value.request_headers,
            response_headers: value.response_headers,
            request_query: value.request_query,
        }
    }
}

impl From<gproxy_channel_api::TrafficBlacklistConfig> for TrafficBlacklistDto {
    fn from(value: gproxy_channel_api::TrafficBlacklistConfig) -> Self {
        Self {
            request_headers: value.request_headers,
            response_headers: value.response_headers,
            request_query: value.request_query,
        }
    }
}

impl TryFrom<TrafficBlacklistDto> for gproxy_channel_api::TrafficBlacklistConfig {
    type Error = String;

    fn try_from(value: TrafficBlacklistDto) -> Result<Self, Self::Error> {
        Self::new(
            value.request_headers,
            value.response_headers,
            value.request_query,
        )
    }
}
