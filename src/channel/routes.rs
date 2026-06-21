//! A channel's explicit routing surface — the verbatim port of v1's per-channel
//! `routing_table()`. A channel declares, for each `(operation, inbound kind)`
//! cell it can serve, how to service it ([`RoutingDecision`]). `seed_default_routing`
//! materializes this list into real `routing_rules` rows; cells the channel does
//! not declare have no rule and are `Unsupported` at request time.

use crate::protocol::{
    ContentGenerationKind as Cg, Operation, OperationKey, OperationKind, Provider,
};
use crate::transform::routing::RoutingDecision;

/// A channel's declared routing surface: source cell → decision.
pub type RouteList = Vec<(OperationKey, RoutingDecision)>;

fn key(operation: Operation, kind: OperationKind) -> OperationKey {
    OperationKey { operation, kind }
}

/// passthrough
pub fn pass(operation: Operation, kind: OperationKind) -> (OperationKey, RoutingDecision) {
    (key(operation, kind), RoutingDecision::Passthrough)
}

/// transform to a different cell
pub fn xform(
    operation: Operation,
    kind: OperationKind,
    d_op: Operation,
    d_kind: OperationKind,
) -> (OperationKey, RoutingDecision) {
    (
        key(operation, kind),
        RoutingDecision::TransformTo(key(d_op, d_kind)),
    )
}

/// served locally
pub fn local(operation: Operation, kind: OperationKind) -> (OperationKey, RoutingDecision) {
    (key(operation, kind), RoutingDecision::Local)
}

// kind shorthands
pub fn cg(k: Cg) -> OperationKind {
    OperationKind::ContentGeneration(k)
}

pub fn pv(p: Provider) -> OperationKind {
    OperationKind::Provider(p)
}
