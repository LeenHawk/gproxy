use bytes::Bytes;
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKind};
use serde_json::{Map, Value, json};

use crate::boundary::RequestCtx;
use crate::control::ControlPlane;
use crate::error::CoreError;

use super::request::Classified;

#[derive(Clone, Copy)]
enum Preset {
    Thinking(&'static str),
    Tier(&'static str),
}

pub(crate) fn apply(
    control: &impl ControlPlane,
    request: &mut RequestCtx,
    classified: &mut Classified,
) -> Result<(), CoreError> {
    let Some(requested) = classified.model.clone() else {
        return Ok(());
    };
    let aliased = control.resolve_alias(&requested, &request.mode);
    let declared_base = (classified.key.operation != Operation::GetModel)
        .then(|| control.resolve_variant(&aliased, &request.mode))
        .flatten();
    let (model, presets) = match declared_base {
        Some(base) => {
            let (stripped, presets) = strip_presets(&aliased, classified.key.kind);
            let presets = if stripped == base {
                presets
            } else {
                Vec::new()
            };
            (base, presets)
        }
        None => (aliased, Vec::new()),
    };
    if model == requested && presets.is_empty() {
        return Ok(());
    }
    rewrite_path(&mut request.path, &requested, &model);
    rewrite_body(request, &requested, &model, &presets, classified.key.kind)?;
    classified.model = Some(model);
    Ok(())
}

fn strip_presets(model: &str, kind: OperationKind) -> (String, Vec<Preset>) {
    let mut model = model.to_owned();
    let mut presets = Vec::new();
    loop {
        let preset = tier(&model).or_else(|| thinking(&model, kind));
        let Some((base, preset)) = preset else { break };
        model.truncate(base);
        presets.push(preset);
    }
    presets.reverse();
    (model, presets)
}

fn tier(model: &str) -> Option<(usize, Preset)> {
    [
        ("-tier-priority", "priority"),
        ("-tier-default", "default"),
        ("-tier-scale", "scale"),
        ("-tier-flex", "flex"),
        ("-tier-auto", "auto"),
        ("-fast", "priority"),
    ]
    .into_iter()
    .find_map(|(suffix, value)| {
        model
            .strip_suffix(suffix)
            .map(|base| (base.len(), Preset::Tier(value)))
    })
}

fn thinking(model: &str, kind: OperationKind) -> Option<(usize, Preset)> {
    if !matches!(kind, OperationKind::ContentGeneration(_)) {
        return None;
    }
    [
        ("-thinking-adaptive", "adaptive"),
        ("-thinking-medium", "medium"),
        ("-thinking-xhigh", "xhigh"),
        ("-thinking-high", "high"),
        ("-thinking-none", "none"),
        ("-thinking-low", "low"),
    ]
    .into_iter()
    .find_map(|(suffix, value)| {
        model
            .strip_suffix(suffix)
            .map(|base| (base.len(), Preset::Thinking(value)))
    })
}

fn rewrite_path(path: &mut String, requested: &str, model: &str) {
    if path.contains(requested) {
        *path = path.replacen(requested, model, 1);
    }
}

fn rewrite_body(
    request: &mut RequestCtx,
    requested: &str,
    model: &str,
    presets: &[Preset],
    kind: OperationKind,
) -> Result<(), CoreError> {
    let Some(mut value) = serde_json::from_slice::<Value>(&request.body).ok() else {
        return Ok(());
    };
    let Some(root) = value.as_object_mut() else {
        return Ok(());
    };
    replace_model(root, requested, model);
    for preset in presets {
        apply_preset(root, *preset, kind);
    }
    request.body = Bytes::from(
        serde_json::to_vec(&value)
            .map_err(|error| CoreError::Transform(format!("suffix preset: {error}")))?,
    );
    Ok(())
}

fn replace_model(root: &mut Map<String, Value>, requested: &str, model: &str) {
    if root.get("model").and_then(Value::as_str) == Some(requested) {
        root.insert("model".into(), Value::String(model.into()));
    }
    if let Some(session) = root.get_mut("session").and_then(Value::as_object_mut)
        && session.get("model").and_then(Value::as_str) == Some(requested)
    {
        session.insert("model".into(), Value::String(model.into()));
    }
}

fn apply_preset(root: &mut Map<String, Value>, preset: Preset, kind: OperationKind) {
    match preset {
        Preset::Tier(tier) => {
            root.insert("service_tier".into(), Value::String(tier.into()));
        }
        Preset::Thinking(effort) => match kind {
            OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChat) => {
                root.insert("reasoning_effort".into(), Value::String(effort.into()));
            }
            OperationKind::ContentGeneration(
                ContentGenerationKind::OpenAiResponses
                | ContentGenerationKind::OpenAiResponsesWebSocket,
            ) => {
                root.insert("reasoning".into(), json!({ "effort": effort }));
            }
            OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages) => {
                root.insert("thinking".into(), claude_thinking(effort));
            }
            OperationKind::ContentGeneration(ContentGenerationKind::GeminiGenerateContent) => {
                let generation = root.entry("generationConfig").or_insert_with(|| json!({}));
                if let Some(generation) = generation.as_object_mut() {
                    generation.insert(
                        "thinkingConfig".into(),
                        json!({ "thinkingLevel": effort.to_ascii_uppercase() }),
                    );
                }
            }
            OperationKind::Family(_) => {}
        },
    }
}

fn claude_thinking(effort: &str) -> Value {
    match effort {
        "adaptive" => json!({ "type": "adaptive", "display": "summarized" }),
        "none" => json!({ "type": "disabled" }),
        value => {
            let budget = match value {
                "low" => 1_024,
                "medium" => 10_240,
                "high" | "xhigh" => 32_768,
                _ => return json!({ "type": "disabled" }),
            };
            json!({ "type": "enabled", "budget_tokens": budget, "display": "summarized" })
        }
    }
}
