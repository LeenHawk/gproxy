//! An OpenAI Responses request turned into the CodeWhisperer conversation
//! envelope `GenerateAssistantResponse` takes (v3 `kiro/request/`).
//!
//! The upstream has no notion of a message list with mixed roles: it takes a
//! `history` of alternating user/assistant turns plus one `currentMessage`
//! that must be a user turn. System text becomes a user turn with a fixed
//! acknowledgement after it, tool calls attach to the preceding assistant
//! turn and tool results attach to the following user turn.

mod content;
mod tools;

use crate::channel::ChannelError;
use gproxy_protocol::connection::Bytes;
use serde_json::{Map, Value, json};

fn invalid(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidConfig(message.into())
}

/// `{conversationState, inferenceConfig?}` from a Responses body.
pub(super) fn build(body: &[u8], conversation_id: &str) -> Result<Bytes, ChannelError> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| invalid(format!("Kiro needs a Responses JSON body: {error}")))?;
    let model = content::map_model(
        value
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or_default(),
    );
    let state = conversation_state(&value, &model, conversation_id)?;
    let mut output = json!({ "conversationState": state });
    if let Some(config) = inference_config(&value) {
        output["inferenceConfig"] = config;
    }
    Ok(Bytes::from(output.to_string()))
}

fn conversation_state(
    value: &Value,
    model: &str,
    conversation_id: &str,
) -> Result<Value, ChannelError> {
    let mut messages = Vec::new();
    let mut system = content::optional_text(value.get("instructions"));
    let input = value
        .get("input")
        .ok_or_else(|| invalid("Kiro requires Responses input"))?;
    match input {
        Value::String(text) => {
            content::push_system(&mut messages, system.as_deref(), model);
            messages.push(content::user(text, model, Vec::new()));
        }
        Value::Array(items) => {
            let mut results = Vec::new();
            for item in items {
                match item.get("type").and_then(Value::as_str) {
                    Some("function_call") => {
                        tools::flush_results(&mut messages, &mut results, model);
                        tools::append_call(&mut messages, item)?;
                        continue;
                    }
                    Some("function_call_output") => {
                        results.push(tools::result(item)?);
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
                match role {
                    "user" => {
                        content::push_system(&mut messages, system.take().as_deref(), model);
                        let mut message = content::user(&text, model, images);
                        tools::attach_results(&mut message, std::mem::take(&mut results));
                        messages.push(message);
                    }
                    "assistant" => {
                        tools::flush_results(&mut messages, &mut results, model);
                        messages.push(content::assistant(text));
                    }
                    other => return Err(invalid(format!("Kiro has no role `{other}`"))),
                }
            }
            content::push_system(&mut messages, system.take().as_deref(), model);
            tools::flush_results(&mut messages, &mut results, model);
        }
        Value::Object(_) => {
            content::push_system(&mut messages, system.as_deref(), model);
            let (text, images) = content::text_and_images(input.get("content").unwrap_or(input))?;
            messages.push(content::user(&text, model, images));
        }
        _ => return Err(invalid("Kiro input must be text, an array or an object")),
    }
    let mut current = messages
        .pop()
        .ok_or_else(|| invalid("Kiro request produced no messages"))?;
    if current.get("userInputMessage").is_none() {
        return Err(invalid("Kiro's last message must be a user message"));
    }
    tools::attach_definitions(&mut current, tools::definitions(value.get("tools"))?);
    Ok(json!({
        "conversationId": conversation_id,
        "history": messages,
        "currentMessage": current,
        "chatTriggerType": "MANUAL",
        "agentTaskType": "vibe",
    }))
}

/// The three sampling fields the envelope has a place for, under AWS's names.
fn inference_config(value: &Value) -> Option<Value> {
    let mut config = Map::new();
    for (source, target) in [
        ("max_output_tokens", "maxTokens"),
        ("temperature", "temperature"),
        ("top_p", "topP"),
    ] {
        if let Some(value) = value.get(source).cloned().filter(|value| !value.is_null()) {
            config.insert(target.into(), value);
        }
    }
    (!config.is_empty()).then_some(Value::Object(config))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn envelope(body: Value) -> Value {
        serde_json::from_slice(&build(body.to_string().as_bytes(), "conv-1").unwrap()).unwrap()
    }

    #[test]
    fn instructions_become_a_user_turn_with_an_acknowledgement() {
        let out = envelope(json!({
            "model": "claude-sonnet-4-5",
            "instructions": "be brief",
            "input": "hello",
            "temperature": 0.2,
        }));
        let state = &out["conversationState"];
        assert_eq!(state["conversationId"], "conv-1");
        assert_eq!(
            state["history"][0]["userInputMessage"]["content"],
            "be brief"
        );
        assert_eq!(
            state["history"][1]["assistantResponseMessage"]["content"],
            "I will follow these instructions."
        );
        assert_eq!(
            state["currentMessage"]["userInputMessage"]["content"],
            "hello"
        );
        assert_eq!(
            state["currentMessage"]["userInputMessage"]["modelId"], "claude-sonnet-4.5",
            "the envelope spells a minor version with a dot"
        );
        assert_eq!(state["chatTriggerType"], "MANUAL");
        assert_eq!(out["inferenceConfig"]["temperature"], 0.2);
        assert!(out["inferenceConfig"].get("maxTokens").is_none());
    }

    #[test]
    fn a_tool_call_attaches_to_the_assistant_turn_and_its_result_to_the_next_user_turn() {
        let out = envelope(json!({
            "model": "claude-sonnet-4.5",
            "input": [
                {"role": "user", "content": [{"type": "input_text", "text": "list"}]},
                {"type": "function_call", "call_id": "call-1", "name": "read_file",
                 "arguments": "{\"path\":\"a\"}"},
                {"type": "function_call_output", "call_id": "call-1", "output": "ok"},
                {"role": "user", "content": "and now?"},
            ],
            "tools": [{"type": "function", "name": "read.file", "description": "d",
                       "parameters": {"type": "object", "additionalProperties": false,
                                      "required": []}}],
        }));
        let state = &out["conversationState"];
        let uses = &state["history"][1]["assistantResponseMessage"]["toolUses"];
        assert_eq!(uses[0]["toolUseId"], "call-1");
        assert_eq!(uses[0]["input"], json!({"path": "a"}));
        let results =
            &state["currentMessage"]["userInputMessage"]["userInputMessageContext"]["toolResults"];
        assert_eq!(results[0]["toolUseId"], "call-1");
        assert_eq!(results[0]["content"][0]["text"], "ok");
        let spec = &state["currentMessage"]["userInputMessage"]["userInputMessageContext"]["tools"]
            [0]["toolSpecification"];
        assert_eq!(
            spec["name"], "readFile",
            "the upstream takes camelCase names"
        );
        assert!(
            spec["inputSchema"]["json"]
                .get("additionalProperties")
                .is_none()
        );
        assert!(spec["inputSchema"]["json"].get("required").is_none());
    }

    #[test]
    fn a_conversation_that_cannot_end_on_a_user_turn_is_refused() {
        let error = build(
            json!({"model": "m", "input": [{"role": "assistant", "content": "hi"}]})
                .to_string()
                .as_bytes(),
            "c",
        );
        assert!(matches!(error, Err(ChannelError::InvalidConfig(_))));
        assert!(matches!(
            build(json!({"model": "m"}).to_string().as_bytes(), "c"),
            Err(ChannelError::InvalidConfig(_))
        ));
    }
}
