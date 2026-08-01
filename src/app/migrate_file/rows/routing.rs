use serde::Deserialize;
use serde_json::Value;

use super::{default_true, default_weight};
use crate::store::persistence::records::{AliasInput, RouteInput, RouteMemberInput};

fn default_provider() -> String {
    "*".to_owned()
}
fn default_route_strategy() -> String {
    "failover".to_owned()
}

#[derive(Deserialize)]
pub(crate) struct LegacyRoute {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default = "default_route_strategy")]
    pub strategy: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub settings_json: Option<Value>,
}

impl From<LegacyRoute> for RouteInput {
    fn from(x: LegacyRoute) -> Self {
        Self {
            id: Some(x.id),
            name: x.name,
            strategy: x.strategy,
            enabled: x.enabled,
            description: x.description,
            settings_json: x.settings_json,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct LegacyRouteMember {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub route_id: i64,
    #[serde(default)]
    pub provider_id: i64,
    #[serde(default)]
    pub upstream_model_id: String,
    #[serde(default = "default_weight")]
    pub weight: i64,
    #[serde(default)]
    pub tier: i64,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl From<LegacyRouteMember> for RouteMemberInput {
    fn from(x: LegacyRouteMember) -> Self {
        Self {
            id: Some(x.id),
            route_id: x.route_id,
            provider_id: x.provider_id,
            upstream_model_id: x.upstream_model_id,
            weight: x.weight,
            tier: x.tier,
            enabled: x.enabled,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct LegacyAlias {
    #[serde(default)]
    pub id: i64,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default)]
    pub alias: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub sort_order: i64,
    #[serde(default = "default_true")]
    pub enabled: bool,
}

impl From<LegacyAlias> for AliasInput {
    fn from(x: LegacyAlias) -> Self {
        Self {
            id: Some(x.id),
            provider: x.provider,
            alias: x.alias,
            target: Some(x.target),
            sort_order: x.sort_order,
            enabled: x.enabled,
        }
    }
}
