//! Routing records (§8-A): routes, members, aliases.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A route — one logical model name backed by 1..N members (§3.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Route {
    pub id: i64,
    pub name: String,
    /// `weighted` | `round_robin` | `failover` | `least_latency`.
    pub strategy: String,
    pub enabled: bool,
    pub description: Option<String>,
    /// Free-form route settings. `circuit_breaker` overrides member-provider
    /// thresholds (§3.2); `affinity` configures user/session/conversation pins.
    #[serde(default)]
    pub settings_json: Option<Value>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Upsert input for a route.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteInput {
    pub id: Option<i64>,
    pub name: String,
    pub strategy: String,
    pub enabled: bool,
    pub description: Option<String>,
    #[serde(default)]
    pub settings_json: Option<Value>,
}

/// A member of a route's backend pool (§3.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteMember {
    pub id: i64,
    pub route_id: i64,
    pub provider_id: i64,
    pub upstream_model_id: String,
    pub weight: i64,
    pub tier: i64,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Upsert input for a route member.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteMemberInput {
    pub id: Option<i64>,
    pub route_id: i64,
    pub provider_id: i64,
    pub upstream_model_id: String,
    pub weight: i64,
    pub tier: i64,
    pub enabled: bool,
}

fn default_alias_provider() -> String {
    "*".to_owned()
}

fn default_true() -> bool {
    true
}

/// A model alias rule. `provider == "*"` is global; any other value scopes the
/// rule to the resolved provider. `alias` is a full-match regex pattern and
/// `target` is a regex replacement string.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alias {
    pub id: i64,
    #[serde(default = "default_alias_provider")]
    pub provider: String,
    pub alias: String,
    #[serde(default)]
    pub target: String,
    #[serde(default)]
    pub sort_order: i64,
    #[serde(default = "default_true")]
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Upsert input for an alias.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AliasInput {
    pub id: Option<i64>,
    #[serde(default = "default_alias_provider")]
    pub provider: String,
    pub alias: String,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub sort_order: i64,
    #[serde(default = "default_true")]
    pub enabled: bool,
}
