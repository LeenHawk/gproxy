//! Codex account usage contracts, not the public OpenAI organization API.
//! Sources: samples/codex/codex-rs/backend-client/src/client/{task_usage,plan_history}.rs.
//! Credit strings and JSON numbers retain upstream precision; missing amounts
//! remain unknown. These services do not introduce inference OperationKeys.
use crate::{Rest, WireRequest, WireResponse};
use serde::{Deserialize, Serialize};
use serde_json::Number;

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
pub struct TaskUsageRequestBody {
    pub threads: Vec<TaskUsageThread>,
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
pub struct TaskUsageThread {
    pub thread_id: String,
    pub created_at: Option<String>,
    pub descendant_thread_ids: Vec<String>,
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
pub struct TaskUsageResponseBody {
    pub data_as_of: Option<String>,
    pub threads: Vec<TaskUsage>,
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
pub struct TaskUsage {
    pub thread_id: String,
    pub data_status: TaskUsageStatus,
    pub usage_source: String,
    pub five_hour_limit_percent: Option<Number>,
    pub weekly_limit_percent: Option<Number>,
    pub balance_usage_credits: Option<String>,
    pub groups: Vec<TaskUsageGroup>,
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
pub enum TaskUsageStatus {
    Available,
    Partial,
    Unavailable,
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
pub struct TaskUsageGroup {
    pub product_experience: Option<String>,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub speed: Option<String>,
    pub five_hour_limit_percent: Option<Number>,
    pub weekly_limit_percent: Option<Number>,
    pub balance_usage_credits: Option<String>,
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
pub struct PlanLimitHistory {
    pub data_as_of: Option<String>,
    pub coverage_start: Option<String>,
    pub coverage_complete: bool,
    #[serde(default = "approximate_by_default")]
    pub approximate: bool,
    pub boundary_tolerance_seconds: Option<u32>,
    pub periods: Vec<PlanLimitPeriod>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

fn approximate_by_default() -> bool {
    true
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
pub struct PlanLimitPeriod {
    pub id: String,
    pub window_minutes: u32,
    pub plan_type: String,
    pub starts_at: String,
    pub ends_at: String,
    pub accounting_complete: bool,
    pub used_basis_points: Option<Number>,
    pub breakdowns: Option<Vec<PlanLimitBreakdown>>,
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
pub struct PlanLimitBreakdown {
    pub dimension: String,
    pub rows: Vec<PlanLimitValue>,
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
pub struct PlanLimitValue {
    pub key: String,
    pub basis_points: Number,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

pub type TaskUsageRequest = WireRequest<TaskUsageRequestBody>;
pub type TaskUsageResponse = WireResponse<TaskUsageResponseBody>;
pub type PlanLimitHistoryRequest = WireRequest<()>;
pub type PlanLimitHistoryResponse = WireResponse<PlanLimitHistory>;
