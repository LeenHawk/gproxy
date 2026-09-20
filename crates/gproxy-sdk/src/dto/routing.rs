//! Routes, their members and the public names that select them.

use serde::{Deserialize, Serialize};

use gproxy_store::entity::routing::{exposed_model, route, route_member};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RouteDto {
    pub id: String,
    pub name: String,
    /// `round_robin`, `weighted` or `failover`.
    pub strategy: String,
    pub max_attempts: u32,
    pub enabled: bool,
}

impl From<route::Model> for RouteDto {
    fn from(row: route::Model) -> Self {
        Self {
            id: row.id,
            name: row.name,
            strategy: strategy_name(row.strategy).to_owned(),
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
#[serde(rename_all = "camelCase")]
pub struct RouteWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    #[serde(default)]
    pub strategy: Option<String>,
    #[serde(default)]
    pub max_attempts: Option<u32>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutePatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub strategy: Option<String>,
    #[serde(default)]
    pub max_attempts: Option<u32>,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
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

/// A public model name bound to a route. The name is matched exactly, and its
/// first path segment may not be a registered channel id or a provider name —
/// those prefixes already mean `channel/model` and `provider/model` narrowing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExposedModelDto {
    pub id: String,
    pub name: String,
    pub route_id: String,
    pub enabled: bool,
}

impl From<exposed_model::Model> for ExposedModelDto {
    fn from(row: exposed_model::Model) -> Self {
        Self {
            id: row.id,
            name: row.name,
            route_id: row.route_id,
            enabled: row.enabled,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExposedModelWrite {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub route_id: String,
    #[serde(default)]
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExposedModelPatch {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub route_id: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
}
