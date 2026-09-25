//! Routing rows of one durable revision, indexed the way resolution reads them.

use std::collections::HashMap;

use gproxy_core::ConfigRevision;
use gproxy_store::{RoutingData, entity::routing::route::RouteStrategy};

/// The attempt budget used when no route names one. Matches the Setting column
/// default, so a database without a settings row behaves like a fresh one.
pub const DEFAULT_MAX_ATTEMPTS: u32 = 6;

/// One member of a route, flattened out of the two durable rows it comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MemberSeed {
    pub member_id: String,
    pub provider_id: String,
    /// Route members always name an upstream model; the column is not nullable.
    pub upstream_model: String,
    /// Lower is preferred. A later tier is only reached when every target of
    /// the earlier one is unusable.
    pub tier: u32,
    /// Relative share within a tier, and the descending tiebreak for failover.
    pub weight: u32,
}

/// A named pool with its balancing strategy and attempt budget.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RouteEntry {
    pub strategy: RouteStrategy,
    pub session_affinity: bool,
    pub max_attempts: u32,
    /// Enabled members only, ordered by `(tier, Reverse(weight), member_id)`
    /// so a plan built from this table does not depend on row order.
    pub members: Vec<MemberSeed>,
}

/// Routing rows as of one durable revision. Rebuilt whole on every reload and
/// published through `ArcSwap`, so a request pins one coherent table for its
/// lifetime exactly as it pins one `CoreData`.
///
/// Only enabled routes and members are kept. A disabled route name is not
/// callable, and a disabled member is not a candidate. Resolution therefore
/// never repeats an `enabled` check.
#[derive(Debug)]
pub struct RoutingTable {
    /// The revision the rows were read at, for the same monotonic reasoning
    /// core applies to its snapshot.
    pub revision: ConfigRevision,
    /// The instance-wide attempt budget from the settings row, used by every
    /// resolution that does not go through a route.
    pub default_max_attempts: u32,
    /// Enabled route model name to route id. Names are matched exactly.
    pub names: HashMap<String, String>,
    /// Enabled routes by id, with their enabled members.
    pub routes: HashMap<String, RouteEntry>,
}

impl Default for RoutingTable {
    /// What a handle serves before its first reload: no names, and the same
    /// attempt budget a fresh settings row would have given.
    fn default() -> Self {
        Self {
            revision: ConfigRevision::default(),
            default_max_attempts: DEFAULT_MAX_ATTEMPTS,
            names: HashMap::new(),
            routes: HashMap::new(),
        }
    }
}

impl RoutingTable {
    pub fn new(revision: ConfigRevision, data: RoutingData, default_max_attempts: u32) -> Self {
        let mut routes: HashMap<String, RouteEntry> = data
            .routes
            .iter()
            .filter(|route| route.enabled)
            .map(|route| {
                (
                    route.id.clone(),
                    RouteEntry {
                        strategy: route.strategy,
                        session_affinity: route.session_affinity,
                        // `Setting.max_attempts` is the instance-wide ceiling
                        // (see the `routes` entity): a route may ask for fewer
                        // attempts than the instance allows, never for more.
                        max_attempts: route.max_attempts.clamp(1, default_max_attempts.max(1)),
                        members: Vec::new(),
                    },
                )
            })
            .collect();
        for member in data.route_members.iter().filter(|member| member.enabled) {
            let Some(route) = routes.get_mut(&member.route_id) else {
                continue;
            };
            route.members.push(MemberSeed {
                member_id: member.id.clone(),
                provider_id: member.provider_id.clone(),
                upstream_model: member.upstream_model.clone(),
                tier: member.tier,
                weight: member.weight,
            });
        }
        for route in routes.values_mut() {
            route.members.sort_by(|a, b| {
                (a.tier, std::cmp::Reverse(a.weight), &a.member_id).cmp(&(
                    b.tier,
                    std::cmp::Reverse(b.weight),
                    &b.member_id,
                ))
            });
        }
        let names = data
            .routes
            .iter()
            .filter(|route| route.enabled)
            .map(|route| (route.name.clone(), route.id.clone()))
            .collect();
        Self {
            revision,
            default_max_attempts: default_max_attempts.max(1),
            names,
            routes,
        }
    }

    /// The enabled route whose name exactly matches the requested model.
    pub fn route_for(&self, model: &str) -> Option<(&str, &RouteEntry)> {
        let id = self.names.get(model)?;
        Some((id.as_str(), self.routes.get(id)?))
    }
}
