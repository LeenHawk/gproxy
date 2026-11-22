//! A provider's operation rules over `gproxy-convert`'s default routing.

use crate::ProviderData;
use gproxy_channel::channel::BaseChannel;
use gproxy_convert::{
    Route, RoutingMappings, default_route_for_model, default_with_native, validate_mapping,
};
use gproxy_protocol::{Dialect, Operation, OperationKey};
use gproxy_store::entity::upstream::operation_rule;
use gproxy_upstream::ProviderView;

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum RouteError {
    #[error("provider `{provider_id}` does not support {operation:?}")]
    Undeclared {
        provider_id: String,
        operation: Operation,
    },
    #[error("operation rule `{id}` is invalid: {reason}")]
    InvalidRule { id: String, reason: String },
}

pub fn resolve_route(
    channel: &dyn BaseChannel,
    provider: ProviderView<'_>,
    rule: Option<&operation_rule::Model>,
    source: OperationKey,
) -> Result<Route, RouteError> {
    resolve_route_for_model(channel, provider, rule, source, None)
}

/// `resolve_route` for a request naming `model`; an explicit rule still wins.
pub fn resolve_route_for_model(
    channel: &dyn BaseChannel,
    provider: ProviderView<'_>,
    rule: Option<&operation_rule::Model>,
    source: OperationKey,
    model: Option<&str>,
) -> Result<Route, RouteError> {
    let Some(rule) = rule else {
        return Ok(default_route_for_model(channel, provider, source, model));
    };
    let invalid = |reason: String| RouteError::InvalidRule {
        id: rule.id.clone(),
        reason,
    };
    match rule.action.as_str() {
        "deny" => Ok(Route::Unsupported),
        "routing" => {
            let mappings: RoutingMappings =
                serde_json::from_value(rule.target.clone().unwrap_or(serde_json::json!({})))
                    .map_err(|e| invalid(e.to_string()))?;
            match mappings.get(&source.dialect).copied() {
                Some(mapping) => {
                    validate_mapping(source, mapping).map_err(invalid)?;
                    Ok(mapping)
                }
                None => Ok(default_route_for_model(channel, provider, source, model)),
            }
        }
        // Read old data as a support set, never as a preference order.
        "dialects" => {
            let native: Vec<Dialect> =
                serde_json::from_value(rule.target.clone().unwrap_or(serde_json::json!([])))
                    .map_err(|e| invalid(e.to_string()))?;
            if native.is_empty() {
                Ok(default_route_for_model(channel, provider, source, model))
            } else {
                Ok(default_with_native(
                    channel, provider, source, &native, model,
                ))
            }
        }
        other => Err(invalid(format!("unknown routing action `{other}`"))),
    }
}

pub fn route(provider: &ProviderData, key: OperationKey) -> Result<Route, RouteError> {
    route_for_model(provider, key, None)
}

/// `route` for a request naming `model`, the upstream model a target serves.
pub fn route_for_model(
    provider: &ProviderData,
    key: OperationKey,
    model: Option<&str>,
) -> Result<Route, RouteError> {
    let rule = provider
        .operation_rules
        .iter()
        .find(|r| r.operation == key.operation.id());
    let channel = provider.channel().ok_or_else(|| RouteError::Undeclared {
        provider_id: provider.entity.id.clone(),
        operation: key.operation,
    })?;
    resolve_route_for_model(
        channel.as_ref(),
        provider.view(),
        rule.map(|r| r.as_ref()),
        key,
        model,
    )
}
