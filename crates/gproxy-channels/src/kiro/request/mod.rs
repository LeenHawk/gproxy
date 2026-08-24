mod content;
mod tools;

use bytes::Bytes;
use gproxy_channel_api::ChannelError;
use serde_json::{Map, Value, json};

pub(super) fn build(
    responses_body: &Bytes,
    model: &str,
    conversation_id: &str,
) -> Result<Vec<u8>, ChannelError> {
    let request: gproxy_protocol::openai::ResponseCreateRequest =
        serde_json::from_slice(responses_body).map_err(json_error)?;
    let value = serde_json::to_value(request).map_err(json_error)?;
    let state = conversation_state(&value, model, conversation_id)?;
    let mut output = json!({"conversationState":state});
    if let Some(config) = inference_config(&value) {
        output["inferenceConfig"] = config;
    }
    serde_json::to_vec(&output).map_err(json_error)
}

fn conversation_state(
    value: &Value,
    model: &str,
    conversation_id: &str,
) -> Result<Value, ChannelError> {
    let model = content::map_model(model);
    let mut messages = Vec::new();
    let mut system = content::optional_text(value.get("instructions"));
    let input = value
        .get("input")
        .ok_or_else(|| ChannelError::Prepare("Kiro request requires Responses input".into()))?;
    match input {
        Value::String(text) => {
            content::push_system(&mut messages, system.as_deref(), &model);
            messages.push(content::user(text, &model, Vec::new()));
        }
        Value::Array(items) => {
            let mut results = Vec::new();
            for item in items {
                match item.get("type").and_then(Value::as_str) {
                    Some("function_call") => {
                        tools::flush_results(&mut messages, &mut results, &model);
                        tools::append_call(&mut messages, item);
                        continue;
                    }
                    Some("function_call_output") => {
                        results.push(tools::result(item));
                        continue;
                    }
                    _ => {}
                }
                let role = item.get("role").and_then(Value::as_str).unwrap_or("user");
                let (text, images) = content::text_and_images(item.get("content").unwrap_or(item))?;
                if matches!(role, "system" | "developer") {
                    system = Some(content::join(system.as_deref(), &text));
                    continue;
                }
                if role == "user" {
                    content::push_system(&mut messages, system.take().as_deref(), &model);
                    let mut message = content::user_with_images(&text, &model, images);
                    tools::attach_results(&mut message, std::mem::take(&mut results));
                    messages.push(message);
                } else if role == "assistant" {
                    tools::flush_results(&mut messages, &mut results, &model);
                    messages.push(content::assistant(text));
                } else {
                    return Err(ChannelError::Prepare(format!(
                        "Kiro does not support role {role}"
                    )));
                }
            }
            content::push_system(&mut messages, system.take().as_deref(), &model);
            tools::flush_results(&mut messages, &mut results, &model);
        }
        Value::Object(_) => {
            content::push_system(&mut messages, system.as_deref(), &model);
            let (text, images) = content::text_and_images(input.get("content").unwrap_or(input))?;
            messages.push(content::user_with_images(&text, &model, images));
        }
        _ => {
            return Err(ChannelError::Prepare(
                "Kiro input must be text, array, or object".into(),
            ));
        }
    }
    let mut current = messages
        .pop()
        .ok_or_else(|| ChannelError::Prepare("Kiro request produced no messages".into()))?;
    if current.get("userInputMessage").is_none() {
        return Err(ChannelError::Prepare(
            "Kiro final message must be a user message".into(),
        ));
    }
    let definitions = tools::definitions(value.get("tools"))?;
    tools::attach_definitions(&mut current, definitions);
    Ok(json!({
        "conversationId":conversation_id,
        "history":messages,
        "currentMessage":current,
        "chatTriggerType":"MANUAL",
        "agentTaskType":"vibe"
    }))
}

fn inference_config(value: &Value) -> Option<Value> {
    let mut config = Map::new();
    for (source, target) in [
        ("max_output_tokens", "maxTokens"),
        ("temperature", "temperature"),
        ("top_p", "topP"),
    ] {
        if let Some(value) = value.get(source).cloned() {
            config.insert(target.into(), value);
        }
    }
    (!config.is_empty()).then_some(Value::Object(config))
}

fn json_error(error: serde_json::Error) -> ChannelError {
    ChannelError::Prepare(format!("Kiro Responses JSON: {error}"))
}
