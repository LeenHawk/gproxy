use std::collections::BTreeMap;

use futures_util::future::join_all;
use gproxy_protocol::{OperationKind, WireFamily};
use web_time::Instant;

use crate::api::Core;
use crate::boundary::{RequestCtx, ResponseBody, RoutingMode};
use crate::control::{ControlPlane, ExposedModel, FailoverBudget, Plan, Target};
use crate::host::Host;

pub(super) async fn run<H: Host>(
    core: &Core<H>,
    control: &dyn ControlPlane,
    request: &RequestCtx,
    plan: &Plan,
    identity: &gproxy_channel_api::CallerIdentity,
) -> Vec<ExposedModel> {
    let mut providers = BTreeMap::<i64, (String, Vec<Target>)>::new();
    for target in &plan.targets {
        providers
            .entry(target.provider.id)
            .or_insert_with(|| (target.provider.name.clone(), Vec::new()))
            .1
            .push(target.clone());
    }
    let namespace_models = !matches!(&request.mode, RoutingMode::Scoped { .. });
    let requests = providers
        .into_iter()
        .map(|(provider_id, (provider, targets))| {
            let mut request = request.clone();
            request.request_id = format!("{}:catalog:{provider_id}", request.request_id);
            request.mode = RoutingMode::Scoped {
                provider: provider.clone(),
            };
            async move {
                if !control.catalogue_visible(identity, None, &request.mode) {
                    return None;
                }
                let classified = super::request::classify(&request).ok()?;
                let OperationKind::Family(family) = classified.key.kind() else {
                    return None;
                };
                let channel = core.channels.get(&targets.first()?.provider.channel)?;
                if targets.iter().any(|target| {
                    super::local_models::route_is_local(channel, target, classified.key)
                }) {
                    let models =
                        super::local_models::run(core, channel, &targets, namespace_models).await;
                    return Some((provider_id, provider.clone(), models));
                }
                // A scoped listing names one provider on purpose, as the console's
                // "import from upstream" does; the auto-refresh switch only governs
                // the aggregated fan-out.
                let targets = targets
                    .into_iter()
                    .filter(|target| !namespace_models || auto_refresh(target))
                    .collect::<Vec<_>>();
                if targets.is_empty() {
                    return None;
                }
                let budget = FailoverBudget {
                    max_attempts: plan
                        .budget
                        .max_attempts
                        .min(targets.len().try_into().unwrap_or(u32::MAX)),
                };
                let outcome = super::failover::run(
                    core,
                    control,
                    request,
                    Plan { targets, budget },
                    super::AdmittedRequest {
                        classified,
                        owner_user_id: identity.user_id,
                        session_affinity: None,
                        started: Instant::now(),
                    },
                )
                .await
                .ok()?;
                let ResponseBody::Full(body) = outcome.body else {
                    return None;
                };
                outcome.status.is_success().then(|| {
                    (
                        provider_id,
                        provider.clone(),
                        parse(family, &provider, &body, namespace_models),
                    )
                })
            }
        });
    // Read-only: what a provider reports is shown, never written. Which models a
    // provider serves is the operator's decision, taken in the pull dialog.
    let mut pulled = Vec::new();
    for (provider_id, provider, models) in join_all(requests).await.into_iter().flatten() {
        let _ = (provider_id, provider);
        pulled.extend(models);
    }
    pulled
}

pub(super) async fn for_local_get<H: Host>(
    core: &Core<H>,
    control: &dyn ControlPlane,
    request: &RequestCtx,
    plan: &Plan,
    classified: &super::request::Classified,
    identity: &gproxy_channel_api::CallerIdentity,
) -> Vec<ExposedModel> {
    let models = run(core, control, request, plan, identity).await;
    if !models.is_empty() {
        return models;
    }
    let Ok(key) = gproxy_protocol::OperationKey::try_new(
        gproxy_protocol::Operation::ListModels,
        classified.key.kind(),
    ) else {
        return Vec::new();
    };
    let Some((method, path)) = gproxy_protocol::request_target(key, "") else {
        return Vec::new();
    };
    let mut list = request.clone();
    list.method = method;
    list.path = path;
    list.query = None;
    list.body = bytes::Bytes::new();
    run(core, control, &list, plan, identity).await
}

fn parse(family: WireFamily, provider: &str, body: &[u8], namespace: bool) -> Vec<ExposedModel> {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) else {
        return Vec::new();
    };
    let models = match family {
        WireFamily::OpenAi | WireFamily::Claude => value.get("data"),
        WireFamily::Gemini => value.get("models"),
    }
    .and_then(serde_json::Value::as_array)
    .into_iter()
    .flatten();
    models
        .filter_map(|model| entry(provider, model, namespace))
        .collect()
}

fn entry(provider: &str, value: &serde_json::Value, namespace: bool) -> Option<ExposedModel> {
    // OpenAI-compatible catalogues are not uniform: some gateways label the
    // entry `model` and omit `id` entirely.
    let raw_id = ["id", "model", "name"]
        .iter()
        .find_map(|name| value.get(*name)?.as_str())?;
    let id = raw_id.strip_prefix("models/").unwrap_or(raw_id);
    Some(ExposedModel {
        id: if namespace {
            format!("{provider}/{id}")
        } else {
            id.into()
        },
        display_name: text(value, &["display_name", "displayName"]),
        context_window: integer(
            value,
            &[
                "context_window",
                "context_length",
                "max_context_window",
                "inputTokenLimit",
            ],
        ),
        max_output_tokens: integer(
            value,
            &[
                "max_output_tokens",
                "max_completion_tokens",
                "outputTokenLimit",
            ],
        ),
        thinking_supported: boolean(value, "thinking_supported"),
        thinking_adaptive_supported: boolean(value, "thinking_adaptive_supported"),
        thinking_enabled_supported: boolean(value, "thinking_enabled_supported"),
        metadata: metadata(value),
    })
}

fn metadata(value: &serde_json::Value) -> crate::ModelMetadata {
    let capabilities = value.get("capabilities");
    crate::ModelMetadata {
        description: text(value, &["description"]),
        instructions: text(value, &["instructions", "base_instructions"]),
        max_context_window: integer(value, &["max_context_window"]),
        input_modalities: strings(value, &["input_modalities"]),
        output_modalities: strings(value, &["output_modalities"]),
        supported_parameters: strings(value, &["supported_parameters"]),
        reasoning_levels: reasoning_levels(value),
        default_reasoning_level: text(value, &["default_reasoning_level"]),
        service_tiers: service_tiers(value),
        default_service_tier: text(value, &["default_service_tier"]),
        generation_methods: strings(value, &["supportedGenerationMethods"]),
        supported_actions: strings(value, &["supportedActions"]),
        shell_type: text(value, &["shell_type"]),
        support_verbosity: boolean(value, "support_verbosity"),
        default_verbosity: text(value, &["default_verbosity"]),
        supports_reasoning_summary_parameter: boolean(
            value,
            "supports_reasoning_summary_parameter",
        ),
        default_reasoning_summary: text(value, &["default_reasoning_summary"]),
        apply_patch_tool_type: text(value, &["apply_patch_tool_type"]),
        web_search_tool_type: text(value, &["web_search_tool_type"]),
        truncation_mode: value
            .get("truncation_policy")
            .and_then(|policy| text(policy, &["mode"])),
        truncation_limit: value
            .get("truncation_policy")
            .and_then(|policy| integer(policy, &["limit"])),
        auto_compact_token_limit: integer(value, &["auto_compact_token_limit"]),
        effective_context_window_percent: integer(value, &["effective_context_window_percent"]),
        batch_supported: capability(capabilities, "batch"),
        citations_supported: capability(capabilities, "citations"),
        code_execution_supported: capability(capabilities, "code_execution"),
        context_management_supported: capability(capabilities, "context_management"),
        structured_outputs_supported: capability(capabilities, "structured_outputs"),
        pdf_input_supported: capability(capabilities, "pdf_input"),
        supports_image_detail_original: boolean(value, "supports_image_detail_original"),
        supports_search_tool: boolean(value, "supports_search_tool"),
    }
}

fn strings(value: &serde_json::Value, names: &[&str]) -> Option<Vec<String>> {
    let values = names.iter().find_map(|name| value.get(*name)?.as_array())?;
    Some(
        values
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(str::to_owned)
            .collect(),
    )
}

fn reasoning_levels(value: &serde_json::Value) -> Option<Vec<crate::ModelReasoningLevel>> {
    Some(
        value
            .get("supported_reasoning_levels")?
            .as_array()?
            .iter()
            .filter_map(|level| {
                if let Some(effort) = level.as_str() {
                    return Some(crate::ModelReasoningLevel {
                        effort: effort.into(),
                        description: String::new(),
                    });
                }
                Some(crate::ModelReasoningLevel {
                    effort: level.get("effort")?.as_str()?.into(),
                    description: level
                        .get("description")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .into(),
                })
            })
            .collect(),
    )
}

fn service_tiers(value: &serde_json::Value) -> Option<Vec<crate::ModelServiceTier>> {
    Some(
        value
            .get("service_tiers")?
            .as_array()?
            .iter()
            .filter_map(|tier| {
                Some(crate::ModelServiceTier {
                    id: tier.get("id")?.as_str()?.into(),
                    name: tier.get("name")?.as_str()?.into(),
                    description: tier
                        .get("description")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .into(),
                })
            })
            .collect(),
    )
}

fn capability(value: Option<&serde_json::Value>, name: &str) -> Option<bool> {
    value?.get(name)?.get("supported")?.as_bool()
}

fn text(value: &serde_json::Value, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| value.get(*name)?.as_str().map(str::to_owned))
}

fn integer(value: &serde_json::Value, names: &[&str]) -> Option<i64> {
    names.iter().find_map(|name| value.get(*name)?.as_i64())
}

fn boolean(value: &serde_json::Value, name: &str) -> Option<bool> {
    value.get(name).and_then(serde_json::Value::as_bool)
}

/// Whether an aggregated listing may ask this provider what it serves.
///
/// On by default, as in v2: a catalogue that never refreshes goes stale silently.
/// Off is for a provider whose list is maintained by hand, or one where a fan-out
/// on every `/v1/models` costs more than the freshness is worth. A scoped
/// listing ignores it: naming the provider is the request to ask it.
fn auto_refresh(target: &Target) -> bool {
    target
        .provider
        .settings
        .get("auto_refresh_models")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatible_catalogues_may_label_the_entry_model() {
        let body = br#"{"object":"list","data":[{"model":"vendor/one"},{"id":"two"},{"name":"three"},{"object":"model"}]}"#;
        let ids = parse(WireFamily::OpenAi, "gw", body, false)
            .into_iter()
            .map(|model| model.id)
            .collect::<Vec<_>>();
        assert_eq!(ids, ["vendor/one", "two", "three"]);
    }
}
