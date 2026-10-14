//! Metadata and Codex projection for the configured downstream model catalog.
use gproxy_core::CoreData;
use gproxy_protocol::wire::openai::models::Model;
use gproxy_sdk::resolve::RoutingTable;
use serde_json::{Value, json};

/// Required Codex fields get neutral defaults; actual model capabilities and extensions win.
pub(super) fn project(id: &str, source: &Value) -> Result<Model, serde_json::Error> {
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
    // Keep the modality tags declared by the Codex catalog protocol.
    if let Some(modalities) = target
        .get_mut("input_modalities")
        .and_then(Value::as_array_mut)
    {
        modalities.retain(|value| matches!(value.as_str(), Some("text" | "image" | "audio")));
    }
    serde_json::from_value(model)
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
