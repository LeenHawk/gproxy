use gproxy_admin::AdminError;
use gproxy_admin::dto::{
    DiscoveredModelDto, ModelDiscoverRequest, ModelDiscoverResponse, ModelMetadataDto,
};
use http::{HeaderMap, HeaderValue, Method};
use serde_json::Value;
use web_time::Instant;

use crate::AppHandle;

/// Ask one provider what it serves, through the ordinary list-models path.
///
/// It reaches an upstream, so it goes through the funnel like anything else and is
/// authenticated with the operator's own key. Nothing is written: the answer comes
/// back for the operator to choose from, and only what they pick is added.
pub(super) async fn run(
    app: &AppHandle,
    actor_user_id: i64,
    request: &ModelDiscoverRequest,
) -> Result<ModelDiscoverResponse, AdminError> {
    let snapshot = app.inner.host.services.control.current();
    let provider = snapshot
        .providers
        .iter()
        .find(|provider| provider.id == request.provider_id)
        .ok_or_else(|| AdminError::BadRequest("unknown provider".into()))?;
    let known = snapshot
        .provider_models
        .iter()
        .filter(|model| model.provider_id == request.provider_id)
        .map(|model| model.model_id.as_str())
        .collect::<std::collections::BTreeSet<_>>();

    let (key_prefix, secret) = super::operator_key(app, actor_user_id, &snapshot).await?;
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {secret}"))
            .map_err(|_| AdminError::Internal("malformed key".into()))?,
    );
    let ctx = gproxy_core::RequestCtx {
        request_id: format!("model-discover:{}", request.provider_id),
        client_ip: None,
        method: Method::GET,
        path: "/v1/models".into(),
        query: None,
        headers,
        body: bytes::Bytes::new(),
        upgrade: false,
        mode: gproxy_core::RoutingMode::Scoped {
            provider: provider.name.clone(),
        },
    };

    let started = Instant::now();
    let outcome = app.execute(ctx).await;
    let latency_ms = started.elapsed().as_millis().try_into().unwrap_or(u64::MAX);

    match outcome {
        Ok(outcome) => {
            let ok = outcome.status.is_success();
            let body = match outcome.body {
                gproxy_core::ResponseBody::Full(bytes) => Some(bytes),
                _ => None,
            };
            Ok(ModelDiscoverResponse {
                ok,
                status: outcome.status.as_u16(),
                latency_ms,
                key_prefix,
                models: if ok {
                    body.as_deref()
                        .map(|body| parse(body, &provider.name, &known))
                        .unwrap_or_default()
                } else {
                    Vec::new()
                },
                message: (!ok)
                    .then(|| body.as_deref().and_then(upstream_error))
                    .flatten(),
            })
        }
        Err(error) => Ok(ModelDiscoverResponse {
            ok: false,
            status: 0,
            latency_ms,
            key_prefix,
            models: Vec::new(),
            message: Some(error.to_string()),
        }),
    }
}

/// The catalogue in the three vendor list shapes. Ids come back namespaced by
/// provider for rows the operator already has, so the prefix is stripped to leave
/// the upstream id a `provider_models` row is keyed by.
fn parse(
    body: &[u8],
    provider: &str,
    known: &std::collections::BTreeSet<&str>,
) -> Vec<DiscoveredModelDto> {
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return Vec::new();
    };
    let entries = value
        .get("data")
        .or_else(|| value.get("models"))
        .and_then(Value::as_array);
    let prefix = format!("{provider}/");
    let mut models = entries
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            let id = entry
                .get("id")
                .or_else(|| entry.get("name"))
                .and_then(Value::as_str)?;
            let model_id = id.strip_prefix(&prefix).unwrap_or(id).to_owned();
            let defaults = gproxy_admin::default_model(&model_id);
            let metadata = discovered_metadata(entry, defaults);
            let supported_parameters = metadata.supported_parameters.as_deref().unwrap_or_default();
            Some(DiscoveredModelDto {
                known: known.contains(model_id.as_str()),
                display_name: entry
                    .get("display_name")
                    .or_else(|| entry.get("displayName"))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .or_else(|| defaults.and_then(|model| model.display_name.clone())),
                context_window: number(entry, &["context_window", "inputTokenLimit"])
                    .or_else(|| defaults.and_then(|model| model.context_window)),
                max_output_tokens: number(entry, &["max_output_tokens", "outputTokenLimit"])
                    .or_else(|| defaults.and_then(|model| model.max_output_tokens)),
                thinking_supported: boolean(entry, "thinking_supported").or_else(|| {
                    supported_parameters
                        .iter()
                        .any(|parameter| {
                            matches!(
                                parameter.as_str(),
                                "reasoning" | "reasoning_effort" | "include_reasoning"
                            )
                        })
                        .then_some(true)
                }),
                thinking_adaptive_supported: boolean(entry, "thinking_adaptive_supported"),
                thinking_enabled_supported: boolean(entry, "thinking_enabled_supported"),
                metadata,
                default_price_available: gproxy_admin::default_model_price_available(&model_id),
                model_id,
            })
        })
        .collect::<Vec<_>>();
    models.sort_by(|left, right| left.model_id.cmp(&right.model_id));
    models.dedup_by(|left, right| left.model_id == right.model_id);
    models
}

fn discovered_metadata(
    entry: &Value,
    defaults: Option<&gproxy_admin::dto::DefaultModelDto>,
) -> ModelMetadataDto {
    let mut metadata = defaults
        .map(|model| model.metadata.clone())
        .unwrap_or_default();
    macro_rules! text {
        ($field:ident, $name:literal) => {
            if let Some(value) = entry.get($name).and_then(Value::as_str) {
                metadata.$field = Some(value.to_owned());
            }
        };
    }
    macro_rules! number {
        ($field:ident, $name:literal) => {
            if let Some(value) = entry.get($name).and_then(Value::as_i64) {
                metadata.$field = Some(value);
            }
        };
    }
    macro_rules! boolean {
        ($field:ident, $name:literal) => {
            if let Some(value) = entry.get($name).and_then(Value::as_bool) {
                metadata.$field = Some(value);
            }
        };
    }
    text!(description, "description");
    if let Some(value) = entry
        .get("instructions")
        .or_else(|| entry.get("base_instructions"))
        .and_then(Value::as_str)
    {
        metadata.instructions = Some(value.to_owned());
    }
    number!(max_context_window, "max_context_window");
    metadata.input_modalities = strings(entry, "input_modalities").or(metadata.input_modalities);
    metadata.output_modalities = strings(entry, "output_modalities").or(metadata.output_modalities);
    metadata.supported_parameters =
        strings(entry, "supported_parameters").or(metadata.supported_parameters);
    metadata.reasoning_levels =
        structured(entry, "supported_reasoning_levels").or(metadata.reasoning_levels);
    metadata.service_tiers = structured(entry, "service_tiers").or(metadata.service_tiers);
    metadata.generation_methods =
        strings(entry, "supportedGenerationMethods").or(metadata.generation_methods);
    metadata.supported_actions = strings(entry, "supportedActions").or(metadata.supported_actions);
    for (field, name) in [
        (
            &mut metadata.default_reasoning_level,
            "default_reasoning_level",
        ),
        (&mut metadata.default_service_tier, "default_service_tier"),
        (&mut metadata.shell_type, "shell_type"),
        (&mut metadata.default_verbosity, "default_verbosity"),
        (
            &mut metadata.default_reasoning_summary,
            "default_reasoning_summary",
        ),
        (&mut metadata.apply_patch_tool_type, "apply_patch_tool_type"),
        (&mut metadata.web_search_tool_type, "web_search_tool_type"),
    ] {
        if let Some(value) = entry.get(name).and_then(Value::as_str) {
            *field = Some(value.to_owned());
        }
    }
    boolean!(support_verbosity, "support_verbosity");
    boolean!(
        supports_reasoning_summary_parameter,
        "supports_reasoning_summary_parameter"
    );
    boolean!(
        supports_image_detail_original,
        "supports_image_detail_original"
    );
    boolean!(supports_search_tool, "supports_search_tool");
    if let Some(policy) = entry.get("truncation_policy") {
        metadata.truncation_mode = policy
            .get("mode")
            .and_then(Value::as_str)
            .map(str::to_owned);
        metadata.truncation_limit = policy.get("limit").and_then(Value::as_i64);
    }
    number!(auto_compact_token_limit, "auto_compact_token_limit");
    number!(
        effective_context_window_percent,
        "effective_context_window_percent"
    );
    metadata
}

fn structured<T: serde::de::DeserializeOwned>(entry: &Value, name: &str) -> Option<Vec<T>> {
    serde_json::from_value(entry.get(name)?.clone()).ok()
}

fn number(entry: &Value, names: &[&str]) -> Option<i64> {
    names
        .iter()
        .find_map(|name| entry.get(*name).and_then(Value::as_i64))
}

fn boolean(entry: &Value, name: &str) -> Option<bool> {
    entry.get(name).and_then(Value::as_bool)
}

fn strings(entry: &Value, name: &str) -> Option<Vec<String>> {
    Some(
        entry
            .get(name)?
            .as_array()?
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect(),
    )
}

fn upstream_error(body: &[u8]) -> Option<String> {
    let text = String::from_utf8(body.to_vec()).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.chars().take(300).collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn discovery_keeps_upstream_values_and_fills_catalog_gaps() {
        let known = std::collections::BTreeSet::new();
        let models = super::parse(
            br#"{"data":[{"id":"gpt-5.6-sol","context_window":42}]}"#,
            "codex",
            &known,
        );
        let model = models.first().expect("discovered model");
        assert_eq!(model.context_window, Some(42));
        assert!(model.max_output_tokens.is_some());
        assert!(model.thinking_supported.is_some());
        assert!(
            model
                .metadata
                .input_modalities
                .as_ref()
                .is_some_and(|values| values.iter().any(|value| value == "text"))
        );
        assert!(model.default_price_available);
    }
}
