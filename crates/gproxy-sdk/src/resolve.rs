//! Model name to execution plan.
//!
//! Core does not route: it executes against providers a caller already named.
//! This module owns the other half — exposed model names, the routes they
//! select, the `channel/model` and `provider/model` prefixes, and the ordering
//! among a route's members. The rows live here rather than in `CoreData`
//! because nothing in the engine reads them.
//!
//! Resolution itself arrives with the resolve phase; what is here is the table
//! every reload rebuilds.

use gproxy_core::ConfigRevision;
use gproxy_store::{RoutingData, entity::routing};

/// Routing rows as of one durable revision. Rebuilt whole on every reload and
/// published through `ArcSwap`, so a request pins one coherent table for its
/// lifetime exactly as it pins one `CoreData`.
#[derive(Debug, Default)]
pub struct RoutingTable {
    /// The revision the rows were read at, for the same monotonic reasoning
    /// core applies to its snapshot.
    pub revision: ConfigRevision,
    pub routes: Vec<routing::route::Model>,
    pub route_members: Vec<routing::route_member::Model>,
    pub exposed_models: Vec<routing::exposed_model::Model>,
}

impl RoutingTable {
    pub fn new(revision: ConfigRevision, data: RoutingData) -> Self {
        Self {
            revision,
            routes: data.routes,
            route_members: data.route_members,
            exposed_models: data.exposed_models,
        }
    }
}
