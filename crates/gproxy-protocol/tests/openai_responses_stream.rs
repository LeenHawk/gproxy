use gproxy_protocol::openai::responses::stream::*;
use serde_json::{Value, json};
fn fixtures() -> Vec<Value> {
    vec![
        json!({"delta": "x", "sequence_number": 1, "type": "response.audio.delta"}),
        json!({"sequence_number": 1, "type": "response.audio.done"}),
        json!({"delta": "x", "sequence_number": 1, "type": "response.audio.transcript.delta"}),
        json!({"sequence_number": 1, "type": "response.audio.transcript.done"}),
        json!({"delta": "x", "item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.code_interpreter_call_code.delta"}),
        json!({"code": "x", "item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.code_interpreter_call_code.done"}),
        json!({"item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.code_interpreter_call.completed"}),
        json!({"item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.code_interpreter_call.in_progress"}),
        json!({"item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.code_interpreter_call.interpreting"}),
        json!({"response": {"id": "r", "created_at": 1, "error": null, "incomplete_details": null, "instructions": null, "metadata": null, "model": "m", "object": "response", "output": [], "parallel_tool_calls": true, "temperature": null, "tool_choice": "auto", "tools": [], "top_p": null}, "sequence_number": 1, "type": "response.completed"}),
        json!({"content_index": 1, "item_id": "x", "output_index": 1, "part": {"type": "reasoning_text", "text": "r"}, "sequence_number": 1, "type": "response.content_part.added"}),
        json!({"content_index": 1, "item_id": "x", "output_index": 1, "part": {"type": "reasoning_text", "text": "r"}, "sequence_number": 1, "type": "response.content_part.done"}),
        json!({"response": {"id": "r", "created_at": 1, "error": null, "incomplete_details": null, "instructions": null, "metadata": null, "model": "m", "object": "response", "output": [], "parallel_tool_calls": true, "temperature": null, "tool_choice": "auto", "tools": [], "top_p": null}, "sequence_number": 1, "type": "response.created"}),
        json!({"code": null, "message": "x", "param": null, "sequence_number": 1, "type": "error"}),
        json!({"item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.file_search_call.completed"}),
        json!({"item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.file_search_call.in_progress"}),
        json!({"item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.file_search_call.searching"}),
        json!({"delta": "x", "item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.function_call_arguments.delta"}),
        json!({"arguments": "x", "item_id": "x", "name": "x", "output_index": 1, "sequence_number": 1, "type": "response.function_call_arguments.done"}),
        json!({"response": {"id": "r", "created_at": 1, "error": null, "incomplete_details": null, "instructions": null, "metadata": null, "model": "m", "object": "response", "output": [], "parallel_tool_calls": true, "temperature": null, "tool_choice": "auto", "tools": [], "top_p": null}, "sequence_number": 1, "type": "response.in_progress"}),
        json!({"response": {"id": "r", "created_at": 1, "error": null, "incomplete_details": null, "instructions": null, "metadata": null, "model": "m", "object": "response", "output": [], "parallel_tool_calls": true, "temperature": null, "tool_choice": "auto", "tools": [], "top_p": null}, "sequence_number": 1, "type": "response.failed"}),
        json!({"response": {"id": "r", "created_at": 1, "error": null, "incomplete_details": null, "instructions": null, "metadata": null, "model": "m", "object": "response", "output": [], "parallel_tool_calls": true, "temperature": null, "tool_choice": "auto", "tools": [], "top_p": null}, "sequence_number": 1, "type": "response.incomplete"}),
        json!({"item": {"type": "compaction", "id": "c", "encrypted_content": "cipher", "created_by": "a"}, "output_index": 1, "sequence_number": 1, "type": "response.output_item.added"}),
        json!({"item": {"type": "compaction", "id": "c", "encrypted_content": "cipher", "created_by": "a"}, "output_index": 1, "sequence_number": 1, "type": "response.output_item.done"}),
        json!({"item_id": "x", "output_index": 1, "part": {"type": "summary_text", "text": "s"}, "sequence_number": 1, "summary_index": 1, "type": "response.reasoning_summary_part.added"}),
        json!({"item_id": "x", "output_index": 1, "part": {"type": "summary_text", "text": "s"}, "sequence_number": 1, "summary_index": 1, "type": "response.reasoning_summary_part.done", "status": "incomplete"}),
        json!({"delta": "x", "item_id": "x", "output_index": 1, "sequence_number": 1, "summary_index": 1, "type": "response.reasoning_summary_text.delta"}),
        json!({"item_id": "x", "output_index": 1, "sequence_number": 1, "summary_index": 1, "text": "x", "type": "response.reasoning_summary_text.done"}),
        json!({"content_index": 1, "delta": "x", "item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.reasoning_text.delta"}),
        json!({"content_index": 1, "item_id": "x", "output_index": 1, "sequence_number": 1, "text": "x", "type": "response.reasoning_text.done"}),
        json!({"content_index": 1, "delta": "x", "item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.refusal.delta"}),
        json!({"content_index": 1, "item_id": "x", "output_index": 1, "refusal": "x", "sequence_number": 1, "type": "response.refusal.done"}),
        json!({"content_index": 1, "delta": "x", "item_id": "x", "logprobs": [{"token": "x", "logprob": -0.5, "top_logprobs": [{}, {"token": "y", "logprob": -1.5}]}], "output_index": 1, "sequence_number": 1, "type": "response.output_text.delta"}),
        json!({"content_index": 1, "item_id": "x", "logprobs": [{"token": "x", "logprob": -0.5, "top_logprobs": [{}, {"token": "y", "logprob": -1.5}]}], "output_index": 1, "sequence_number": 1, "text": "x", "type": "response.output_text.done"}),
        json!({"item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.web_search_call.completed"}),
        json!({"item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.web_search_call.in_progress"}),
        json!({"item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.web_search_call.searching"}),
        json!({"item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.image_generation_call.completed"}),
        json!({"item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.image_generation_call.generating"}),
        json!({"item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.image_generation_call.in_progress"}),
        json!({"item_id": "x", "output_index": 1, "partial_image_b64": "x", "partial_image_index": 1, "sequence_number": 1, "type": "response.image_generation_call.partial_image"}),
        json!({"delta": "x", "item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.mcp_call_arguments.delta"}),
        json!({"arguments": "x", "item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.mcp_call_arguments.done"}),
        json!({"item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.mcp_call.completed"}),
        json!({"item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.mcp_call.failed"}),
        json!({"item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.mcp_call.in_progress"}),
        json!({"item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.mcp_list_tools.completed"}),
        json!({"item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.mcp_list_tools.failed"}),
        json!({"item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.mcp_list_tools.in_progress"}),
        json!({"annotation": {"arbitrary": [1, null]}, "annotation_index": 1, "content_index": 1, "item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.output_text.annotation.added"}),
        json!({"response": {"id": "r", "created_at": 1, "error": null, "incomplete_details": null, "instructions": null, "metadata": null, "model": "m", "object": "response", "output": [], "parallel_tool_calls": true, "temperature": null, "tool_choice": "auto", "tools": [], "top_p": null}, "sequence_number": 1, "type": "response.queued"}),
        json!({"delta": "x", "item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.custom_tool_call_input.delta"}),
        json!({"input": "x", "item_id": "x", "output_index": 1, "sequence_number": 1, "type": "response.custom_tool_call_input.done"}),
    ]
}
fn rest(e: &StreamEvent) -> &gproxy_protocol::Rest {
    match e {
        StreamEvent::Keepalive => unreachable!("keepalive is not a JSON payload fixture"),
        StreamEvent::Created(x) => &x.rest,
        StreamEvent::Queued(x) => &x.rest,
        StreamEvent::InProgress(x) => &x.rest,
        StreamEvent::Completed(x) => &x.rest,
        StreamEvent::Failed(x) => &x.rest,
        StreamEvent::Incomplete(x) => &x.rest,
        StreamEvent::OutputItemAdded(x) => &x.rest,
        StreamEvent::OutputItemDone(x) => &x.rest,
        StreamEvent::ContentPartAdded(x) => &x.rest,
        StreamEvent::ContentPartDone(x) => &x.rest,
        StreamEvent::OutputTextDelta(x) => &x.rest,
        StreamEvent::OutputTextDone(x) => &x.rest,
        StreamEvent::OutputTextAnnotationAdded(x) => &x.rest,
        StreamEvent::RefusalDelta(x) => &x.rest,
        StreamEvent::RefusalDone(x) => &x.rest,
        StreamEvent::ReasoningTextDelta(x) => &x.rest,
        StreamEvent::ReasoningTextDone(x) => &x.rest,
        StreamEvent::ReasoningSummaryPartAdded(x) => &x.rest,
        StreamEvent::ReasoningSummaryPartDone(x) => &x.rest,
        StreamEvent::ReasoningSummaryTextDelta(x) => &x.rest,
        StreamEvent::ReasoningSummaryTextDone(x) => &x.rest,
        StreamEvent::FunctionCallArgumentsDelta(x) => &x.rest,
        StreamEvent::FunctionCallArgumentsDone(x) => &x.rest,
        StreamEvent::CustomToolInputDelta(x) => &x.rest,
        StreamEvent::CustomToolInputDone(x) => &x.rest,
        StreamEvent::CodeInterpreterCodeDelta(x) => &x.rest,
        StreamEvent::CodeInterpreterCodeDone(x) => &x.rest,
        StreamEvent::AudioDelta(x) => &x.rest,
        StreamEvent::AudioDone(x) => &x.rest,
        StreamEvent::AudioTranscriptDelta(x) => &x.rest,
        StreamEvent::AudioTranscriptDone(x) => &x.rest,
        StreamEvent::ImagePartial(x) => &x.rest,
        StreamEvent::ImageCall(x) => &x.rest,
        StreamEvent::ImageGenerating(x) => &x.rest,
        StreamEvent::ImageCompleted(x) => &x.rest,
        StreamEvent::CodeInterpreterInProgress(x) => &x.rest,
        StreamEvent::CodeInterpreterInterpreting(x) => &x.rest,
        StreamEvent::CodeInterpreterCompleted(x) => &x.rest,
        StreamEvent::FileSearchInProgress(x) => &x.rest,
        StreamEvent::FileSearchSearching(x) => &x.rest,
        StreamEvent::FileSearchCompleted(x) => &x.rest,
        StreamEvent::WebSearchInProgress(x) => &x.rest,
        StreamEvent::WebSearchSearching(x) => &x.rest,
        StreamEvent::WebSearchCompleted(x) => &x.rest,
        StreamEvent::McpArgumentsDelta(x) => &x.rest,
        StreamEvent::McpArgumentsDone(x) => &x.rest,
        StreamEvent::McpInProgress(x) => &x.rest,
        StreamEvent::McpCompleted(x) => &x.rest,
        StreamEvent::McpFailed(x) => &x.rest,
        StreamEvent::McpListToolsInProgress(x) => &x.rest,
        StreamEvent::McpListToolsCompleted(x) => &x.rest,
        StreamEvent::McpListToolsFailed(x) => &x.rest,
        StreamEvent::Error(x) => &x.rest,
        #[cfg(not(feature = "exhaustive"))]
        _ => panic!(),
    }
}
#[test]
fn all_53_events_have_known_typed_fields_and_preserve_unknown_extensions() {
    let fixtures = fixtures();
    assert_eq!(fixtures.len(), 53);
    for wire in fixtures {
        let event: StreamEvent = serde_json::from_value(wire.clone()).unwrap();
        assert!(
            rest(&event).is_empty(),
            "{} {:?}",
            wire["type"],
            rest(&event)
        );
        assert_eq!(serde_json::to_value(event).unwrap(), wire);
        let mut x = wire;
        x["future"] = json!({"nested":[true]});
        let event: StreamEvent = serde_json::from_value(x.clone()).unwrap();
        assert_eq!(rest(&event).len(), 1);
        assert_eq!(serde_json::to_value(event).unwrap(), x);
    }
}
#[test]
fn source_derived_every_field_required_optional_and_null_contract() {
    type Contract = (
        &'static str,
        &'static [&'static str],
        &'static [&'static str],
        &'static [&'static str],
    );
    let table: &[Contract] = &[
        (
            "response.audio.delta",
            &["delta", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.audio.done",
            &["sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.audio.transcript.delta",
            &["delta", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.audio.transcript.done",
            &["sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.code_interpreter_call_code.delta",
            &[
                "delta",
                "item_id",
                "output_index",
                "sequence_number",
                "type",
            ],
            &[],
            &[],
        ),
        (
            "response.code_interpreter_call_code.done",
            &["code", "item_id", "output_index", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.code_interpreter_call.completed",
            &["item_id", "output_index", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.code_interpreter_call.in_progress",
            &["item_id", "output_index", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.code_interpreter_call.interpreting",
            &["item_id", "output_index", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.completed",
            &["response", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.content_part.added",
            &[
                "content_index",
                "item_id",
                "output_index",
                "part",
                "sequence_number",
                "type",
            ],
            &[],
            &[],
        ),
        (
            "response.content_part.done",
            &[
                "content_index",
                "item_id",
                "output_index",
                "part",
                "sequence_number",
                "type",
            ],
            &[],
            &[],
        ),
        (
            "response.created",
            &["response", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "error",
            &["code", "message", "param", "sequence_number", "type"],
            &[],
            &["code", "param"],
        ),
        (
            "response.file_search_call.completed",
            &["item_id", "output_index", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.file_search_call.in_progress",
            &["item_id", "output_index", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.file_search_call.searching",
            &["item_id", "output_index", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.function_call_arguments.delta",
            &[
                "delta",
                "item_id",
                "output_index",
                "sequence_number",
                "type",
            ],
            &[],
            &[],
        ),
        (
            "response.function_call_arguments.done",
            &[
                "arguments",
                "item_id",
                "name",
                "output_index",
                "sequence_number",
                "type",
            ],
            &[],
            &[],
        ),
        (
            "response.in_progress",
            &["response", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.failed",
            &["response", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.incomplete",
            &["response", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.output_item.added",
            &["item", "output_index", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.output_item.done",
            &["item", "output_index", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.reasoning_summary_part.added",
            &[
                "item_id",
                "output_index",
                "part",
                "sequence_number",
                "summary_index",
                "type",
            ],
            &[],
            &[],
        ),
        (
            "response.reasoning_summary_part.done",
            &[
                "item_id",
                "output_index",
                "part",
                "sequence_number",
                "summary_index",
                "type",
            ],
            &["status"],
            &[],
        ),
        (
            "response.reasoning_summary_text.delta",
            &[
                "delta",
                "item_id",
                "output_index",
                "sequence_number",
                "summary_index",
                "type",
            ],
            &[],
            &[],
        ),
        (
            "response.reasoning_summary_text.done",
            &[
                "item_id",
                "output_index",
                "sequence_number",
                "summary_index",
                "text",
                "type",
            ],
            &[],
            &[],
        ),
        (
            "response.reasoning_text.delta",
            &[
                "content_index",
                "delta",
                "item_id",
                "output_index",
                "sequence_number",
                "type",
            ],
            &[],
            &[],
        ),
        (
            "response.reasoning_text.done",
            &[
                "content_index",
                "item_id",
                "output_index",
                "sequence_number",
                "text",
                "type",
            ],
            &[],
            &[],
        ),
        (
            "response.refusal.delta",
            &[
                "content_index",
                "delta",
                "item_id",
                "output_index",
                "sequence_number",
                "type",
            ],
            &[],
            &[],
        ),
        (
            "response.refusal.done",
            &[
                "content_index",
                "item_id",
                "output_index",
                "refusal",
                "sequence_number",
                "type",
            ],
            &[],
            &[],
        ),
        (
            "response.output_text.delta",
            &[
                "content_index",
                "delta",
                "item_id",
                "logprobs",
                "output_index",
                "sequence_number",
                "type",
            ],
            &[],
            &[],
        ),
        (
            "response.output_text.done",
            &[
                "content_index",
                "item_id",
                "logprobs",
                "output_index",
                "sequence_number",
                "text",
                "type",
            ],
            &[],
            &[],
        ),
        (
            "response.web_search_call.completed",
            &["item_id", "output_index", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.web_search_call.in_progress",
            &["item_id", "output_index", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.web_search_call.searching",
            &["item_id", "output_index", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.image_generation_call.completed",
            &["item_id", "output_index", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.image_generation_call.generating",
            &["item_id", "output_index", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.image_generation_call.in_progress",
            &["item_id", "output_index", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.image_generation_call.partial_image",
            &[
                "item_id",
                "output_index",
                "partial_image_b64",
                "partial_image_index",
                "sequence_number",
                "type",
            ],
            &[],
            &[],
        ),
        (
            "response.mcp_call_arguments.delta",
            &[
                "delta",
                "item_id",
                "output_index",
                "sequence_number",
                "type",
            ],
            &[],
            &[],
        ),
        (
            "response.mcp_call_arguments.done",
            &[
                "arguments",
                "item_id",
                "output_index",
                "sequence_number",
                "type",
            ],
            &[],
            &[],
        ),
        (
            "response.mcp_call.completed",
            &["item_id", "output_index", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.mcp_call.failed",
            &["item_id", "output_index", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.mcp_call.in_progress",
            &["item_id", "output_index", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.mcp_list_tools.completed",
            &["item_id", "output_index", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.mcp_list_tools.failed",
            &["item_id", "output_index", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.mcp_list_tools.in_progress",
            &["item_id", "output_index", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.output_text.annotation.added",
            &[
                "annotation",
                "annotation_index",
                "content_index",
                "item_id",
                "output_index",
                "sequence_number",
                "type",
            ],
            &[],
            &["annotation"],
        ),
        (
            "response.queued",
            &["response", "sequence_number", "type"],
            &[],
            &[],
        ),
        (
            "response.custom_tool_call_input.delta",
            &[
                "delta",
                "item_id",
                "output_index",
                "sequence_number",
                "type",
            ],
            &[],
            &[],
        ),
        (
            "response.custom_tool_call_input.done",
            &[
                "input",
                "item_id",
                "output_index",
                "sequence_number",
                "type",
            ],
            &[],
            &[],
        ),
    ];
    for (tag, required, optional, nullable) in table {
        let base = fixtures().into_iter().find(|v| v["type"] == *tag).unwrap();
        for field in *required {
            let mut v = base.clone();
            v.as_object_mut().unwrap().remove(*field);
            assert!(
                serde_json::from_value::<StreamEvent>(v).is_err(),
                "{tag} missing {field}"
            );
        }
        for field in *optional {
            let mut v = base.clone();
            v.as_object_mut().unwrap().remove(*field);
            let p: StreamEvent = serde_json::from_value(v.clone()).unwrap();
            assert_eq!(serde_json::to_value(p).unwrap(), v);
        }
        for field in required.iter().chain(optional.iter()) {
            let mut v = base.clone();
            v[*field] = Value::Null;
            let parsed = serde_json::from_value::<StreamEvent>(v.clone());
            if nullable.contains(field) {
                assert_eq!(serde_json::to_value(parsed.unwrap()).unwrap(), v);
            } else {
                assert!(parsed.is_err(), "{tag} null {field}");
            }
        }
    }
}

#[test]
fn stream_logprobs_follow_optional_top_alternatives_not_message_logprobs() {
    for tag in ["response.output_text.delta", "response.output_text.done"] {
        for logs in [
            json!([]),
            json!([{"token":"x","logprob":-0.3}]),
            json!([{"token":"x","logprob":-0.3,"top_logprobs":[{}, {"token":"y"},{"logprob":-1.0}]}]),
        ] {
            let mut wire = fixtures().into_iter().find(|v| v["type"] == tag).unwrap();
            wire["logprobs"] = logs;
            let e: StreamEvent = serde_json::from_value(wire.clone()).unwrap();
            let values = match &e {
                StreamEvent::OutputTextDelta(v) => &v.logprobs,
                StreamEvent::OutputTextDone(v) => &v.logprobs,
                _ => panic!(),
            };
            for v in values {
                assert!(v.rest.is_empty());
                if let Some(top) = &v.top_logprobs {
                    for t in top {
                        assert!(t.rest.is_empty());
                    }
                }
            }
            assert_eq!(serde_json::to_value(e).unwrap(), wire);
        }
    }
    for invalid in [
        json!({"token":"x"}),
        json!({"logprob":0.2}),
        json!({"token":"x","logprob":0.2,"top_logprobs":null}),
    ] {
        assert!(serde_json::from_value::<StreamLogprob>(invalid).is_err());
    }
    for invalid in [json!({"token":null}), json!({"logprob":null})] {
        assert!(serde_json::from_value::<StreamTopLogprob>(invalid).is_err());
    }
}
#[test]
fn content_and_summary_parts_use_exact_known_nested_shapes() {
    for tag in ["response.content_part.added", "response.content_part.done"] {
        for part in [
            json!({"type":"output_text","text":"x","annotations":[],"logprobs":[]}),
            json!({"type":"refusal","refusal":"no"}),
            json!({"type":"reasoning_text","text":"reason"}),
        ] {
            let mut wire = fixtures().into_iter().find(|v| v["type"] == tag).unwrap();
            wire["part"] = part;
            let e: StreamEvent = serde_json::from_value(wire.clone()).unwrap();
            let part = match &e {
                StreamEvent::ContentPartAdded(v) | StreamEvent::ContentPartDone(v) => &v.part,
                _ => panic!(),
            };
            match part {
                OutputContentPart::Text(v) => assert!(v.rest.is_empty()),
                OutputContentPart::Refusal(v) => assert!(v.rest.is_empty()),
                OutputContentPart::Reasoning(v) => assert!(v.rest.is_empty()),
                #[cfg(not(feature = "exhaustive"))]
                _ => panic!(),
            }
            assert_eq!(serde_json::to_value(e).unwrap(), wire);
        }
    }
    for tag in [
        "response.reasoning_summary_part.added",
        "response.reasoning_summary_part.done",
    ] {
        let wire = fixtures().into_iter().find(|v| v["type"] == tag).unwrap();
        let e: StreamEvent = serde_json::from_value(wire.clone()).unwrap();
        let p = match &e {
            StreamEvent::ReasoningSummaryPartAdded(v) => &v.part,
            StreamEvent::ReasoningSummaryPartDone(v) => &v.part,
            _ => panic!(),
        };
        assert!(p.rest.is_empty());
        assert_eq!(serde_json::to_value(e).unwrap(), wire);
    }
    assert!(serde_json::from_value::<SummaryPartStatus>(json!("completed")).is_err());
    assert!(
        serde_json::from_value::<OutputContentPart>(json!({"type":"input_text","text":"x"}))
            .is_err()
    );
}
#[test]
fn annotation_event_preserves_documented_arbitrary_json() {
    // Responses.md:136212 explicitly says unknown, unlike output-text annotations.
    for annotation in [
        Value::Null,
        json!(true),
        json!(1),
        json!("opaque"),
        json!([1, null]),
        json!({"type":"future_annotation","data":true}),
    ] {
        let mut wire = fixtures()
            .into_iter()
            .find(|v| v["type"] == "response.output_text.annotation.added")
            .unwrap();
        wire["annotation"] = annotation;
        let event: StreamEvent = serde_json::from_value(wire.clone()).unwrap();
        assert!(rest(&event).is_empty());
        assert_eq!(serde_json::to_value(event).unwrap(), wire);
    }
}
#[test]
fn lifecycle_and_output_item_reference_the_approved_response_contract() {
    for tag in [
        "response.created",
        "response.queued",
        "response.in_progress",
        "response.completed",
        "response.failed",
        "response.incomplete",
    ] {
        let mut wire = fixtures().into_iter().find(|v| v["type"] == tag).unwrap();
        wire["response"]["instructions"] = json!([{"type":"item_reference","id":"i"}]);
        let e: StreamEvent = serde_json::from_value(wire.clone()).unwrap();
        let body = match &e {
            StreamEvent::Created(v) => &v.response,
            StreamEvent::Queued(v) => &v.response,
            StreamEvent::InProgress(v) => &v.response,
            StreamEvent::Completed(v) => &v.response,
            StreamEvent::Failed(v) => &v.response,
            StreamEvent::Incomplete(v) => &v.response,
            _ => panic!(),
        };
        assert!(body.rest.is_empty());
        assert_eq!(serde_json::to_value(e).unwrap(), wire);
        wire["response"]
            .as_object_mut()
            .unwrap()
            .remove("temperature");
        assert!(serde_json::from_value::<StreamEvent>(wire).is_err());
    }
    for tag in ["response.output_item.added", "response.output_item.done"] {
        let mut wire = fixtures().into_iter().find(|v| v["type"] == tag).unwrap();
        let e: StreamEvent = serde_json::from_value(wire.clone()).unwrap();
        let item = match &e {
            StreamEvent::OutputItemAdded(v) | StreamEvent::OutputItemDone(v) => &v.item,
            _ => panic!(),
        };
        match item {
            gproxy_protocol::openai::responses::response::ResponseOutputItem::Compaction(v) => {
                assert!(v.rest.is_empty());
                assert_eq!(v.created_by.as_deref(), Some("a"));
            }
            _ => panic!(),
        };
        wire["item"] = json!({"type":"item_reference","id":"i"});
        assert!(serde_json::from_value::<StreamEvent>(wire).is_err());
    }
}
#[test]
fn audio_minimal_shapes_and_builders_emit_native_wire_tags() {
    let delta = StreamEvent::AudioDelta(AudioDelta::builder(1, "AA==".into()).build());
    assert_eq!(
        serde_json::to_value(delta).unwrap(),
        json!({"type":"response.audio.delta","sequence_number":1,"delta":"AA=="})
    );
    let done = StreamEvent::AudioTranscriptDone(AudioTranscriptDone::builder(2).build());
    assert_eq!(
        serde_json::to_value(done).unwrap(),
        json!({"type":"response.audio.transcript.done","sequence_number":2})
    );
    let text = StreamEvent::OutputTextDelta(
        OutputTextDelta::builder(
            3,
            "i".into(),
            0,
            0,
            "x".into(),
            vec![StreamLogprob::builder("x".into(), serde_json::Number::from(0)).build()],
        )
        .build(),
    );
    let wire = serde_json::to_value(text).unwrap();
    assert_eq!(wire["logprobs"], json!([{"token":"x","logprob":0}]));
    let err = StreamEvent::Error(ResponseErrorEvent::builder(None, "m".into(), None, 4).build());
    assert_eq!(
        serde_json::to_value(err).unwrap(),
        json!({"type":"error","sequence_number":4,"code":null,"message":"m","param":null})
    );
    assert!(
        serde_json::from_value::<StreamEvent>(
            json!({"type":"response.future","sequence_number":1})
        )
        .is_err()
    );
    let wire = json!({"type":"response.audio.done","sequence_number":1,"itemId":"extension"});
    let e: StreamEvent = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(rest(&e)["itemId"], "extension");
    assert_eq!(serde_json::to_value(e).unwrap(), wire);
}
#[test]
fn http_byte_chunks_remain_independent_of_typed_json_event_boundaries() {
    use std::{
        pin::Pin,
        task::{Context, Poll, Waker},
    };
    struct Chunks(std::vec::IntoIter<bytes::Bytes>);
    impl futures_core::Stream for Chunks {
        type Item = Result<bytes::Bytes, gproxy_protocol::connection::TransportError>;
        fn poll_next(mut self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Option<Self::Item>> {
            Poll::Ready(self.0.next().map(Ok))
        }
    }
    let chunks = vec![
        bytes::Bytes::from_static(b"event: response.audio.done\nda"),
        bytes::Bytes::from_static(
            b"ta: {\"type\":\"response.audio.done\",\"sequence_number\":1}\n\n",
        ),
    ];
    let mut response: ResponseStream = ResponseStream {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: Box::pin(Chunks(chunks.clone().into_iter())),
    };
    response.headers.insert(
        "content-type",
        http::HeaderValue::from_static("text/event-stream"),
    );
    assert_eq!(response.status, http::StatusCode::OK);
    assert_eq!(response.headers["content-type"], "text/event-stream");
    let mut cx = Context::from_waker(Waker::noop());
    for expected in chunks {
        let Poll::Ready(Some(Ok(actual))) = response.body.as_mut().poll_next(&mut cx) else {
            panic!()
        };
        assert_eq!(actual, expected);
    }
    assert!(matches!(
        response.body.as_mut().poll_next(&mut cx),
        Poll::Ready(None)
    ));
}

fn output_fixtures() -> Vec<Value> {
    vec![
        json!({"type": "file_search_call", "id": "f", "queries": ["x"], "status": "searching", "results": [{"attributes": {"tag": "a", "rank": 1.5, "active": true}, "file_id": "f", "filename": "a.txt", "score": 0.5, "text": "x"}]}),
        json!({"type": "computer_call", "id": "c", "call_id": "c", "status": "in_progress", "pending_safety_checks": [{"id": "s", "code": null, "message": null}], "action": {"type": "wait"}, "actions": [{"type": "screenshot"}]}),
        json!({"type": "computer_call_output", "call_id": "c", "id": "c", "output": {"type": "computer_screenshot", "file_id": "f", "image_url": "https://x"}, "acknowledged_safety_checks": [{"id": "s", "code": null, "message": null}], "status": "failed", "created_by": "actor"}),
        json!({"type": "web_search_call", "id": "w", "status": "failed", "action": {"type": "search", "query": "x", "queries": ["x"], "sources": [{"type": "url", "url": "https://x"}]}}),
        json!({"type": "function_call", "id": "f", "call_id": "c", "name": "f", "namespace": "ns", "arguments": "{}", "caller": {"type": "program", "caller_id": "p"}, "status": "incomplete"}),
        json!({"type": "function_call_output", "id": "f", "call_id": "c", "output": [{"type": "input_image", "detail": "auto", "file_id": null}], "name": "f", "namespace": "n", "caller": null, "status": "completed", "created_by": "actor"}),
        json!({"type": "tool_search_call", "arguments": {"arbitrary": [true, 1]}, "id": "t", "call_id": null, "execution": "client", "status": "completed", "created_by": "actor"}),
        json!({"type": "tool_search_output", "tools": [], "id": "t", "call_id": null, "execution": "server", "status": "completed", "created_by": "actor"}),
        json!({"type": "additional_tools", "role": "critic", "tools": [], "id": "a"}),
        json!({"type": "reasoning", "id": "r", "summary": [{"type": "summary_text", "text": "summary"}], "content": [{"type": "reasoning_text", "text": "reason"}], "encrypted_content": null, "status": "completed"}),
        json!({"type": "compaction", "encrypted_content": "data", "id": "c", "created_by": "actor"}),
        json!({"type": "image_generation_call", "id": "i", "result": null, "status": "generating"}),
        json!({"type": "code_interpreter_call", "id": "i", "container_id": "c", "code": null, "outputs": null, "status": "interpreting"}),
        json!({"type": "local_shell_call", "id": "l", "call_id": "c", "status": "completed", "action": {"type": "exec", "command": ["pwd"], "env": {"LANG": "C"}, "timeout_ms": null, "user": null, "working_directory": null}}),
        json!({"type": "local_shell_call_output", "id": "l", "output": "x", "status": null}),
        json!({"type": "shell_call", "call_id": "c", "action": {"commands": ["pwd"], "max_output_length": null, "timeout_ms": null}, "id": "s", "caller": null, "environment": {"type": "local"}, "status": "completed", "created_by": "actor"}),
        json!({"type": "shell_call_output", "call_id": "c", "output": [{"stdout": "", "stderr": "", "outcome": {"type": "exit", "exit_code": 0}, "created_by": "actor"}], "id": "s", "caller": null, "max_output_length": null, "status": "completed", "created_by": "actor"}),
        json!({"type": "apply_patch_call", "call_id": "c", "operation": {"type": "delete_file", "path": "a"}, "status": "in_progress", "id": "p", "caller": null, "created_by": "actor"}),
        json!({"type": "apply_patch_call_output", "call_id": "c", "status": "failed", "output": null, "id": "p", "caller": null, "created_by": "actor"}),
        json!({"type": "mcp_list_tools", "id": "m", "server_label": "s", "tools": [{"name": "f", "input_schema": {"type": "object"}, "annotations": null, "description": null}], "error": null}),
        json!({"type": "mcp_approval_request", "id": "m", "arguments": "{}", "name": "f", "server_label": "s"}),
        json!({"type": "mcp_approval_response", "approval_request_id": "m", "approve": false, "id": "m", "reason": null}),
        json!({"type": "mcp_call", "id": "m", "arguments": "{}", "name": "f", "server_label": "s", "approval_request_id": null, "error": null, "output": null, "status": "calling"}),
        json!({"type": "custom_tool_call_output", "call_id": "c", "id": "c", "caller": null, "output": [{"type": "input_image", "detail": "original", "file_id": null, "image_url": null}], "status": "completed", "created_by": "actor"}),
        json!({"type": "custom_tool_call", "call_id": "c", "input": "raw", "name": "f", "id": "i", "caller": null, "namespace": "n"}),
        json!({"type": "program", "id": "p", "call_id": "c", "code": "1", "fingerprint": "fp"}),
        json!({"type": "program_output", "id": "p", "call_id": "c", "result": "1", "status": "incomplete"}),
        json!({"type": "message", "id": "m", "role": "assistant", "status": "completed", "phase": "final_answer", "content": [{"type": "output_text", "text": "answer", "annotations": [], "logprobs": []}]}),
    ]
}

#[test]
fn output_item_events_cover_all_28_output_variants() {
    for item in output_fixtures() {
        for tag in ["response.output_item.added", "response.output_item.done"] {
            let wire = json!({"type":tag,"sequence_number":1,"output_index":0,"item":item});
            let event: StreamEvent = serde_json::from_value(wire.clone()).unwrap();
            assert!(rest(&event).is_empty());
            assert_eq!(serde_json::to_value(event).unwrap(), wire);
        }
    }
}
