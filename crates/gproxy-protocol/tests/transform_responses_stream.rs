use gproxy_protocol::transform::{
    TransformErrorKind,
    generate::stream::responses::*,
    identity::{IdNamespace, IdentityFlow},
};
use gproxy_protocol::wire::{
    DeclaredFields,
    openai::responses::{response as r, stream as s},
};
use serde_json::{Value, json};
fn response(output: Value) -> r::GenerateContentResponseBody {
    serde_json::from_value(json!({"id":"r","created_at":123,"error":null,"incomplete_details":null,"instructions":null,"metadata":null,"model":"m","object":"response","output":output,"parallel_tool_calls":true,"temperature":null,"tool_choice":"auto","tools":[],"top_p":null,"status":"completed"})).unwrap()
}
fn flow() -> IdentityFlow {
    IdentityFlow::new(IdNamespace::with_bytes([3; 16]))
}
fn synth(v: r::GenerateContentResponseBody) -> Vec<s::StreamEvent> {
    synthesize_responses_stream(v, &mut flow(), Default::default())
        .unwrap()
        .value
}
fn collect(
    events: Vec<s::StreamEvent>,
) -> Result<r::GenerateContentResponseBody, gproxy_protocol::transform::TransformError> {
    let mut c = ResponsesStreamCollector::new(Default::default());
    for e in events {
        c.push(e)?;
    }
    Ok(c.finish()?.value)
}
fn text_response() -> r::GenerateContentResponseBody {
    response(
        json!([{"type":"message","id":"msg","role":"assistant","status":"completed","content":[{"type":"output_text","text":"Hello 🌏","annotations":[],"logprobs":[]},{"type":"refusal","refusal":"No"}]}]),
    )
}
fn output_fixtures() -> Vec<Value> {
    vec![
        json!({"type": "file_search_call", "id": "f", "queries": ["x"], "status": "completed", "results": [{"attributes": {"tag": "a", "rank": 1.5, "active": true}, "file_id": "f", "filename": "a.txt", "score": 0.5, "text": "x"}]}),
        json!({"type": "computer_call", "id": "c", "call_id": "c", "status": "completed", "pending_safety_checks": [{"id": "s", "code": null, "message": null}], "action": {"type": "wait"}, "actions": [{"type": "screenshot"}]}),
        json!({"type": "computer_call_output", "call_id": "c", "id": "c", "output": {"type": "computer_screenshot", "file_id": "f", "image_url": "https://x"}, "acknowledged_safety_checks": [{"id": "s", "code": null, "message": null}], "status": "failed", "created_by": "actor"}),
        json!({"type": "web_search_call", "id": "w", "status": "failed", "action": {"type": "search", "query": "x", "queries": ["x"], "sources": [{"type": "url", "url": "https://x"}]}}),
        json!({"type": "function_call", "id": "f", "call_id": "c", "name": "f", "namespace": "ns", "arguments": "{}", "caller": {"type": "program", "caller_id": "p"}, "status": "completed"}),
        json!({"type": "function_call_output", "id": "f", "call_id": "c", "output": [{"type": "input_image", "detail": "auto", "file_id": null}], "name": "f", "namespace": "n", "caller": null, "status": "completed", "created_by": "actor"}),
        json!({"type": "tool_search_call", "arguments": {"arbitrary": [true, 1]}, "id": "t", "call_id": null, "execution": "client", "status": "completed", "created_by": "actor"}),
        json!({"type": "tool_search_output", "tools": [], "id": "t", "call_id": null, "execution": "server", "status": "completed", "created_by": "actor"}),
        json!({"type": "additional_tools", "role": "critic", "tools": [], "id": "a"}),
        json!({"type": "reasoning", "id": "r", "summary": [{"type": "summary_text", "text": "summary"}], "content": [{"type": "reasoning_text", "text": "reason"}], "encrypted_content": null, "status": "completed"}),
        json!({"type": "compaction", "encrypted_content": "data", "id": "c", "created_by": "actor"}),
        json!({"type": "image_generation_call", "id": "i", "result": null, "status": "completed"}),
        json!({"type": "code_interpreter_call", "id": "i", "container_id": "c", "code": null, "outputs": null, "status": "completed"}),
        json!({"type": "local_shell_call", "id": "l", "call_id": "c", "status": "completed", "action": {"type": "exec", "command": ["pwd"], "env": {"LANG": "C"}, "timeout_ms": null, "user": null, "working_directory": null}}),
        json!({"type": "local_shell_call_output", "id": "l", "output": "x", "status": null}),
        json!({"type": "shell_call", "call_id": "c", "action": {"commands": ["pwd"], "max_output_length": null, "timeout_ms": null}, "id": "s", "caller": null, "environment": {"type": "local"}, "status": "completed", "created_by": "actor"}),
        json!({"type": "shell_call_output", "call_id": "c", "output": [{"stdout": "", "stderr": "", "outcome": {"type": "exit", "exit_code": 0}, "created_by": "actor"}], "id": "s", "caller": null, "max_output_length": null, "status": "completed", "created_by": "actor"}),
        json!({"type": "apply_patch_call", "call_id": "c", "operation": {"type": "delete_file", "path": "a"}, "status": "completed", "id": "p", "caller": null, "created_by": "actor"}),
        json!({"type": "apply_patch_call_output", "call_id": "c", "status": "failed", "output": null, "id": "p", "caller": null, "created_by": "actor"}),
        json!({"type": "mcp_list_tools", "id": "m", "server_label": "s", "tools": [{"name": "f", "input_schema": {"type": "object"}, "annotations": null, "description": null}], "error": null}),
        json!({"type": "mcp_approval_request", "id": "m", "arguments": "{}", "name": "f", "server_label": "s"}),
        json!({"type": "mcp_approval_response", "approval_request_id": "m", "approve": false, "id": "m", "reason": null}),
        json!({"type": "mcp_call", "id": "m", "arguments": "{}", "name": "f", "server_label": "s", "approval_request_id": null, "error": null, "output": null, "status": "completed"}),
        json!({"type": "custom_tool_call_output", "call_id": "c", "id": "c", "caller": null, "output": [{"type": "input_image", "detail": "original", "file_id": null, "image_url": null}], "status": "completed", "created_by": "actor"}),
        json!({"type": "custom_tool_call", "call_id": "c", "input": "raw", "name": "f", "id": "i", "caller": null, "namespace": "n"}),
        json!({"type": "program", "id": "p", "call_id": "c", "code": "1", "fingerprint": "fp"}),
        json!({"type": "program_output", "id": "p", "call_id": "c", "result": "1", "status": "completed"}),
        json!({"type": "message", "id": "m", "role": "assistant", "status": "completed", "phase": "final_answer", "content": [{"type": "output_text", "text": "answer", "annotations": [], "logprobs": []}]}),
    ]
}

#[test]
fn all_28_native_output_shapes_roundtrip_declared_contents() {
    let values = output_fixtures();
    assert_eq!(values.len(), 28);
    for mut v in values {
        v["x-rest"] = json!({"poison":true});
        let input = response(json!([v]));
        let expected = input.clone().into_declared();
        let events = synth(input);
        assert_eq!(collect(events).unwrap(), expected);
    }
}
#[test]
fn text_refusal_reasoning_arguments_stream_empty_seeds_then_exact_terminal() {
    let mut input = text_response();
    let other = response(json!([
    {"type":"reasoning","id":"rs","summary":[{"type":"summary_text","text":"Summary"}],"content":[{"type":"reasoning_text","text":"Reason"}],"encrypted_content":"opaque","status":"completed"},
    {"type":"function_call","id":"fc1","call_id":"same-name-call-1","name":"f","arguments":"{\"x\":1}","status":"completed"},
    {"type":"function_call","id":"fc2","call_id":"same-name-call-2","name":"f","arguments":"{\"x\":2}","status":"completed"},
    {"type":"custom_tool_call","id":"ct","call_id":"call3","name":"f","input":"raw text"},
    {"type":"mcp_call","id":"mc","name":"remote","arguments":"{}","server_label":"server","output":"result","status":"completed"},
    {"type":"code_interpreter_call","id":"ci","container_id":"container","code":"print(1)","outputs":[{"type":"logs","logs":"1"}],"status":"completed"}
    ]));
    input.output.extend(other.output);
    let events = synth(input.clone());
    for e in &events {
        match e {
            s::StreamEvent::Created(v) => {
                assert_eq!(v.response.status, Some(r::ResponseStatus::InProgress));
                assert!(v.response.output.is_empty());
                assert!(v.response.usage.is_none());
            }
            s::StreamEvent::ContentPartAdded(v) => {
                let part = serde_json::to_value(&v.part).unwrap();
                assert!(
                    part.get("text")
                        .or_else(|| part.get("refusal"))
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .is_empty()
                );
            }
            _ => {}
        }
    }
    assert_eq!(collect(events).unwrap(), input);
}
#[test]
fn missing_function_and_custom_item_ids_are_not_call_ids_and_retry_stable() {
    let input = response(
        json!([{"type":"function_call","call_id":"call","name":"f","arguments":"{}"},{"type":"custom_tool_call","call_id":"call2","name":"f","input":"raw"}]),
    );
    let mut f = flow();
    let first = synthesize_responses_stream(input.clone(), &mut f, Default::default())
        .unwrap()
        .value;
    let second = synthesize_responses_stream(input, &mut f, Default::default())
        .unwrap()
        .value;
    assert_eq!(first, second);
    let out = serde_json::to_value(collect(first).unwrap()).unwrap();
    assert!(out["output"][0]["id"].as_str().unwrap().starts_with("fc_"));
    assert_ne!(out["output"][0]["id"], out["output"][0]["call_id"]);
    assert_ne!(out["output"][1]["id"], out["output"][1]["call_id"]);
}
#[test]
fn lifecycle_mutations_reject_and_poison() {
    let events = synth(text_response());
    for kind in 0..9 {
        let mut mutated = events.clone();
        match kind {
            0 => {
                mutated.remove(0);
            }
            1 => {
                if let s::StreamEvent::Completed(v) = mutated.last_mut().unwrap() {
                    v.response.id = "other".into();
                }
            }
            2 => {
                if let s::StreamEvent::Completed(v) = mutated.last_mut().unwrap() {
                    v.response.output.clear();
                }
            }
            3 => {
                mutated.retain(|v| !matches!(v, s::StreamEvent::OutputItemDone(_)));
            }
            4 => {
                mutated.retain(|v| !matches!(v, s::StreamEvent::ContentPartDone(_)));
            }
            5 => {
                for v in &mut mutated {
                    if let s::StreamEvent::OutputTextDelta(v) = v {
                        v.output_index = 1;
                    }
                }
            }
            6 => {
                for v in &mut mutated {
                    if let s::StreamEvent::OutputTextDone(v) = v {
                        v.text = "contradiction".into();
                    }
                }
            }
            7 => {
                for v in &mut mutated {
                    if let s::StreamEvent::RefusalDelta(v) = v {
                        v.sequence_number = 0;
                    }
                }
            }
            8 => {
                mutated.push(events.last().unwrap().clone());
            }
            _ => unreachable!(),
        }
        let mut c = ResponsesStreamCollector::new(Default::default());
        let mut failed = false;
        for e in mutated {
            if c.push(e).is_err() {
                failed = true;
                break;
            }
        }
        assert!(failed, "mutation {kind}");
        assert!(c.push(events.last().unwrap().clone()).is_err());
        assert!(c.finish().is_err());
    }
}
#[test]
fn completed_requires_done_items_and_eof_is_not_terminal() {
    let mut events = synth(text_response());
    events.pop();
    assert!(collect(events).is_err());
    for status in [
        None,
        Some(r::ResponseStatus::Queued),
        Some(r::ResponseStatus::InProgress),
        Some(r::ResponseStatus::Failed),
        Some(r::ResponseStatus::Cancelled),
    ] {
        let mut input = text_response();
        input.status = status;
        assert!(synthesize_responses_stream(input, &mut flow(), Default::default()).is_err());
    }
    let mut input = text_response();
    input.status = Some(r::ResponseStatus::Incomplete);
    input.incomplete_details =
        Some(serde_json::from_value(json!({"reason":"max_output_tokens"})).unwrap());
    assert_eq!(collect(synth(input.clone())).unwrap(), input);
}
#[test]
fn limits_apply_to_events_bytes_text_and_json() {
    for limits in [
        ResponsesStreamLimits {
            max_events: 1,
            ..Default::default()
        },
        ResponsesStreamLimits {
            max_bytes: 64,
            ..Default::default()
        },
        ResponsesStreamLimits {
            max_items: 0,
            ..Default::default()
        },
        ResponsesStreamLimits {
            max_text_bytes: 2,
            ..Default::default()
        },
    ] {
        assert_eq!(
            synthesize_responses_stream(text_response(), &mut flow(), limits)
                .unwrap_err()
                .kind(),
            TransformErrorKind::Limit
        );
    }
    let input = response(
        json!([{"type":"function_call","id":"fc","call_id":"call","name":"f","arguments":"{\"x\":1}"}]),
    );
    assert_eq!(
        synthesize_responses_stream(
            input,
            &mut flow(),
            ResponsesStreamLimits {
                max_json_bytes: 2,
                ..Default::default()
            }
        )
        .unwrap_err()
        .kind(),
        TransformErrorKind::Limit
    );
}
#[test]
fn annotation_and_logprob_metadata_preserved_without_extension_bags() {
    let input = response(
        json!([{"type":"message","id":"msg","role":"assistant","status":"completed","x-rest":"drop","content":[{"type":"output_text","text":"A","x-rest":"drop","annotations":[{"type":"url_citation","start_index":0,"end_index":1,"title":"title","url":"https://example.test","x-rest":"drop"}],"logprobs":[{"token":"A","logprob":-0.1,"bytes":[65],"top_logprobs":[{"token":"B","logprob":-1,"bytes":[66]}]}]}]}]),
    );
    let expected = input.clone().into_declared();
    assert_eq!(collect(synth(input)).unwrap(), expected);
}
#[test]
fn tool_id_index_type_and_argument_final_disagreement_reject() {
    let input = response(
        json!([{"type":"function_call","id":"fc","call_id":"call","name":"f","arguments":"{}"}]),
    );
    let events = synth(input);
    for kind in 0..4 {
        let mut e = events.clone();
        for v in &mut e {
            match (kind, v) {
                (0, s::StreamEvent::FunctionCallArgumentsDelta(v)) => v.item_id = "call".into(),
                (1, s::StreamEvent::FunctionCallArgumentsDone(v)) => v.name = "other".into(),
                (2, s::StreamEvent::OutputItemDone(v)) => {
                    if let r::ResponseOutputItem::FunctionCall(v) = &mut v.item {
                        v.call_id = "different".into();
                    }
                }
                (3, s::StreamEvent::Completed(v)) => {
                    if let r::ResponseOutputItem::FunctionCall(v) = &mut v.response.output[0] {
                        v.arguments = "[]".into();
                    }
                }
                _ => {}
            }
        }
        assert!(collect(e).is_err());
    }
}

#[test]
fn progress_events_validate_kind_sequence_and_terminal_status() {
    for (item_type, events) in [
        (
            "image_generation_call",
            vec!["in_progress", "generating", "completed"],
        ),
        (
            "code_interpreter_call",
            vec!["in_progress", "interpreting", "completed"],
        ),
        (
            "file_search_call",
            vec!["in_progress", "searching", "completed"],
        ),
        (
            "web_search_call",
            vec!["in_progress", "searching", "completed"],
        ),
        ("mcp_call", vec!["in_progress", "completed"]),
        ("mcp_list_tools", vec!["in_progress", "completed"]),
    ] {
        let mut item = output_fixtures()
            .into_iter()
            .find(|v| v["type"] == item_type)
            .unwrap();
        if item.get("status").is_some() {
            item["status"] = json!("completed");
        }
        let base = synth(response(json!([item.clone()])));
        for mutation in 0..4 {
            let mut wire: Vec<Value> = base
                .iter()
                .map(|v| serde_json::to_value(v).unwrap())
                .collect();
            let at = wire
                .iter()
                .position(|v| v["type"] == "response.output_item.done")
                .unwrap();
            let extra=events.iter().map(|event|json!({"type":format!("response.{item_type}.{event}"),"sequence_number":0,"output_index":0,"item_id":item["id"]}));
            wire.splice(at..at, extra);
            if mutation == 1 {
                wire[at]["item_id"] = json!("wrong");
            }
            if mutation == 2 {
                let last = wire[at + events.len() - 1].clone();
                wire.insert(at, last);
            }
            if mutation == 3 {
                wire[at]["type"] = json!(if item_type == "file_search_call" {
                    "response.web_search_call.in_progress"
                } else {
                    "response.file_search_call.in_progress"
                });
            }
            for (n, event) in wire.iter_mut().enumerate() {
                event["sequence_number"] = json!(n);
            }
            let result = collect(
                wire.into_iter()
                    .map(|v| serde_json::from_value(v).unwrap())
                    .collect(),
            );
            assert_eq!(
                result.is_ok(),
                mutation == 0,
                "{item_type} mutation {mutation}"
            );
        }
    }
}

#[test]
fn initial_snapshots_cannot_bypass_aggregate_text_limits() {
    let mut c = ResponsesStreamCollector::new(ResponsesStreamLimits {
        max_text_bytes: 3,
        ..Default::default()
    });
    c.push(synth(response(json!([]))).remove(0)).unwrap();
    for (index, id) in ["a", "b"].into_iter().enumerate() {
        let event=serde_json::from_value(json!({"type":"response.output_item.added","sequence_number":index+1,"output_index":index,"item":{"id":id,"type":"message","role":"assistant","status":"in_progress","content":[{"type":"output_text","text":"xx","annotations":[],"logprobs":[]}]}})).unwrap();
        assert_eq!(c.push(event).is_ok(), index == 0);
    }
    assert!(c.finish().is_err());
}

#[test]
fn native_late_optional_item_id_binds_by_index_without_call_id_fallback() {
    let input = response(
        json!([{"type":"function_call","id":"late-fc","call_id":"call","name":"f","arguments":"{}"}]),
    );
    let events = synth(input.clone());
    for delta_id in [true, false] {
        let mut wire: Vec<Value> = events
            .iter()
            .map(|v| serde_json::to_value(v).unwrap())
            .collect();
        for event in &mut wire {
            if event["type"] == "response.output_item.added" {
                event["item"].as_object_mut().unwrap().remove("id");
                if !delta_id {
                    event["item"]["arguments"] = json!("{}");
                }
            }
        }
        if !delta_id {
            wire.retain(|v| {
                !matches!(
                    v["type"].as_str(),
                    Some(
                        "response.function_call_arguments.delta"
                            | "response.function_call_arguments.done"
                    )
                )
            });
        }
        let actual = collect(
            wire.into_iter()
                .map(|v| serde_json::from_value(v).unwrap())
                .collect(),
        )
        .unwrap();
        assert_eq!(actual, input);
    }
}

#[test]
fn populated_creation_and_progress_snapshots_are_verified_not_ignored() {
    let final_response = text_response();
    let mut initial = final_response.clone();
    initial.status = Some(r::ResponseStatus::InProgress);
    initial.usage = None;
    initial.completed_at = None;
    let item = final_response.output[0].clone();
    let events=vec![serde_json::from_value(json!({"type":"response.created","sequence_number":0,"response":initial})).unwrap(),serde_json::from_value(json!({"type":"response.output_item.done","sequence_number":1,"output_index":0,"item":item})).unwrap(),serde_json::from_value(json!({"type":"response.in_progress","sequence_number":2,"response":initial})).unwrap(),serde_json::from_value(json!({"type":"response.completed","sequence_number":3,"response":final_response})).unwrap()];
    assert_eq!(collect(events.clone()).unwrap(), final_response);
    let mut invalid = events;
    let s::StreamEvent::InProgress(progress) = &mut invalid[2] else {
        panic!()
    };
    let r::ResponseOutputItem::Message(message) = &mut progress.response.output[0] else {
        panic!()
    };
    let gproxy_protocol::openai::responses::input::OutputContent::Text(text) =
        &mut message.content[0]
    else {
        panic!()
    };
    text.text = "different snapshot".into();
    assert!(collect(invalid).is_err());
}
#[test]
fn synth_late_failure_does_not_publish_allocated_identities_and_cache_totals_validate() {
    use gproxy_protocol::transform::identity::{IdentityRole, OutputItemKind};
    let mut input = response(
        json!([{"type":"function_call","call_id":"call","name":"f","arguments":"{}","status":"completed"}]),
    );
    let mut ids = flow();
    let limits = ResponsesStreamLimits {
        max_events: 1,
        ..Default::default()
    };
    assert!(synthesize_responses_stream(input.clone(), &mut ids, limits).is_err());
    assert!(
        ids.lookup_logical(
            IdentityRole::OutputItem(OutputItemKind::FunctionCall),
            &gproxy_protocol::Dialect::OpenAi,
            0
        )
        .is_none()
    );
    input.usage=Some(serde_json::from_value(json!({"input_tokens":4,"input_tokens_details":{"cached_tokens":3,"cache_write_tokens":2},"output_tokens":1,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":5})).unwrap());
    assert!(synthesize_responses_stream(input, &mut ids, Default::default()).is_err());
    assert!(
        ids.lookup_logical(
            IdentityRole::OutputItem(OutputItemKind::FunctionCall),
            &gproxy_protocol::Dialect::OpenAi,
            0
        )
        .is_none()
    );
}
#[test]
fn incomplete_summary_and_nonterminal_items_cannot_complete_successfully() {
    let input = response(
        json!([{"type":"reasoning","id":"rs","summary":[{"type":"summary_text","text":"summary"}],"status":"completed"}]),
    );
    let mut events = synth(input);
    let event = events
        .iter_mut()
        .find(|e| matches!(e, s::StreamEvent::ReasoningSummaryPartDone(_)))
        .unwrap();
    let s::StreamEvent::ReasoningSummaryPartDone(v) = event else {
        panic!()
    };
    v.status = Some(s::SummaryPartStatus::Incomplete);
    assert!(collect(events).is_err());
    for item in [
        json!({"type":"message","id":"m","role":"assistant","status":"in_progress","content":[]}),
        json!({"type":"image_generation_call","id":"i","status":"generating","result":null}),
        json!({"type":"code_interpreter_call","id":"ci","container_id":"container","status":"interpreting","code":null,"outputs":null}),
    ] {
        assert!(
            synthesize_responses_stream(response(json!([item])), &mut flow(), Default::default())
                .is_err()
        );
    }
}
#[test]
fn output_item_done_cannot_change_mcp_server_identity() {
    let input = response(
        json!([{"type":"mcp_list_tools","id":"mcp","server_label":"original","tools":[],"error":null}]),
    );
    let mut events = synth(input);
    for event in &mut events {
        if let s::StreamEvent::OutputItemDone(value) = event {
            let r::ResponseOutputItem::McpListTools(item) = &mut value.item else {
                panic!()
            };
            item.server_label = "other-server".into();
        }
    }
    assert!(collect(events).is_err());
}

#[test]
fn seeded_partial_text_and_logprobs_can_continue_with_missing_top_fields() {
    let a = json!({"token":"a","bytes":[97],"logprob":-0.1,"top_logprobs":[{"token":"a","bytes":[97],"logprob":-0.1}]});
    let b = json!({"token":"b","bytes":[98],"logprob":-0.2,"top_logprobs":[]});
    let part = json!({"type":"output_text","text":"ab","annotations":[],"logprobs":[a.clone(),b]});
    let item = json!({"type":"message","id":"m","role":"assistant","status":"completed","content":[part.clone()]});
    let final_response = response(json!([item.clone()]));
    let mut initial = final_response.clone();
    initial.output.clear();
    initial.status = Some(r::ResponseStatus::InProgress);
    initial.usage = None;
    let seed = json!({"type":"message","id":"m","role":"assistant","status":"in_progress","content":[{"type":"output_text","text":"a","annotations":[],"logprobs":[a]}]});
    let values = vec![
        json!({"type":"response.created","sequence_number":0,"response":initial}),
        json!({"type":"response.output_item.added","sequence_number":1,"output_index":0,"item":seed}),
        json!({"type":"response.output_text.delta","sequence_number":2,"item_id":"m","output_index":0,"content_index":0,"delta":"b","logprobs":[{"token":"b","logprob":-0.2}]}),
        json!({"type":"response.output_text.done","sequence_number":3,"item_id":"m","output_index":0,"content_index":0,"text":"ab","logprobs":[{"token":"a","logprob":-0.1},{"token":"b","logprob":-0.2}]}),
        json!({"type":"response.content_part.done","sequence_number":4,"item_id":"m","output_index":0,"content_index":0,"part":part}),
        json!({"type":"response.output_item.done","sequence_number":5,"output_index":0,"item":item}),
        json!({"type":"response.completed","sequence_number":6,"response":final_response}),
    ];
    let events = values
        .into_iter()
        .map(|v| serde_json::from_value(v).unwrap())
        .collect();
    assert_eq!(collect(events).unwrap(), final_response);
}
#[test]
fn malformed_known_annotation_is_not_silently_treated_as_unknown_extension() {
    let mut events = synth(text_response());
    let position = events
        .iter()
        .position(|e| matches!(e, s::StreamEvent::OutputTextDone(_)))
        .unwrap();
    let original = events[position].clone();
    let s::StreamEvent::OutputTextDone(done) = original else {
        panic!()
    };
    let inserted: s::StreamEvent=serde_json::from_value(json!({"type":"response.output_text.annotation.added","sequence_number":0,"item_id":done.item_id,"output_index":done.output_index,"content_index":done.content_index,"annotation_index":0,"annotation":{"type":"url_citation","url":"https://example"}})).unwrap();
    events.insert(position, inserted);
    let events = events
        .into_iter()
        .enumerate()
        .map(|(index, e)| {
            let mut v = serde_json::to_value(e).unwrap();
            v["sequence_number"] = json!(index);
            serde_json::from_value(v).unwrap()
        })
        .collect();
    assert!(collect(events).is_err());
}

#[test]
fn server_arguments_cannot_grow_past_aggregate_cap_only_at_done() {
    let mut empty = response(json!([]));
    empty.status = Some(r::ResponseStatus::InProgress);
    empty.usage = None;
    let mut c = ResponsesStreamCollector::new(ResponsesStreamLimits {
        max_json_bytes: 5,
        ..Default::default()
    });
    c.push(
        serde_json::from_value(
            json!({"type":"response.created","sequence_number":0,"response":empty}),
        )
        .unwrap(),
    )
    .unwrap();
    for (index, id) in ["a", "b"].iter().enumerate() {
        let seed = json!({"type":"code_interpreter_call","id":id,"container_id":"container","status":"in_progress","code":null,"outputs":null});
        c.push(serde_json::from_value(json!({"type":"response.output_item.added","sequence_number":index*2+1,"output_index":index,"item":seed})).unwrap()).unwrap();
        let item = json!({"type":"code_interpreter_call","id":id,"container_id":"container","status":"completed","code":"123","outputs":null});
        let result=c.push(serde_json::from_value(json!({"type":"response.output_item.done","sequence_number":index*2+2,"output_index":index,"item":item})).unwrap());
        assert_eq!(result.is_err(), index == 1);
    }
    assert!(c.finish().is_err());
}
#[test]
fn native_error_keeps_provider_code_parameter_and_message() {
    let mut c = ResponsesStreamCollector::new(Default::default());
    let error=c.push(serde_json::from_value(json!({"type":"error","sequence_number":0,"code":"invalid_request","param":"tools","message":"bad tool definition"})).unwrap()).unwrap_err();
    assert!(error.to_string().contains("bad tool definition"));
    assert!(error.to_string().contains("tools"));
    assert!(c.finish().is_err());
}
