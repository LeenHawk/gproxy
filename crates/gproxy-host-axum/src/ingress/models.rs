//! OpenAI model lists also carry a Codex catalog, independent of client headers.
use std::sync::Arc;

use axum::response::Response;
use gproxy_app::{App, CallOutcome};
use gproxy_core::CoreData;
use gproxy_protocol::{
    HttpBody,
    codec::{CodecLimits, encode_json, read_http_body},
    connection::TransportError,
};
use gproxy_sdk::resolve::RoutingTable;
use gproxy_seaorm::BatchConnectionTrait;
use http::header;
use serde_json::{Value, json};

use crate::response::{CancelOnDrop, Trailer, leased, streamed};

/// Keep the normal response lease, cancellation and capture around the transformed body.
pub(super) fn response<C>(
    app: Arc<App<C>>,
    outcome: CallOutcome,
    cancel: CancelOnDrop,
    limits: CodecLimits,
) -> Response
where
    C: BatchConnectionTrait + Send + Sync + 'static,
{
    if !outcome.execution.response().status.is_success() {
        return streamed(app, outcome, cancel);
    }
    let CallOutcome {
        execution,
        admitted,
        mut capture,
    } = outcome;
    let (mut response, usage) = execution.into_parts();
    for name in [header::CONTENT_LENGTH, header::ETAG, header::LAST_MODIFIED] {
        response.headers.remove(name);
    }
    let body = response.body;
    response.body = HttpBody::Stream(Box::pin(futures_util::stream::once(async move {
        let bytes = read_http_body(body, limits).await?;
        let mut value: Value = serde_json::from_slice(&bytes)?;
        let data = value
            .get("data")
            .and_then(Value::as_array)
            .ok_or_else(|| std::io::Error::other("model list is missing data"))?;
        let models = data
            .iter()
            .map(|model| {
                let id = model
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| std::io::Error::other("model entry is missing id"))?;
                Ok(project(id, model))
            })
            .collect::<Result<Vec<_>, std::io::Error>>()?;
        value["models"] = json!(models);
        Ok::<_, TransportError>(encode_json(&value, limits)?)
    })));
    if let Some(capture) = capture.as_mut() {
        capture.record_response_head(response.status, &response.headers);
    }
    leased(
        Trailer::new(app, admitted, capture, usage, cancel),
        response,
    )
}

/// Required Codex fields get neutral defaults; actual model capabilities and extensions win.
pub(super) fn project(id: &str, source: &Value) -> Value {
    let mut model = json!({
        "display_name": id,
        "description": null,
        "supported_reasoning_levels": [],
        "shell_type": "default",
        "visibility": "list",
        "supported_in_api": true,
        "priority": 0,
        "support_verbosity": false,
        "supports_reasoning_summary_parameter": false,
        "truncation_policy": {"mode": "bytes", "limit": 10000},
        "experimental_supported_tools": [],
        "input_modalities": ["text"]
    });
    let target = model.as_object_mut().unwrap();
    if let Some(source) = source.as_object() {
        target.extend(
            source
                .iter()
                .filter(|(_, value)| !value.is_null())
                .map(|(key, value)| (key.clone(), value.clone())),
        );
    }
    target.insert("slug".into(), json!(id));
    for key in ["id", "object", "created", "owned_by"] {
        target.remove(key);
    }
    let instructions = target.remove("instructions");
    // Model Messages already contain the instruction template. Copying it again
    // can exceed Codex's 1 MiB limit for an explicit model_catalog_url.
    if target
        .get("model_messages")
        .and_then(|messages| messages.get("instructions_template"))
        .is_some_and(Value::is_string)
    {
        target.remove("base_instructions");
    } else {
        target.entry("base_instructions").or_insert_with(|| instructions.unwrap_or_else(|| {
            json!("You are Codex, a coding agent. Work with the user in the current workspace, follow repository instructions, and complete the requested task carefully.")
        }));
    }
    if let Some(levels) = target
        .get_mut("supported_reasoning_levels")
        .and_then(Value::as_array_mut)
    {
        for level in levels {
            if let Some(effort) = level.as_str() {
                *level = json!({"effort": effort, "description": ""});
            }
        }
    }
    // Codex only understands text and image input modalities.
    if let Some(modalities) = target
        .get_mut("input_modalities")
        .and_then(Value::as_array_mut)
    {
        modalities.retain(|value| matches!(value.as_str(), Some("text" | "image")));
    }
    model
}

/// Resolve configured metadata using the same names the public catalog lists.
/// A pooled alias only advertises facts shared by every backing model.
pub(super) fn metadata(core: &CoreData, routing: &RoutingTable, name: &str) -> Value {
    let find = |provider_id: &str, name: &str| {
        core.providers
            .get(provider_id)?
            .models
            .iter()
            .find(|model| {
                model.enabled
                    && (model.upstream_name == name || model.variant_names().contains(&name))
            })
            .and_then(|model| model.metadata.as_object())
            .cloned()
    };
    if let Some((_, route)) = routing.route_for(name) {
        let mut sources = route
            .members
            .iter()
            .map(|member| find(&member.provider_id, &member.upstream_model).unwrap_or_default());
        let mut common = sources.next().unwrap_or_default();
        for source in sources {
            common.retain(|key, value| source.get(key) == Some(value));
        }
        return Value::Object(common);
    }
    let source = name.split_once('/').and_then(|(provider_name, model)| {
        let provider = core
            .providers
            .values()
            .find(|p| p.entity.name == provider_name)?;
        find(&provider.entity.id, model)
    });
    Value::Object(source.unwrap_or_default())
}
