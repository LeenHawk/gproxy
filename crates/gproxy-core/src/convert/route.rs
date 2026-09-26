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
    default_route_for_model(channel, provider, source, None)
}

/// `default_route` for a request naming `model`, for channels whose upstream
/// picks its wire by model (`BaseChannel::native_dialects_for_model`).
pub fn default_route_for_model(
    channel: &dyn BaseChannel,
    provider: ProviderView<'_>,
    source: OperationKey,
    model: Option<&str>,
) -> Route {
    default_with_native(
        channel,
        provider,
        source,
        &channel.native_dialects_for_model(provider, source.operation, model),
        model,
    )
}

fn default_with_native(
    channel: &dyn BaseChannel,
    provider: ProviderView<'_>,
    source: OperationKey,
    native: &[Dialect],
    model: Option<&str>,
) -> Route {
    if native.contains(&source.dialect) {
        return if channel.local_operations().contains(&source.operation) {
            Route::Local
        } else {
            Route::Passthrough
        };
    }
    if let Some(target) = channel.default_conversion_target_for_model(provider, source, model)
        && can_convert(source, target)
        && (if target.operation == source.operation {
            native.contains(&target.dialect)
        } else {
            channel
                .native_dialects_for_model(provider, target.operation, model)
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
                channel.native_dialects_for_model(provider, target.operation, model)
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
    resolve_route_for_model(
        provider.channel.as_ref(),
        provider_view(&provider.entity),
        rule.map(|r| r.as_ref()),
        key,
        model,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An upstream that, like Bedrock, serves Claude models on Messages and
    /// every other model on Chat Completions.
    struct ByModel;

    impl BaseChannel for ByModel {
        fn id(&self) -> &'static str {
            "by_model"
        }
        fn native_dialects(&self, _: ProviderView<'_>, _: Operation) -> Vec<Dialect> {
            vec![Dialect::Claude]
        }
        fn native_dialects_for_model(
            &self,
            provider: ProviderView<'_>,
            operation: Operation,
            model: Option<&str>,
        ) -> Vec<Dialect> {
            match model {
                Some(model) if !model.starts_with("claude") => vec![Dialect::OpenAiChat],
                _ => self.native_dialects(provider, operation),
            }
        }
    }

    #[test]
    fn the_same_request_takes_the_wire_its_model_is_served_on() {
        let config = serde_json::json!({});
        let view = ProviderView {
            id: "p",
            channel: "by_model",
            base_url: None,
            config: &config,
        };
        let chat = OperationKey {
            operation: Operation::GenerateContent,
            dialect: Dialect::OpenAiChat,
        };
        assert_eq!(
            default_route_for_model(&ByModel, view, chat, Some("gpt-5.5")),
            Route::Passthrough
        );
        assert_eq!(
            default_route_for_model(&ByModel, view, chat, Some("claude-sonnet-5")),
            Route::TransformTo {
                target: OperationKey {
                    operation: Operation::GenerateContent,
                    dialect: Dialect::Claude,
                }
            }
        );
        // Without a model, the provider-wide answer: what `native_dialects` says.
        assert_eq!(
            default_route(&ByModel, view, chat),
            default_route_for_model(&ByModel, view, chat, Some("claude-sonnet-5"))
        );
    }
}
