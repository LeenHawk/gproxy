use std::collections::BTreeMap;

use futures_util::future::join_all;
use gproxy_protocol::{OperationKind, WireFamily};
use web_time::Instant;

use crate::api::Core;
use crate::boundary::{RequestCtx, ResponseBody, RoutingMode};
use crate::control::{ControlPlane, DiscoveredModel, ExposedModel, FailoverBudget, Plan, Target};
use crate::host::Host;

pub(super) async fn run<H: Host>(
    core: &Core<H>,
    control: &impl ControlPlane,
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
                    classified,
                    owner_user_id,
                    Instant::now(),
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
                        parse(family, &provider, &body),
                    )
                })
            }
        });
    let discovered = join_all(requests).await.into_iter().flatten();
    let mut persisted = Vec::new();
    for (provider_id, provider, models) in discovered {
        let rows = models
            .iter()
            .map(|model| DiscoveredModel {
                model_id: model.upstream_id.clone(),
                display_name: model.entry.display_name.clone(),
                context_window: model.entry.context_window,
                max_output_tokens: model.entry.max_output_tokens,
            })
            .collect::<Vec<_>>();
        core.host.record_discovered_models(provider_id, &rows).await;
        let _ = provider;
        persisted.extend(models.into_iter().map(|model| model.entry));
    }
    // The operator's rows win: anything disabled there never reaches a client, and
    // anything already recorded keeps the limits they set rather than the wire's.
    let catalogue = control.provider_catalogue();
    let known = catalogue
        .iter()
        .map(|model| model.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    catalogue
        .iter()
        .cloned()
        .chain(
            persisted
                .into_iter()
                .filter(|model| !known.contains(model.id.as_str())),
        )
        .collect()
}

struct Discovered {
    upstream_id: String,
    entry: ExposedModel,
}

fn parse(family: WireFamily, provider: &str, body: &[u8]) -> Vec<Discovered> {
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
    models.filter_map(|model| entry(provider, model)).collect()
}

fn entry(provider: &str, value: &serde_json::Value) -> Option<Discovered> {
    let raw_id = value.get("id").or_else(|| value.get("name"))?.as_str()?;
    let id = raw_id.strip_prefix("models/").unwrap_or(raw_id);
    let entry = ExposedModel {
        id: format!("{provider}/{id}"),
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
    };
    Some(Discovered {
        upstream_id: id.to_owned(),
        entry,
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
