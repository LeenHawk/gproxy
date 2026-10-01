//! Codex account analytics contracts. Source: samples/codex/codex-rs/
//! codex-backend-openapi-models/src/models/analytics.rs (schema 8dad4dcab85a).
//! Attribution, extensions, null amounts and numeric precision are retained.
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
pub struct ActiveUsersSummary {
    pub total_users: i64,
    pub clients: Vec<ClientActiveUsersCount>,
    pub models: Option<Vec<ModelActivitySummary>>,
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
pub struct ClientActiveUsersCount {
    pub client_id: String,
    pub users: i64,
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
pub struct ClientWorkspaceUsageCount {
    pub client_id: String,
    pub users: i64,
    pub threads: i64,
    pub turns: i64,
    pub credits: Number,
    pub cost_usd: Option<String>,
    pub on_demand_credits: Option<Number>,
    pub uncached_text_input_tokens: Option<i64>,
    pub cached_text_input_tokens: Option<i64>,
    pub text_output_tokens: Option<i64>,
    pub text_total_tokens: Option<i64>,
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
pub struct ConsumptionUsageGroup {
    pub dimensions: std::collections::BTreeMap<String, String>,
    pub is_other: Option<bool>,
    pub credits: Number,
    pub on_demand_credits: Option<Number>,
    pub uncached_text_input_tokens: Option<i64>,
    pub cached_text_input_tokens: Option<i64>,
    pub text_output_tokens: Option<i64>,
    pub text_total_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
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
pub struct CreditUsageEventBySurface {
    pub date: String,
    pub product_surface: String,
    pub credit_amount: Number,
    pub usage_id: Option<String>,
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
pub struct CreditUsageEventsResponse {
    pub data: Vec<CreditUsageEventBySurface>,
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
pub struct CurrentUserCreditUsageDatum {
    pub date: String,
    pub values: std::collections::BTreeMap<String, Number>,
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
pub struct CurrentUserCreditUsageResponse {
    pub breakdown: String,
    pub data: Vec<CurrentUserCreditUsageDatum>,
    pub series: Vec<CurrentUserCreditUsageSeries>,
    pub unit: Option<String>,
    pub data_freshness_ts: Option<String>,
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
pub struct CurrentUserCreditUsageSeries {
    pub key: String,
    pub label: String,
    pub total: Number,
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
pub struct DailyProductSurfaceUsage {
    pub attribution: Option<Vec<PersonalUsageAttribution>>,
    pub date: String,
    pub product_surface_usage_values: std::collections::BTreeMap<String, Number>,
    pub premium_usage_values: Option<ProductSurfacePremiumUsageValues>,
    pub models: Option<Vec<ModelUsage>>,
    pub groups: Option<Vec<ConsumptionUsageGroup>>,
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
pub struct DailyProductSurfaceUsageResponse {
    pub data: Vec<DailyProductSurfaceUsage>,
    pub units: Option<String>,
    pub data_freshness_ts: Option<String>,
    pub group_by: Option<String>,
    pub breakdown_by: Option<Vec<String>>,
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
pub struct DailySkillUsageMetricsResponse {
    pub data: Vec<DailySkillUsageOverview>,
    pub data_freshness_ts: Option<String>,
    pub group_by: Option<String>,
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
pub struct DailySkillUsageOverview {
    pub date: String,
    pub skill_usage_overviews: Vec<SkillUsageOverview>,
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
pub struct DailyWorkspaceUsageCount {
    pub date: String,
    pub totals: WorkspaceUsageCount,
    pub clients: Vec<ClientWorkspaceUsageCount>,
    pub models: Option<Vec<ModelUsage>>,
    pub groups: Option<Vec<WorkspaceUsageGroup>>,
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
pub struct DailyWorkspaceUsageCountResponse {
    pub data: Vec<DailyWorkspaceUsageCount>,
    pub balance_unit: Option<String>,
    pub active_users_summary: Option<ActiveUsersSummary>,
    pub group_by: Option<String>,
    pub breakdown_by: Option<Vec<String>>,
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
pub struct ModelActivitySummary {
    pub model: String,
    pub users: i64,
    pub threads: i64,
    pub turns: i64,
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
pub struct ModelUsage {
    pub model: String,
    pub speed: Option<String>,
    pub credits: Number,
    pub cost_usd: Option<String>,
    pub users: Option<i64>,
    pub threads: Option<i64>,
    pub turns: Option<i64>,
    pub on_demand_credits: Option<Number>,
    pub uncached_text_input_tokens: Option<i64>,
    pub cached_text_input_tokens: Option<i64>,
    pub text_output_tokens: Option<i64>,
    pub text_total_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
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
pub struct PersonalUsageAttribution {
    pub thread_source: String,
    pub turn_trigger: String,
    pub model: String,
    pub surface: String,
    pub value: Number,
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
pub struct PluginUsageBucket {
    pub date: String,
    pub plugin_usage_overviews: Vec<PluginUsageOverview>,
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
pub struct PluginUsageMetricsResponse {
    pub data: Vec<PluginUsageBucket>,
    pub data_freshness_ts: Option<String>,
    pub group_by: Option<String>,
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
pub struct PluginUsageOverview {
    pub plugin_id: Option<String>,
    pub plugin_name: String,
    pub display_name: String,
    pub marketplace: Option<String>,
    pub invocation_counts: i64,
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
pub struct ProductSurfacePremiumUsageValues {
    pub total_usage_credits: std::collections::BTreeMap<String, Number>,
    pub credit_usage_credits: std::collections::BTreeMap<String, Number>,
    pub uncached_text_input_tokens_by_surface: std::collections::BTreeMap<String, i64>,
    pub cached_text_input_tokens_by_surface: std::collections::BTreeMap<String, i64>,
    pub text_output_tokens_by_surface: std::collections::BTreeMap<String, i64>,
    pub text_total_tokens_by_surface: std::collections::BTreeMap<String, i64>,
    pub total_tokens_by_surface: Option<std::collections::BTreeMap<String, i64>>,
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
pub struct SkillUsageOverview {
    pub skill_name: String,
    pub display_name: String,
    pub skill_ids: Vec<String>,
    pub invocation_counts: i64,
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
pub struct WorkspaceUsageCount {
    pub users: i64,
    pub threads: i64,
    pub turns: i64,
    pub credits: Number,
    pub cost_usd: Option<String>,
    pub on_demand_credits: Option<Number>,
    pub uncached_text_input_tokens: Option<i64>,
    pub cached_text_input_tokens: Option<i64>,
    pub text_output_tokens: Option<i64>,
    pub text_total_tokens: Option<i64>,
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
pub struct WorkspaceUsageGroup {
    pub dimensions: std::collections::BTreeMap<String, String>,
    pub is_other: Option<bool>,
    pub users: i64,
    pub turns: i64,
    pub cost_usd: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "serde_json::Map::is_empty")]
    pub rest: Rest,
}

pub type DailyProductSurfaceUsageRequest = WireRequest<()>;
pub type DailyProductSurfaceUsageWireResponse = WireResponse<DailyProductSurfaceUsageResponse>;
pub type CreditUsageEventsRequest = WireRequest<()>;
pub type CreditUsageEventsWireResponse = WireResponse<CreditUsageEventsResponse>;
pub type CurrentUserCreditUsageRequest = WireRequest<()>;
pub type CurrentUserCreditUsageWireResponse = WireResponse<CurrentUserCreditUsageResponse>;
pub type DailyWorkspaceUsageCountRequest = WireRequest<()>;
pub type DailyWorkspaceUsageCountWireResponse = WireResponse<DailyWorkspaceUsageCountResponse>;
pub type PluginUsageMetricsRequest = WireRequest<()>;
pub type PluginUsageMetricsWireResponse = WireResponse<PluginUsageMetricsResponse>;
pub type DailySkillUsageMetricsRequest = WireRequest<()>;
pub type DailySkillUsageMetricsWireResponse = WireResponse<DailySkillUsageMetricsResponse>;
