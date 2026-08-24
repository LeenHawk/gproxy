use gproxy_channel_api::{ChannelError, Frame};
use serde_json::{Value, json};

use super::KiroDecoder;

pub(super) fn handle(
    state: &mut KiroDecoder,
    frame: crate::shared::aws_eventstream::Frame,
    output: &mut Vec<Frame>,
) -> Result<(), ChannelError> {
    let event = frame
        .exception_type
        .as_deref()
        .or(frame.event_type.as_deref())
        .unwrap_or_default();
    let value: Value = serde_json::from_slice(&frame.payload)
        .map_err(|error| ChannelError::Decode(format!("Kiro event JSON: {error}")))?;
    let payload = value.get(event).unwrap_or(&value);
    match event {
        "assistantResponseEvent" => assistant(state, payload, output),
        "reasoningContentEvent" => reasoning(state, payload, output),
        "metadataEvent" => {
            if let Some(usage) = payload
                .get("tokenUsage")
                .and_then(super::super::usage::from_value)
            {
                state.usage = Some(usage);
            }
        }
        "messageMetadataEvent" if !state.started => {
            if let Some(id) = payload.get("conversationId").and_then(Value::as_str) {
                state.response_id = id.into();
                state.message_id = super::super::sse::id("msg", id);
                state.reasoning_id = super::super::sse::id("rs", id);
            }
        }
        "toolUseEvent" => {
            state.ensure_started(output);
            output.extend(state.tools.handle(payload, &mut state.sequence)?);
        }
        "invalidStateEvent" | "InternalServerException" | "internalServerException" => {
            state.ensure_started(output);
            state.failed = true;
            let sequence = state.take();
            output.push(super::super::sse::frame(json!({
                "type":"error","sequence_number":sequence,
                "code":"kiro_eventstream_error","param":null,
                "message":payload.get("message").or_else(||payload.get("reason"))
                    .and_then(Value::as_str).unwrap_or("Kiro stream failed")
            })));
        }
        _ => {}
    }
    Ok(())
}

fn assistant(state: &mut KiroDecoder, value: &Value, output: &mut Vec<Frame>) {
    let Some(value) = value.get("content").and_then(Value::as_str) else {
        return;
    };
    let delta = super::super::sse::percent_decode(&super::super::sse::dedup(
        value,
        &mut state.last_content,
    ));
    if delta.is_empty() {
        return;
    }
    state.ensure_started(output);
    if !state.content_started {
        state.content_started = true;
        output.extend(super::terminal::open_content(state));
    }
    state.content.push_str(&delta);
    let sequence = state.take();
    let item_id = state.message_id.clone();
    output.push(super::super::sse::frame(json!({
        "type":"response.output_text.delta","sequence_number":sequence,
        "output_index":0,"item_id":item_id,"content_index":0,"delta":delta
    })));
}

fn reasoning(state: &mut KiroDecoder, value: &Value, output: &mut Vec<Frame>) {
    let Some(value) = value
        .get("text")
        .or_else(|| value.get("content"))
        .and_then(Value::as_str)
    else {
        return;
    };
    let delta = super::super::sse::percent_decode(&super::super::sse::dedup(
        value,
        &mut state.last_reasoning,
    ));
    if delta.is_empty() {
        return;
    }
    state.ensure_started(output);
    if !state.reasoning_started {
        state.reasoning_started = true;
        output.extend(super::terminal::open_reasoning(state));
    }
    state.reasoning.push_str(&delta);
    let sequence = state.take();
    let item_id = state.reasoning_id.clone();
    output.push(super::super::sse::frame(json!({
        "type":"response.reasoning_text.delta","sequence_number":sequence,
        "output_index":1,"item_id":item_id,"content_index":0,"delta":delta
    })));
}
