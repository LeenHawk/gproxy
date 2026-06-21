//! Default routing-rule seeding for a provider, from the channel's routing table.
//!
//! [`seed_default_routing`] materializes the provider channel's declared
//! [`routing_table`](crate::channel::Channel::routing_table) as real
//! `routing_rules` rows. Cells the channel does not declare have no rule and are
//! `Unsupported` at request time, so the stored rules are the whole contract.
//! Runs at provider creation and on the explicit "reset defaults" action.

use crate::api::error::ApiError;
use crate::channel::registry::ChannelRegistry;
use crate::store::persistence::PersistenceBackend;
use crate::store::persistence::records::RoutingRuleInput;
use crate::transform::routing::RoutingDecision;

/// Serialize a `serde`-enum to its snake-case string form.
fn to_str<T: serde::Serialize>(v: &T) -> String {
    serde_json::to_value(v)
        .ok()
        .and_then(|j| j.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// Map a [`RoutingDecision`] to `(implementation, dest_operation, dest_kind)`.
fn decision_strs(decision: &RoutingDecision) -> (String, Option<String>, Option<String>) {
    match decision {
        RoutingDecision::Passthrough => ("passthrough".to_owned(), None, None),
        RoutingDecision::Local => ("local".to_owned(), None, None),
        RoutingDecision::Unsupported => ("unsupported".to_owned(), None, None),
        RoutingDecision::TransformTo(dest) => (
            "transform_to".to_owned(),
            Some(to_str(&dest.operation)),
            Some(to_str(&dest.kind)),
        ),
    }
}

/// Seed the provider channel's declared routing table as real rules.
/// `overwrite=false` fills only cells that have no rule yet (keeps existing/edited
/// rows); `overwrite=true` recomputes and overwrites every declared cell (the
/// "reset defaults" semantics). Rules for cells outside the table are untouched.
///
/// Errors: `NotFound` (provider gone), `BadRequest` (unknown channel), `Internal`.
pub async fn seed_default_routing(
    persistence: &dyn PersistenceBackend,
    channels: &ChannelRegistry,
    provider_id: i64,
    overwrite: bool,
) -> Result<(), ApiError> {
    let provider = persistence
        .get_provider(provider_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?
        .ok_or_else(|| ApiError::NotFound(format!("provider {provider_id} not found")))?;

    let channel = channels
        .get(&provider.channel)
        .ok_or_else(|| ApiError::BadRequest(format!("unknown channel: {}", provider.channel)))?;
    let table = channel.routing_table();

    let existing = persistence
        .list_routing_rules(provider_id)
        .await
        .map_err(|e| ApiError::Internal(e.to_string()))?;

    let mut sort_order = 0i64;
    for (source, decision) in table {
        if matches!(decision, RoutingDecision::Unsupported) {
            continue; // a channel never needs to materialize "unsupported"
        }
        let (implementation, dest_operation, dest_kind) = decision_strs(&decision);
        let operation = to_str(&source.operation);
        let kind = to_str(&source.kind);
        let existing_id = existing
            .iter()
            .find(|r| r.operation == operation && r.kind == kind)
            .map(|r| r.id);
        if existing_id.is_some() && !overwrite {
            continue; // keep the existing/edited rule
        }

        persistence
            .upsert_routing_rule(RoutingRuleInput {
                id: existing_id,
                provider_id,
                operation,
                kind,
                implementation,
                dest_operation,
                dest_kind,
                sort_order,
                enabled: true,
            })
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?;
        sort_order += 1;
    }

    Ok(())
}
