use gproxy_core::{QuotaSample, QuotaScope};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CycleTracking {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_observation: Option<super::CredentialQuotaObservation>,
    pub unit: Option<String>,
    pub reset_behavior: gproxy_core::QuotaResetBehavior,
    pub models: std::collections::BTreeMap<String, serde_json::Value>,
    pub needs_rebuild: bool,
    pub scope: QuotaScope,
    pub sample: QuotaSample,
    pub baseline_at_ms: i64,
    pub baseline_percent: Option<Decimal>,
    pub baseline_limit: Option<Decimal>,
    pub uncertain: bool,
    pub local_boundary: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CycleEstimate {
    pub tokens: Option<Decimal>,
    pub cost: Option<Decimal>,
    pub reason: Option<String>,
    pub from_ms: Option<i64>,
    pub to_ms: Option<i64>,
}
