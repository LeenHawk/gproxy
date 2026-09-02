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
    owner_user_id: i64,
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
                let classified = super::request::classify(&request).ok()?;
                let OperationKind::Family(family) = classified.key.kind else {
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
                let targets = targets.into_iter().filter(auto_refresh).collect::<Vec<_>>();
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
                        owner_user_id,
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
    owner_user_id: i64,
) -> Vec<ExposedModel> {
    let models = run(core, control, request, plan, owner_user_id).await;
    if !models.is_empty() {
        return models;
    }
    let key = gproxy_protocol::OperationKey {
        operation: gproxy_protocol::Operation::ListModels,
        kind: classified.key.kind,
    };
    let Some((method, path)) = gproxy_protocol::request_target(key, "") else {
        return Vec::new();
    };
    let mut list = request.clone();
    list.method = method;
    list.path = path;
    list.query = None;
    list.body = bytes::Bytes::new();
    run(core, control, &list, plan, owner_user_id).await
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
    let raw_id = value.get("id").or_else(|| value.get("name"))?.as_str()?;
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
    })
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

/// Whether listing models may ask this provider what it serves.
///
/// On by default, as in v2: a catalogue that never refreshes goes stale silently.
/// Off is for a provider whose list is maintained by hand, or one where a fan-out
/// on every `/v1/models` costs more than the freshness is worth.
fn auto_refresh(target: &Target) -> bool {
    target
        .provider
        .settings
        .get("auto_refresh_models")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true)
}
