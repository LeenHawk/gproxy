use std::collections::BTreeMap;

use http::StatusCode;

use crate::openai::count_tokens::types::ResponseImageGenerationCallStatus;
use crate::openai::create_response::response::OpenAiCreateResponseResponse;
use crate::openai::create_response::stream::{ResponseStreamErrorPayload, ResponseStreamEvent};
use crate::openai::create_response::types::{
    OpenAiApiError, OpenAiApiErrorResponse, ResponseOutputItem,
};
use crate::openai::types::OpenAiResponseHeaders;
use crate::transform::utils::TransformError;

impl TryFrom<Vec<ResponseStreamEvent>> for OpenAiCreateResponseResponse {
    type Error = TransformError;

    fn try_from(value: Vec<ResponseStreamEvent>) -> Result<Self, TransformError> {
        let mut latest_response = None;
        let mut stream_error = None::<ResponseStreamErrorPayload>;
        // `OutputItemDone` events are the source of truth for the final
        // response body's `output` array — the `response.completed`
        // snapshot returned by codex (and matching OpenAI's Responses
        // API spec) ships with `output: []`, and the per-item content
        // is only visible on the incremental `output_item.done` stream
        // events. Collect those in `output_index` order and inject
        // them into `latest_response.output` below.
        let mut output_items: BTreeMap<u64, ResponseOutputItem> = BTreeMap::new();

        for event in value {
            match event {
                ResponseStreamEvent::Created { response, .. }
                | ResponseStreamEvent::Queued { response, .. }
                | ResponseStreamEvent::InProgress { response, .. }
                | ResponseStreamEvent::Completed { response, .. }
                | ResponseStreamEvent::Incomplete { response, .. }
                | ResponseStreamEvent::Failed { response, .. } => {
                    latest_response = Some(response);
                }
                ResponseStreamEvent::OutputItemDone {
                    mut item,
                    output_index,
                    ..
                } => {
                    // Codex ships `output_item.done` for image_generation_call
                    // with `status:"generating"` even though the item is final
                    // (the result base64 is fully populated and no further
                    // events follow). Normalize to `completed` so non-stream
                    // consumers see a terminal status, matching the OpenAI
                    // Responses API spec.
                    if let ResponseOutputItem::ImageGenerationCall(call) = &mut item
                        && matches!(
                            call.status,
                            ResponseImageGenerationCallStatus::Generating
                                | ResponseImageGenerationCallStatus::InProgress
                        )
                    {
                        call.status = ResponseImageGenerationCallStatus::Completed;
                    }
                    output_items.insert(output_index, item);
                }
                ResponseStreamEvent::Error { error, .. } => stream_error = Some(error),
                _ => {}
            }
        }

        if let Some(mut body) = latest_response {
            if body.output.is_empty() && !output_items.is_empty() {
                body.output = output_items.into_values().collect();
            }
            Ok(OpenAiCreateResponseResponse::Success {
                stats_code: StatusCode::OK,
                headers: OpenAiResponseHeaders::default(),
                body,
            })
        } else if let Some(error) = stream_error {
            Ok(OpenAiCreateResponseResponse::Error {
                stats_code: StatusCode::BAD_REQUEST,
                headers: OpenAiResponseHeaders::default(),
                body: OpenAiApiErrorResponse {
                    error: OpenAiApiError {
                        message: error.message,
                        type_: error.type_,
                        param: error.param,
                        code: error.code,
                    },
                },
            })
        } else {
            Err(TransformError::not_implemented(
                "cannot convert OpenAI response SSE stream body without response snapshots",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::kinds::ProtocolKind;
    use serde_json::{Value, json};

    // Mirrors what chatgpt.com/backend-api/codex/responses actually sends for
    // a `tools:[{type:image_generation}]` call: `output_item.added` ships the
    // image item WITHOUT `result`, and the base64 only lands on
    // `output_item.done`. The ResponseOutputItem untagged enum must still
    // match the added frame — otherwise aggregation fails and the handler
    // returns a 500 with no upstream log body.
    #[test]
    fn stream_to_nonstream_reconstructs_codex_image_generation_output() {
        let chunks = [
            serde_json::to_vec(&json!({
                "type": "response.output_item.added",
                "item": {
                    "id": "ig_1",
                    "type": "image_generation_call",
                    "status": "in_progress"
                },
                "output_index": 0,
                "sequence_number": 2
            }))
            .expect("serialize output_item.added"),
            serde_json::to_vec(&json!({
                "type": "response.image_generation_call.partial_image",
                "background": "opaque",
                "item_id": "ig_1",
                "output_format": "png",
                "output_index": 0,
                "partial_image_b64": "Zm9v",
                "partial_image_index": 0,
                "revised_prompt": "cute gray tabby cat hugging an otter",
                "size": "1122x1402",
                "sequence_number": 7
            }))
            .expect("serialize partial_image"),
            serde_json::to_vec(&json!({
                "type": "response.output_item.done",
                "item": {
                    "id": "ig_1",
                    "type": "image_generation_call",
                    "status": "completed",
                    "action": "generate",
                    "background": "opaque",
                    "output_format": "png",
                    "quality": "medium",
                    "result": "iVBORw0KGgo="
                },
                "output_index": 0,
                "sequence_number": 11
            }))
            .expect("serialize output_item.done"),
            serde_json::to_vec(&json!({
                "type": "response.completed",
                "response": {
                    "id": "resp_1",
                    "created_at": 1776994440u64,
                    "metadata": {},
                    "model": "gpt-5.5",
                    "object": "response",
                    "output": [],
                    "parallel_tool_calls": true,
                    "temperature": 1.0,
                    "tool_choice": {
                        "type": "image_generation"
                    },
                    "tools": [{
                        "type": "image_generation"
                    }],
                    "top_p": 0.98,
                    "status": "completed"
                },
                "sequence_number": 13
            }))
            .expect("serialize response.completed"),
        ];
        let chunk_refs = chunks.iter().map(Vec::as_slice).collect::<Vec<_>>();

        let body = crate::transform::dispatch::stream_to_nonstream(
            ProtocolKind::OpenAiResponse,
            &chunk_refs,
        )
        .expect("aggregate image response stream");
        let json: Value = serde_json::from_slice(&body).expect("parse aggregated response");

        assert_eq!(
            json.get("status").and_then(Value::as_str),
            Some("completed")
        );
        assert_eq!(json["output"][0]["type"], "image_generation_call");
        assert_eq!(json["output"][0]["status"], "completed");
        assert_eq!(json["output"][0]["result"], "iVBORw0KGgo=");
    }

    // Codex's `output_item.done` for image_generation_call ships
    // `status:"generating"` even though the item is final — the aggregator
    // must normalize that to `"completed"` so downstream Zod / spec
    // validators don't reject the non-stream response.
    #[test]
    fn stream_to_nonstream_normalizes_codex_generating_status_to_completed() {
        let chunks = [
            serde_json::to_vec(&json!({
                "type": "response.output_item.done",
                "item": {
                    "id": "ig_1",
                    "type": "image_generation_call",
                    "status": "generating",
                    "result": "iVBORw0KGgo="
                },
                "output_index": 0,
                "sequence_number": 1
            }))
            .expect("serialize output_item.done"),
            serde_json::to_vec(&json!({
                "type": "response.completed",
                "response": {
                    "id": "resp_1",
                    "created_at": 1u64,
                    "metadata": {},
                    "model": "gpt-5.5",
                    "object": "response",
                    "output": [],
                    "parallel_tool_calls": true,
                    "temperature": 1.0,
                    "tool_choice": {"type": "image_generation"},
                    "tools": [{"type": "image_generation"}],
                    "top_p": 0.98,
                    "status": "completed"
                },
                "sequence_number": 2
            }))
            .expect("serialize response.completed"),
        ];
        let chunk_refs = chunks.iter().map(Vec::as_slice).collect::<Vec<_>>();

        let body = crate::transform::dispatch::stream_to_nonstream(
            ProtocolKind::OpenAiResponse,
            &chunk_refs,
        )
        .expect("aggregate");
        let json: Value = serde_json::from_slice(&body).expect("parse");
        assert_eq!(json["output"][0]["status"], "completed");
    }
}
