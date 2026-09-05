use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct QuotaCapabilitiesDto {
    pub probe: bool,
    pub reset: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct CycleObservationDto {
    pub observed_at_ms: i64,
    pub unit: Option<String>,
    pub upstream_used: Option<String>,
    pub upstream_limit: Option<String>,
    pub used_percent: Option<String>,
    pub estimate: Option<CycleEstimateDto>,
}

impl From<gproxy_store::records::CycleObservationRecord> for CycleObservationDto {
    fn from(value: gproxy_store::records::CycleObservationRecord) -> Self {
        Self {
            observed_at_ms: value.observed_at_ms,
            unit: value.unit,
            upstream_used: value
                .upstream_used
                .map(|value| value.normalize().to_string()),
            upstream_limit: value
                .upstream_limit
                .map(|value| value.normalize().to_string()),
            used_percent: value
                .used_percent
                .map(|value| value.normalize().to_string()),
            estimate: value.estimate.map(Into::into),
        }
    }
}

impl From<gproxy_channel_api::QuotaCapabilities> for QuotaCapabilitiesDto {
    fn from(value: gproxy_channel_api::QuotaCapabilities) -> Self {
        Self {
            probe: value.probe,
            reset: value.reset,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct CycleEstimateDto {
    pub tokens: Option<String>,
    pub cost: Option<String>,
    pub reason: Option<String>,
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
}

impl From<gproxy_store::records::CycleEstimate> for CycleEstimateDto {
    fn from(value: gproxy_store::records::CycleEstimate) -> Self {
        Self {
            tokens: value.tokens.map(|value| value.normalize().to_string()),
            cost: value.cost.map(|value| value.normalize().to_string()),
            reason: value.reason,
            from_ms: value.from_ms,
            to_ms: value.to_ms,
        }
    }
}
