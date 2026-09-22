//! Responses prompt-cache diagnostics (September 2026 API reference).
use crate::Rest;
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, gproxy_protocol_macros::DeclaredFields,
)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum PromptCacheDiagnostics {
    CacheMiss(CacheMiss),
    CacheHit(DiagnosticOutcome),
    ComparisonResponseNotFound(DiagnosticOutcome),
    Unavailable(DiagnosticOutcome),
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
pub struct CacheMiss {
    pub cache_missed_tokens: i64,
    pub reason: CacheMissReason,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub comparison_reusable_tokens: Option<i64>,
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
pub struct DiagnosticOutcome {
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
pub enum CacheMissReason {
    ModelChanged,
    PromptCacheKeyChanged,
    ServiceTierChanged,
    ToolsChanged,
    TextFormatChanged,
    ReasoningEffortChanged,
    VerbosityChanged,
    ContextCompacted,
    InputChanged,
}
