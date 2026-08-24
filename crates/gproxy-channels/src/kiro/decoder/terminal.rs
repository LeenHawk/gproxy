use gproxy_channel_api::Frame;
use serde_json::json;

use super::KiroDecoder;

pub(super) fn open_content(state: &mut KiroDecoder) -> Vec<Frame> {
    let first = state.take();
    let second = state.take();
    let item = super::super::sse::message(&state.message_id, "", "in_progress");
    let item_id = state.message_id.clone();
    vec![
        super::super::sse::frame(json!({
            "type":"response.output_item.added","sequence_number":first,
            "output_index":0,"item":item
        })),
        super::super::sse::frame(json!({
            "type":"response.content_part.added","sequence_number":second,
            "output_index":0,"item_id":item_id,"content_index":0,
            "part":{"type":"output_text","text":"","annotations":[]}
        })),
    ]
}

pub(super) fn open_reasoning(state: &mut KiroDecoder) -> Vec<Frame> {
    let sequence = state.take();
    let item = super::super::sse::reasoning(&state.reasoning_id, "", "in_progress");
    vec![super::super::sse::frame(json!({
        "type":"response.output_item.added","sequence_number":sequence,
        "output_index":1,"item":item
    }))]
}

pub(super) fn finish(state: &mut KiroDecoder) -> Vec<Frame> {
    let mut frames = Vec::new();
    let message_id = state.message_id.clone();
    let reasoning_id = state.reasoning_id.clone();
    let content = state.content.clone();
    let reasoning = state.reasoning.clone();
    if state.content_started {
        let text_done = state.take();
        let part_done = state.take();
        let item_done = state.take();
        frames.push(super::super::sse::frame(json!({
            "type":"response.output_text.done","sequence_number":text_done,
            "output_index":0,"item_id":&message_id,"content_index":0,"text":&content
        })));
        frames.push(super::super::sse::frame(json!({
            "type":"response.content_part.done","sequence_number":part_done,
            "output_index":0,"item_id":&message_id,"content_index":0,
            "part":{"type":"output_text","text":&content,"annotations":[]}
        })));
        let item = super::super::sse::message(&message_id, &content, "completed");
        frames.push(super::super::sse::frame(json!({
            "type":"response.output_item.done","sequence_number":item_done,
            "output_index":0,"item":item
        })));
    }
    if state.reasoning_started {
        let text_done = state.take();
        let item_done = state.take();
        frames.push(super::super::sse::frame(json!({
            "type":"response.reasoning_text.done","sequence_number":text_done,
            "output_index":1,"item_id":&reasoning_id,"content_index":0,"text":&reasoning
        })));
        let item = super::super::sse::reasoning(&reasoning_id, &reasoning, "completed");
        frames.push(super::super::sse::frame(json!({
            "type":"response.output_item.done","sequence_number":item_done,
            "output_index":1,"item":item
        })));
    }
    let mut output = Vec::new();
    if state.content_started {
        output.push(super::super::sse::message(
            &message_id,
            &content,
            "completed",
        ));
    }
    if state.reasoning_started {
        output.push(super::super::sse::reasoning(
            &reasoning_id,
            &reasoning,
            "completed",
        ));
    }
    output.extend(state.tools.items());
    let mut response =
        super::super::sse::response(&state.response_id, &state.model, "completed", output);
    if state.content_started {
        response["output_text"] = serde_json::Value::String(content);
    }
    if let Some(usage) = state.usage.as_ref() {
        response["usage"] = super::super::usage::response_value(usage);
    }
    let sequence = state.take();
    frames.push(super::super::sse::frame(json!({
        "type":"response.completed","sequence_number":sequence,"response":response
    })));
    frames
}
