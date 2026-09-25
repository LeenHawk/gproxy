//! Public model routes and their provider/model members.

use serde::{Deserialize, Serialize};

use gproxy_store::entity::routing::{route, route_member};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct RouteDto {
    pub id: String,
    /// The exact model name clients send in requests.
    pub name: String,
    /// `round_robin`, `weighted` or `failover`.
    pub strategy: String,
    #[serde(default)]
    pub session_affinity: bool,
    pub max_attempts: u32,
    pub enabled: bool,
}

impl From<route::Model> for RouteDto {
    fn from(row: route::Model) -> Self {
        Self {
            id: row.id,
            name: row.name,
            strategy: strategy_name(row.strategy).to_owned(),
            session_affinity: row.session_affinity,
            max_attempts: row.max_attempts,
            enabled: row.enabled,
        }
    }
}

pub(crate) fn strategy_name(strategy: route::RouteStrategy) -> &'static str {
    match strategy {
        route::RouteStrategy::RoundRobin => "round_robin",
        route::RouteStrategy::Weighted => "weighted",
        route::RouteStrategy::Failover => "failover",
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct RouteWrite {
    #[serde(default)]
    pub id: Option<String>,
    /// The exact model name clients send in requests.
    pub name: String,
    #[serde(default)]
    pub strategy: Option<String>,
    #[serde(default)]
    pub session_affinity: Option<bool>,
    #[serde(default)]
    pub max_attempts: Option<u32>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct RoutePatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub strategy: Option<String>,
    #[serde(default)]
    pub session_affinity: Option<bool>,
    #[serde(default)]
    pub max_attempts: Option<u32>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct RouteMemberDto {
    pub id: String,
    pub route_id: String,
    pub provider_id: String,
    /// The upstream model this member sends, not a catalog reference.
    pub upstream_model: String,
    /// Lower tiers are preferred; later tiers are the fallback.
    pub tier: u32,
    /// Positive relative weight within a tier.
    pub weight: u32,
    pub enabled: bool,
}

impl From<route_member::Model> for RouteMemberDto {
    fn from(row: route_member::Model) -> Self {
        Self {
            id: row.id,
            route_id: row.route_id,
            provider_id: row.provider_id,
            upstream_model: row.upstream_model,
            tier: row.tier,
            weight: row.weight,
            enabled: row.enabled,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct RouteMemberWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub route_id: String,
    pub provider_id: String,
    pub upstream_model: String,
    #[serde(default)]
    pub tier: Option<u32>,
    #[serde(default)]
    pub weight: Option<u32>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", ts(rename_all = "camelCase"))]
pub struct RouteMemberPatch {
    #[serde(default)]
    pub route_id: Option<String>,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub upstream_model: Option<String>,
    #[serde(default)]
    pub tier: Option<u32>,
    #[serde(default)]
    pub weight: Option<u32>,
    #[serde(default)]
    pub enabled: Option<bool>,
}
