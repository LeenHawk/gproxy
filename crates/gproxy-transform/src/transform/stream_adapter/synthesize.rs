use serde_json::{Value, json};

use super::{ContentGenerationKind, SseFrame};

/// Turn one complete response into the smallest useful SSE event sequence.
pub fn synthesize_sse(
    kind: ContentGenerationKind,
    body: &[u8],
) -> Result<Vec<u8>, crate::transform::TransformError> {
    let value: Value = serde_json::from_slice(body).map_err(|error| {
        crate::transform::TransformError::InvalidInput {
            reason: format!("synthetic stream response is not JSON: {error}"),
        }
    })?;
    let mut out = String::new();
    match kind {
        ContentGenerationKind::OpenAiChatCompletions => synthesize_chat(&value, &mut out),
        ContentGenerationKind::OpenAiResponses
        | ContentGenerationKind::OpenAiResponsesWebSocket => synthesize_responses(&value, &mut out),
        ContentGenerationKind::ClaudeMessages => synthesize_claude(&value, &mut out),
        ContentGenerationKind::GeminiGenerateContent => {
            out.push_str(&SseFrame::data(value.to_string()).encode());
        }
    }
    Ok(out.into_bytes())
}

fn synthesize_chat(response: &Value, out: &mut String) {
    let choices = response
        .get("choices")
        .and_then(Value::as_array)
        .map(|choices| {
            choices
                .iter()
                .map(|choice| {
                    let mut delta = choice.get("message").cloned().unwrap_or_else(|| json!({}));
                    if let Some(object) = delta.as_object_mut() {
                        object.remove("annotations");
                    }
                    json!({
                        "index":choice.get("index").cloned().unwrap_or_else(|| json!(0)),
                        "delta":delta,
                        "finish_reason":choice.get("finish_reason").cloned().unwrap_or(Value::Null),
                        "logprobs":choice.get("logprobs").cloned().unwrap_or(Value::Null),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut chunk = response.clone();
    if let Some(object) = chunk.as_object_mut() {
        object.insert("object".into(), json!("chat.completion.chunk"));
        object.insert("choices".into(), Value::Array(choices));
    }
    out.push_str(&SseFrame::data(chunk.to_string()).encode());
    out.push_str(&SseFrame::data("[DONE]").encode());
}

fn synthesize_responses(response: &Value, out: &mut String) {
    let mut started = response.clone();
    if let Some(object) = started.as_object_mut() {
        object.insert("status".into(), json!("in_progress"));
        object.insert("output".into(), json!([]));
    }
    push_named(out, json!({"type":"response.created","response":started}));

    for (output_index, item) in response
        .get("output")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let mut added_item = item.clone();
        if let Some(object) = added_item.as_object_mut() {
            object.insert("status".into(), json!("in_progress"));
        }
        push_named(
            out,
            json!({"type":"response.output_item.added","output_index":output_index,"item":added_item}),
        );
        let item_id = item
            .get("id")
            .cloned()
            .unwrap_or_else(|| json!(format!("item_{output_index}")));
        match item.get("type").and_then(Value::as_str) {
            Some("message") => synthesize_response_message(out, item, output_index, &item_id),
            Some("function_call") => {
                let arguments = item.get("arguments").cloned().unwrap_or_else(|| json!(""));
                push_named(
                    out,
                    json!({"type":"response.function_call_arguments.delta","item_id":item_id,"output_index":output_index,"delta":arguments}),
                );
                push_named(
                    out,
                    json!({"type":"response.function_call_arguments.done","item_id":item_id,"output_index":output_index,"name":item.get("name"),"arguments":arguments}),
                );
            }
            Some("custom_tool_call") => {
                let input = item.get("input").cloned().unwrap_or_else(|| json!(""));
                push_named(
                    out,
                    json!({"type":"response.custom_tool_call_input.delta","item_id":item_id,"output_index":output_index,"delta":input}),
                );
                push_named(
                    out,
                    json!({"type":"response.custom_tool_call_input.done","item_id":item_id,"output_index":output_index,"input":input}),
                );
            }
            Some("reasoning") => synthesize_response_reasoning(out, item, output_index, &item_id),
            _ => {}
        }
        push_named(
            out,
            json!({"type":"response.output_item.done","output_index":output_index,"item":item}),
        );
    }
    push_named(
        out,
        json!({"type":"response.completed","response":response}),
    );
}

fn synthesize_response_message(
    out: &mut String,
    item: &Value,
    output_index: usize,
    item_id: &Value,
) {
    for (content_index, part) in item
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        push_named(
            out,
            json!({"type":"response.content_part.added","item_id":item_id,"output_index":output_index,"content_index":content_index,"part":part}),
        );
        let (delta_type, done_type, field) = match part.get("type").and_then(Value::as_str) {
            Some("refusal") => ("response.refusal.delta", "response.refusal.done", "refusal"),
            _ => (
                "response.output_text.delta",
                "response.output_text.done",
                "text",
            ),
        };
        let text = part.get(field).cloned().unwrap_or_else(|| json!(""));
        push_named(
            out,
            json!({"type":delta_type,"item_id":item_id,"output_index":output_index,"content_index":content_index,"delta":text}),
        );
        push_named(
            out,
            json!({"type":done_type,"item_id":item_id,"output_index":output_index,"content_index":content_index,(field):text}),
        );
        push_named(
            out,
            json!({"type":"response.content_part.done","item_id":item_id,"output_index":output_index,"content_index":content_index,"part":part}),
        );
    }
}

fn synthesize_response_reasoning(
    out: &mut String,
    item: &Value,
    output_index: usize,
    item_id: &Value,
) {
    for (content_index, part) in item
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        if let Some(text) = part.get("text") {
            push_named(
                out,
                json!({"type":"response.reasoning_text.delta","item_id":item_id,"output_index":output_index,"content_index":content_index,"delta":text}),
            );
            push_named(
                out,
                json!({"type":"response.reasoning_text.done","item_id":item_id,"output_index":output_index,"content_index":content_index,"text":text}),
            );
        }
    }
}

fn synthesize_claude(response: &Value, out: &mut String) {
    let mut message = response.clone();
    if let Some(object) = message.as_object_mut() {
        object.insert("content".into(), json!([]));
        object.insert("stop_reason".into(), Value::Null);
        object.insert("stop_sequence".into(), Value::Null);
    }
    push_named(out, json!({"type":"message_start","message":message}));
    for (index, block) in response
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let mut start = block.clone();
        if let Some(object) = start.as_object_mut() {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => {
                    object.insert("text".into(), json!(""));
                }
                Some("thinking") => {
                    object.insert("thinking".into(), json!(""));
                }
                Some("tool_use") => {
                    object.insert("input".into(), json!({}));
                }
                _ => {}
            }
        }
        push_named(
            out,
            json!({"type":"content_block_start","index":index,"content_block":start}),
        );
        match block.get("type").and_then(Value::as_str) {
            Some("text") => push_named(
                out,
                json!({"type":"content_block_delta","index":index,"delta":{"type":"text_delta","text":block.get("text").cloned().unwrap_or_else(|| json!(""))}}),
            ),
            Some("thinking") => {
                push_named(
                    out,
                    json!({"type":"content_block_delta","index":index,"delta":{"type":"thinking_delta","thinking":block.get("thinking").cloned().unwrap_or_else(|| json!(""))}}),
                );
                if let Some(signature) = block.get("signature") {
                    push_named(
                        out,
                        json!({"type":"content_block_delta","index":index,"delta":{"type":"signature_delta","signature":signature}}),
                    );
                }
            }
            Some("tool_use") => push_named(
                out,
                json!({"type":"content_block_delta","index":index,"delta":{"type":"input_json_delta","partial_json":block.get("input").cloned().unwrap_or_else(|| json!({})).to_string()}}),
            ),
            _ => {}
        }
        push_named(out, json!({"type":"content_block_stop","index":index}));
    }
    push_named(
        out,
        json!({"type":"message_delta","delta":{"stop_reason":response.get("stop_reason").cloned().unwrap_or(Value::Null),"stop_sequence":response.get("stop_sequence").cloned().unwrap_or(Value::Null)},"usage":response.get("usage").cloned().unwrap_or_else(|| json!({}))}),
    );
    push_named(out, json!({"type":"message_stop"}));
}

fn push_named(out: &mut String, event: Value) {
    let name = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("message");
    out.push_str(&SseFrame::event(name, event.to_string()).encode());
}
