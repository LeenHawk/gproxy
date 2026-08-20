use rust_decimal::Decimal;
use serde::Deserialize;

use super::deserialize_decimal;
use crate::store::persistence::records::{QuotaInput, RateLimitInput, RoutePermissionInput, Scope};

fn default_scope() -> Scope {
    Scope::User
}

#[derive(Deserialize)]
pub(crate) struct LegacyRoutePermission {
    #[serde(default)]
    pub id: i64,
    #[serde(default = "default_scope")]
    pub scope: Scope,
    #[serde(default)]
    pub scope_id: i64,
    #[serde(default)]
    pub route_pattern: String,
}

impl From<LegacyRoutePermission> for RoutePermissionInput {
    fn from(x: LegacyRoutePermission) -> Self {
        Self {
            id: Some(x.id),
            scope: x.scope,
            scope_id: x.scope_id,
            route_pattern: x.route_pattern,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct LegacyRateLimit {
    #[serde(default)]
    pub id: i64,
    #[serde(default = "default_scope")]
    pub scope: Scope,
    #[serde(default)]
    pub scope_id: i64,
    #[serde(default)]
    pub route_pattern: String,
    #[serde(default)]
    pub rpm: Option<i64>,
    #[serde(default)]
    pub rpd: Option<i64>,
    #[serde(default)]
    pub total_tokens: Option<i64>,
}

impl From<LegacyRateLimit> for RateLimitInput {
    fn from(x: LegacyRateLimit) -> Self {
        Self {
            id: Some(x.id),
            scope: x.scope,
            scope_id: x.scope_id,
            route_pattern: x.route_pattern,
            rpm: x.rpm,
            rpd: x.rpd,
            total_tokens: x.total_tokens,
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct LegacyQuota {
    #[serde(default)]
    pub id: i64,
    #[serde(default = "default_scope")]
    pub scope: Scope,
    #[serde(default)]
    pub scope_id: i64,
    #[serde(default, deserialize_with = "deserialize_decimal")]
    pub quota_total: Decimal,
    #[serde(default, deserialize_with = "deserialize_decimal")]
    pub cost_used: Decimal,
}

impl From<LegacyQuota> for QuotaInput {
    fn from(x: LegacyQuota) -> Self {
        Self {
            id: Some(x.id),
            scope: x.scope,
            scope_id: x.scope_id,
            quota_total: x.quota_total,
            quota_daily: None,
            quota_weekly: None,
            quota_monthly: None,
            quota_5h: None,
            quota_7d: None,
            cost_used: x.cost_used,
        }
    }
}
