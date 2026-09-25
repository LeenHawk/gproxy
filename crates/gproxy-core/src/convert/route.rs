//! Deterministic mappings from a source OperationKey to one implementation.

use crate::{ProviderData, assemble::provider_view};
use gproxy_channel::channel::{BaseChannel, ProviderView};
use gproxy_protocol::{Dialect, Operation, OperationKey, spec::OPERATION_SPECS};
use gproxy_store::entity::upstream::operation_rule;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "implementation", rename_all = "snake_case", deny_unknown_fields)]
pub enum Route {
    Passthrough,
    TransformTo { target: OperationKey },
    Local,
    Unsupported,
}

/// One operation's explicitly configured source-protocol mappings.
pub type RoutingMappings = BTreeMap<Dialect, Route>;

pub fn local_supported(key: OperationKey) -> bool {
    super::spec_for(key).is_some()
        && matches!(
            key.operation,
            Operation::CountTokens | Operation::ListModels | Operation::GetModel
        )
}

/// Only edges implemented by the family drivers are offered to configuration.
pub fn can_convert(source: OperationKey, target: OperationKey) -> bool {
    use Dialect::*;
    use Operation::*;
    if source == target || super::spec_for(source).is_none() {
        return false;
    }
    let generation = |d| matches!(d, OpenAi | OpenAiChat | Claude | Gemini);
    match source.operation {
        GenerateContent => {
            matches!(target.operation, GenerateContent | StreamGenerateContent)
                && (generation(source.dialect) || source.dialect == OpenAiResponsesWebSocket)
                && generation(target.dialect)
        }
        StreamGenerateContent => {
            matches!(target.operation, StreamGenerateContent | GenerateContent)
                && (generation(source.dialect) || source.dialect == OpenAiResponsesWebSocket)
                && (generation(target.dialect) || target.dialect == OpenAiResponsesWebSocket)
                && (target.operation != GenerateContent
                    || target.dialect != OpenAiResponsesWebSocket)
        }
        WebSearch => {
            source.dialect == OpenAi
                && target.operation == GenerateContent
                && matches!(target.dialect, OpenAi | Claude | Gemini)
        }
        GuardianReview | GuardianClassify | CompactContent | SummarizeMemory => {
            source.dialect == OpenAi
                && target.operation == GenerateContent
                && generation(target.dialect)
        }
        CountTokens => {
            target.operation == CountTokens
                && source.dialect != target.dialect
                && matches!(source.dialect, OpenAi | Claude | Gemini)
                && matches!(target.dialect, Claude | Gemini)
        }
        CreateEmbedding => {
            target.operation == CreateEmbedding
                && matches!(
                    (source.dialect, target.dialect),
                    (OpenAi, Gemini) | (Gemini, OpenAi)
                )
        }
        BatchCreateEmbedding => {
            source.dialect == Gemini
                && target.operation == CreateEmbedding
                && target.dialect == OpenAi
        }
        CreateImage | EditImage => {
            source.dialect == OpenAi
                && target.operation == GenerateContent
                && target.dialect == Gemini
        }
        ListModels | GetModel | CreateFile | ListFiles | RetrieveFile | RetrieveFileContent
        | DeleteFile => {
            source.operation == target.operation
                && source.dialect != target.dialect
                && matches!(source.dialect, OpenAi | Claude | Gemini)
                && matches!(target.dialect, OpenAi | Claude | Gemini)
        }
        CreateVideo | RetrieveVideo | DownloadVideoContent => {
            source.operation == target.operation
                && matches!(
                    (source.dialect, target.dialect),
                    (OpenAi, Gemini) | (Gemini, OpenAi)
                )
        }
        _ => false,
    }
}

pub fn conversion_targets(source: OperationKey) -> Vec<OperationKey> {
    let mut targets: Vec<_> = OPERATION_SPECS.iter().map(|s| s.key).collect();
    // Gemini's native video API is an upstream-only wire, not a public ingress.
    for operation in [
        Operation::CreateVideo,
        Operation::RetrieveVideo,
        Operation::DownloadVideoContent,
    ] {
        targets.push(OperationKey {
            operation,
            dialect: Dialect::Gemini,
        });
    }
    targets.retain(|&target| can_convert(source, target));
    targets.sort();
    targets.dedup();
    targets
}

pub fn default_route(
    channel: &dyn BaseChannel,
    provider: ProviderView<'_>,
    source: OperationKey,
) -> Route {
    default_with_native(
        channel,
        provider,
        source,
        &channel.native_dialects(provider, source.operation),
    )
}

fn default_with_native(
    channel: &dyn BaseChannel,
    provider: ProviderView<'_>,
    source: OperationKey,
    native: &[Dialect],
) -> Route {
    if native.contains(&source.dialect) {
        return if channel.local_operations().contains(&source.operation) {
            Route::Local
        } else {
            Route::Passthrough
        };
    }
    if let Some(target) = channel.default_conversion_target(provider, source)
        && can_convert(source, target)
        && (if target.operation == source.operation {
            native.contains(&target.dialect)
        } else {
            channel
                .native_dialects(provider, target.operation)
                .contains(&target.dialect)
        })
    {
        return Route::TransformTo { target };
    }
    let mut targets: Vec<_> = conversion_targets(source)
        .into_iter()
        .filter(|target| {
            let supported = if target.operation == source.operation {
                native.to_vec()
            } else {
                channel.native_dialects(provider, target.operation)
            };
            supported.contains(&target.dialect)
        })
        .collect();
    // Prefer the same response mode when the channel supports it.
    if matches!(
        source.operation,
        Operation::GenerateContent | Operation::StreamGenerateContent
    ) && targets.iter().any(|t| t.operation == source.operation)
    {
        targets.retain(|t| t.operation == source.operation);
    }
    targets.sort_by_key(|t| (t.operation, t.dialect));
    targets.dedup();
    match targets.as_slice() {
        [target] => Route::TransformTo { target: *target },
        [] if local_supported(source) => Route::Local,
        _ => Route::Unsupported,
    }
}

pub fn validate_mapping(source: OperationKey, mapping: Route) -> Result<(), String> {
    if super::spec_for(source).is_none() {
        return Err("unknown source operation/protocol pair".into());
    }
    match mapping {
        Route::TransformTo { target } if !conversion_targets(source).contains(&target) => {
            Err("this conversion is not implemented".into())
        }
        Route::Local if !local_supported(source) => {
            Err("this operation has no local handler".into())
        }
        _ => Ok(()),
    }
}

pub fn resolve_route(
    channel: &dyn BaseChannel,
    provider: ProviderView<'_>,
    rule: Option<&operation_rule::Model>,
    source: OperationKey,
) -> Result<Route, RouteError> {
    let Some(rule) = rule else {
        return Ok(default_route(channel, provider, source));
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
                None => Ok(default_route(channel, provider, source)),
            }
        }
        // Read old data as a support set, never as a preference order.
        "dialects" => {
            let native: Vec<Dialect> =
                serde_json::from_value(rule.target.clone().unwrap_or(serde_json::json!([])))
                    .map_err(|e| invalid(e.to_string()))?;
            if native.is_empty() {
                Ok(default_route(channel, provider, source))
            } else {
                Ok(default_with_native(channel, provider, source, &native))
            }
        }
        other => Err(invalid(format!("unknown routing action `{other}`"))),
    }
}

pub fn route(provider: &ProviderData, key: OperationKey) -> Result<Route, RouteError> {
    let rule = provider
        .operation_rules
        .iter()
        .find(|r| r.operation == key.operation.id());
    resolve_route(
        provider.channel.as_ref(),
        provider_view(&provider.entity),
        rule.map(|r| r.as_ref()),
        key,
    )
}
