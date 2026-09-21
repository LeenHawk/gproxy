//! Passthrough or conversion, decided per provider and operation.

use crate::{ProviderData, assemble::provider_view};
use gproxy_protocol::{Dialect, Operation, OperationKey};

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum RouteError {
    #[error("provider `{provider_id}` declares no native dialect for {operation:?}")]
    Undeclared {
        provider_id: String,
        operation: Operation,
    },
    #[error("operation rule `{id}` is invalid: {reason}")]
    InvalidRule { id: String, reason: String },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// The client's dialect is native to the upstream: bytes go through.
    Passthrough,
    /// Convert to the upstream's preferred native dialect and back.
    Convert { upstream: Dialect },
    /// The client streams but the upstream only generates buffered: convert,
    /// invoke once, then synthesize the client's native stream.
    Synthesize { upstream: Dialect },
}

/// Native dialects come from the channel's declaration for this provider; an
/// `OperationRule` row with `action = "dialects"` and a JSON array of dialect
/// IDs in `target` overrides it for that operation. The client's dialect wins
/// when native; otherwise the first declared dialect is the conversion target.
pub fn route(provider: &ProviderData, key: OperationKey) -> Result<Route, RouteError> {
    match natives(provider, key.operation)? {
        natives if natives.contains(&key.dialect) => Ok(Route::Passthrough),
        natives if !natives.is_empty() => Ok(Route::Convert {
            upstream: natives[0],
        }),
        _ if key.operation == Operation::StreamGenerateContent => {
            match natives(provider, Operation::GenerateContent)? {
                buffered if buffered.is_empty() || buffered.contains(&key.dialect) => {
                    Err(RouteError::Undeclared {
                        provider_id: provider.entity.id.clone(),
                        operation: key.operation,
                    })
                }
                buffered => Ok(Route::Synthesize {
                    upstream: buffered[0],
                }),
            }
        }
        _ => Err(RouteError::Undeclared {
            provider_id: provider.entity.id.clone(),
            operation: key.operation,
        }),
    }
}

fn natives(provider: &ProviderData, operation: Operation) -> Result<Vec<Dialect>, RouteError> {
    let mut natives: Vec<Dialect> = Vec::new();
    for rule in provider
        .operation_rules
        .iter()
        .filter(|rule| rule.action == "dialects" && rule.operation == operation.id())
    {
        let ids: Vec<String> = rule
            .target
            .clone()
            .map(serde_json::from_value)
            .transpose()
            .map_err(|e| RouteError::InvalidRule {
                id: rule.id.clone(),
                reason: e.to_string(),
            })?
            .unwrap_or_default();
        natives = ids
            .iter()
            .map(|id| {
                Dialect::from_id(id).ok_or_else(|| RouteError::InvalidRule {
                    id: rule.id.clone(),
                    reason: format!("unknown dialect `{id}`"),
                })
            })
            .collect::<Result<_, _>>()?;
    }
    if natives.is_empty() {
        natives = provider
            .channel
            .native_dialects(provider_view(&provider.entity), operation);
    }
    Ok(natives)
}
