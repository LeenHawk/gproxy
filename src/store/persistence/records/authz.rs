//! Authorization records (§8-C): scope-based route permissions, rate limits,
//! and quotas. Each is scoped to an org/team/user in the identity hierarchy.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// The identity level a permission/limit/quota is bound to (§8-C).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Org,
    Team,
    User,
}

impl Scope {
    /// Stable lowercase wire/storage label for this scope.
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Org => "org",
            Scope::Team => "team",
            Scope::User => "user",
        }
    }

    /// Parse a stored/wire label back into a [`Scope`].
    pub fn parse(s: &str) -> anyhow::Result<Self> {
        match s {
            "org" => Ok(Scope::Org),
            "team" => Ok(Scope::Team),
            "user" => Ok(Scope::User),
            other => anyhow::bail!("invalid scope: {other}"),
        }
    }
}

/// A route-access grant scoped to an org/team/user (§8-C).
///
/// `scope` is `Org` | `Team` | `User`; `scope_id` is the corresponding entity
/// id; `route_pattern` is the route/alias glob the scope may use.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutePermission {
    pub id: i64,
    pub scope: Scope,
    pub scope_id: i64,
    pub route_pattern: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Upsert input for a route permission.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutePermissionInput {
    pub id: Option<i64>,
    pub scope: Scope,
    pub scope_id: i64,
    pub route_pattern: String,
}

/// A rate limit scoped to an org/team/user for a route pattern (§8-C).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RateLimit {
    pub id: i64,
    pub scope: Scope,
    pub scope_id: i64,
    pub route_pattern: String,
    pub rpm: Option<i64>,
    pub rpd: Option<i64>,
    pub total_tokens: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Upsert input for a rate limit.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RateLimitInput {
    pub id: Option<i64>,
    pub scope: Scope,
    pub scope_id: i64,
    pub route_pattern: String,
    pub rpm: Option<i64>,
    pub rpd: Option<i64>,
    pub total_tokens: Option<i64>,
}

/// A spend quota scoped to an org/team/user (§8-C). Unique per `(scope, scope_id)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Quota {
    pub id: i64,
    pub scope: Scope,
    pub scope_id: i64,
    #[serde(with = "rust_decimal::serde::str")]
    pub quota_total: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub cost_used: Decimal,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Upsert input for a quota.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuotaInput {
    pub id: Option<i64>,
    pub scope: Scope,
    pub scope_id: i64,
    #[serde(with = "rust_decimal::serde::str")]
    pub quota_total: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub cost_used: Decimal,
}
