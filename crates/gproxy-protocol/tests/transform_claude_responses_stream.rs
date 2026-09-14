use gproxy_protocol::{
    Dialect,
    transform::{
        TransformErrorKind,
        generate::{
            claude_responses::{
                self as pair, ClaudeRequestContext, ClaudeResponseContext, RestoredClaudeThinking,
                stream::*,
            },
            stream::{
                claude::ClaudeStreamCollector,
                responses::{ResponsesStreamCollector, synthesize_responses_stream},
            },
        },
        identity::{
            IdNamespace, IdentityFlow, IdentityRole, IdentityStateRecord, IdentityTarget,
            KnownIdPrefix, OpaqueField, OpaqueSignature, OutputItemKind, TargetIdPolicy,
        },
    },
    wire::{
        claude::{generate_content as c, stream as cs},
        openai::responses::{input as i, response as r, stream as rs},
    },
};
use serde_json::{Value, json};
fn flow() -> IdentityFlow {
    IdentityFlow::new(IdNamespace::with_bytes([51; 16]))
}
fn rp() -> TargetIdPolicy {
    TargetIdPolicy::new(Dialect::OpenAi)
}
fn cp() -> TargetIdPolicy {
    TargetIdPolicy::new(Dialect::Claude)
        .with_generated_prefix(IdentityRole::Response, KnownIdPrefix::Message)
        .with_generated_prefix(IdentityRole::ToolCall, KnownIdPrefix::Tool)
}
fn ce(value: Value) -> cs::StreamEvent {
    serde_json::from_value(value).unwrap()
}
fn re(value: Value) -> rs::StreamEvent {
    serde_json::from_value(value).unwrap()
}
fn initial() -> c::Usage {
    serde_json::from_value(json!({"input_tokens":3,"output_tokens":0,"cache_creation_input_tokens":2,"cache_read_input_tokens":4,"output_tokens_details":{"thinking_tokens":0}})).unwrap()
}
fn context() -> ClaudeResponseContext {
    ClaudeResponseContext{request:serde_json::from_value(json!({"model":"requested-alias","input":"source task","instructions":"original policy","parallel_tool_calls":false,"tool_choice":"auto","metadata":{"caller":"original"},"temperature":0.3})).unwrap(),effective_parallel_tool_calls:false,effective_tool_choice:i::ToolChoice::Mode(i::ToolChoiceMode::Auto),usage:Default::default(),created_at:123,effective_prompt_cache_options:None}
}
fn rc() -> ResponsesToClaudeContext {
    ResponsesToClaudeContext {
        usage: Some(initial()),
        ..Default::default()
    }
}
fn cstart() -> cs::StreamEvent {
    ce(
        json!({"type":"message_start","message":{"type":"message","id":"claude-source","model":"actual-model","role":"assistant","content":[],"usage":initial()}}),
    )
}
fn cblock(index: i64, value: Value) -> cs::StreamEvent {
    ce(json!({"type":"content_block_start","index":index,"content_block":value}))
}
fn cdelta(index: i64, value: Value) -> cs::StreamEvent {
    ce(json!({"type":"content_block_delta","index":index,"delta":value}))
}
fn cstop(index: i64) -> cs::StreamEvent {
    ce(json!({"type":"content_block_stop","index":index}))
}
fn cend(stop: &str, thinking: Option<i64>) -> Vec<cs::StreamEvent> {
    let mut usage = json!({"output_tokens":5});
    if let Some(n) = thinking {
        usage["output_tokens_details"] = json!({"thinking_tokens":n});
    }
    vec![
        ce(json!({"type":"message_delta","delta":{"stop_reason":stop},"usage":usage})),
        ce(json!({"type":"message_stop"})),
    ]
}
fn cevents(blocks: Vec<Value>, stop: &str) -> Vec<cs::StreamEvent> {
    let mut out = vec![cstart()];
    for (index, block) in blocks.into_iter().enumerate() {
        out.extend([cblock(index as i64, block), cstop(index as i64)]);
    }
    out.extend(cend(stop, Some(1)));
    out
}
fn response(output: Value, status: &str, reason: Option<&str>) -> r::GenerateContentResponseBody {
    serde_json::from_value(json!({"id":"response-source","object":"response","created_at":123,"model":"actual-model","output":output,"status":status,"error":null,"incomplete_details":reason.map(|reason|json!({"reason":reason})),"instructions":null,"metadata":null,"temperature":null,"top_p":null,"parallel_tool_calls":false,"tool_choice":"auto","tools":[],"usage":{"input_tokens":9,"input_tokens_details":{"cache_write_tokens":2,"cached_tokens":4},"output_tokens":5,"output_tokens_details":{"reasoning_tokens":1},"total_tokens":14}})).unwrap()
}
fn message(id: &str, parts: Value, status: &str) -> Value {
    json!({"type":"message","id":id,"role":"assistant","status":status,"content":parts})
}
fn text(value: &str) -> Value {
    json!({"type":"output_text","text":value,"annotations":[],"logprobs":[]})
}
fn function(id: &str, call: &str, args: &str, status: &str) -> Value {
    json!({"type":"function_call","id":id,"call_id":call,"name":"same","arguments":args,"status":status})
}
fn r_events(body: r::GenerateContentResponseBody) -> Vec<rs::StreamEvent> {
    synthesize_responses_stream(body, &mut flow(), Default::default())
        .unwrap()
        .value
}
fn collect_c(events: Vec<cs::StreamEvent>) -> c::GenerateContentResponseBody {
    let mut native = ClaudeStreamCollector::new(Default::default());
    for event in events {
        native.push(event).unwrap();
    }
    native.finish().unwrap().value
}
fn collect_r(events: Vec<rs::StreamEvent>) -> r::GenerateContentResponseBody {
    let mut native = ResponsesStreamCollector::new(Default::default());
    for event in events {
        native.push(event).unwrap();
    }
    native.finish().unwrap().value
}
fn normalize(mut body: r::GenerateContentResponseBody) -> Value {
    for item in &mut body.output {
        let args = match item {
            r::ResponseOutputItem::FunctionCall(v) => &mut v.arguments,
            r::ResponseOutputItem::McpCall(v) => &mut v.arguments,
            _ => continue,
        };
        let parsed: Value = serde_json::from_str(args).unwrap();
        *args = serde_json::to_string(&parsed).unwrap();
    }
    serde_json::to_value(body).unwrap()
}
fn c_to_r(events: Vec<cs::StreamEvent>) -> r::GenerateContentResponseBody {
    let source = collect_c(events.clone());
    let mut stream = ClaudeToResponsesStream::new(context(), flow(), Default::default()).unwrap();
    let mut output = Vec::new();
    for event in events {
        output.extend(stream.push(event).unwrap().value);
    }
    let end = stream.finish().unwrap();
    output.extend(end.chunks);
    let actual = collect_r(output);
    let expected =
        pair::claude_to_responses_response(source, context(), &mut end.identities.clone(), &rp())
            .unwrap()
            .value;
    assert_eq!(normalize(actual.clone()), normalize(expected));
    actual
}
fn r_to_c(events: Vec<rs::StreamEvent>) -> c::GenerateContentResponseBody {
    let source = collect_r(events.clone());
    let mut stream = ResponsesToClaudeStream::new(rc(), flow(), Default::default()).unwrap();
    let mut output = Vec::new();
    for event in events {
        output.extend(stream.push(event).unwrap().value);
    }
    let end = stream.finish().unwrap();
    output.extend(end.chunks);
    let actual = collect_c(output);
    let expected = pair::responses_to_claude_response(source, &mut end.identities.clone(), &cp())
        .unwrap()
        .value;
    assert_eq!(actual, expected);
    actual
}
fn renumber(events: &mut [rs::StreamEvent]) {
    for (index, event) in events.iter_mut().enumerate() {
        let mut value = serde_json::to_value(&*event).unwrap();
        value["sequence_number"] = json!(index);
        *event = serde_json::from_value(value).unwrap();
    }
}
#[test]
fn claude_initial_text_thinking_and_parallel_json_are_live_and_canonical() {
    let mut events = vec![
        cstart(),
        cblock(0, json!({"type":"text","text":"initial"})),
        cblock(
            1,
            json!({"type":"thinking","thinking":"why","signature":"private"}),
        ),
        cblock(
            2,
            json!({"type":"tool_use","id":"call-a","name":"same","input":{},"caller":{"type":"direct"}}),
        ),
        cblock(
            3,
            json!({"type":"tool_use","id":"call-b","name":"same","input":{"b":2}}),
        ),
        cdelta(
            2,
            json!({"type":"input_json_delta","partial_json":"{ \"a\":"}),
        ),
        cdelta(2, json!({"type":"input_json_delta","partial_json":"1 }"})),
        cstop(2),
        cstop(3),
        cdelta(0, json!({"type":"text_delta","text":" tail"})),
        cstop(0),
        cdelta(1, json!({"type":"thinking_delta","thinking":" because"})),
        cdelta(
            1,
            json!({"type":"signature_delta","signature":"private-final"}),
        ),
        cstop(1),
    ];
    events.extend(cend("tool_use", Some(1)));
    let mut probe = ClaudeToResponsesStream::new(context(), flow(), Default::default()).unwrap();
    probe.push(events[0].clone()).unwrap();
    let early = probe.push(events[1].clone()).unwrap().value;
    assert!(
        early
            .iter()
            .any(|v| matches!(v,rs::StreamEvent::OutputTextDelta(v) if v.delta=="initial"))
    );
    let actual = c_to_r(events);
    assert_eq!(actual.output.len(), 4);
    let r::ResponseOutputItem::FunctionCall(call) = &actual.output[2] else {
        unreachable!()
    };
    assert_ne!(call.id.as_deref(), Some(call.call_id.as_str()));
    assert_eq!(
        serde_json::from_str::<Value>(&call.arguments).unwrap(),
        json!({"a":1})
    );
    assert!(!serde_json::to_string(&actual).unwrap().contains("private"));
}
#[test]
fn empty_claude_text_and_redacted_reasoning_preserve_native_item_shape() {
    let actual = c_to_r(cevents(
        vec![
            json!({"type":"redacted_thinking","data":"opaque"}),
            json!({"type":"text","text":""}),
        ],
        "end_turn",
    ));
    assert_eq!(actual.output.len(), 1);
    let r::ResponseOutputItem::Message(message) = &actual.output[0] else {
        unreachable!()
    };
    assert_eq!(message.content.len(), 1);
}
#[test]
fn late_claude_refusal_preserves_text_and_honest_filter_terminal_without_replaying_it() {
    let source = cevents(vec![json!({"type":"text","text":"No"})], "refusal");
    let mut stream = ClaudeToResponsesStream::new(context(), flow(), Default::default()).unwrap();
    let mut output = Vec::new();
    for event in source {
        output.extend(stream.push(event).unwrap().value);
    }
    assert!(
        output
            .iter()
            .any(|v| matches!(v,rs::StreamEvent::OutputTextDelta(v) if v.delta=="No"))
    );
    let end = stream.finish().unwrap();
    assert!(
        end.report
            .diagnostics
            .iter()
            .any(|v| v.field == "stop_reason")
    );
    output.extend(end.chunks);
    assert!(!output.iter().any(|v| matches!(
        v,
        rs::StreamEvent::RefusalDelta(_) | rs::StreamEvent::RefusalDone(_)
    )));
    let actual = collect_r(output);
    assert_eq!(actual.status, Some(r::ResponseStatus::Incomplete));
    assert_eq!(
        actual.incomplete_details.unwrap().reason,
        Some(r::ResponseIncompleteReason::ContentFilter)
    );
}
#[test]
fn claude_mcp_calls_and_results_remain_native_mcp_receipts() {
    for error in [false, true] {
        let actual = c_to_r(cevents(
            vec![
                json!({"type":"mcp_tool_use","id":"mcp-source","name":"query","server_name":"server","input":{"q":"value"}}),
                json!({"type":"mcp_tool_result","tool_use_id":"mcp-source","is_error":error,"content":"result"}),
                json!({"type":"text","text":"after"}),
            ],
            "end_turn",
        ));
        let r::ResponseOutputItem::McpCall(call) = &actual.output[0] else {
            panic!("not a client function")
        };
        assert_eq!(call.server_label, "server");
        assert_eq!(
            call.status,
            Some(if error {
                i::McpCallStatus::Failed
            } else {
                i::McpCallStatus::Completed
            })
        );
    }
}
#[test]
fn explicit_source_bound_annotations_preserve_real_coordinates_and_reject_foreign_facts() {
    let citation: c::ResponseTextCitation=serde_json::from_value(json!({"type":"web_search_result_location","url":"https://source.test","title":"Title","cited_text":"quoted source","encrypted_index":"private-index"})).unwrap();
    let target:i::OutputAnnotation=serde_json::from_value(json!({"type":"url_citation","url":"https://source.test","title":"Title","start_index":0,"end_index":6})).unwrap();
    let events = cevents(
        vec![json!({"type":"text","text":"answer","citations":[citation]})],
        "end_turn",
    );
    for mismatch in [false, true] {
        let mut source = citation.clone();
        if mismatch {
            let c::ResponseTextCitation::Web(ref mut value) = source else {
                unreachable!()
            };
            value.url = "https://other.test".into();
        }
        let ctx = ClaudeToResponsesContext {
            response: context(),
            annotations: vec![BoundResponseAnnotation {
                source_block_index: 0,
                source_citation_index: 0,
                source,
                target: target.clone(),
            }],
        };
        let mut stream = ClaudeToResponsesStream::new(ctx, flow(), Default::default()).unwrap();
        let mut output = Vec::new();
        for event in events.clone() {
            output.extend(stream.push(event).unwrap().value);
        }
        if mismatch {
            assert!(stream.finish().is_err());
        } else {
            output.extend(stream.finish().unwrap().chunks);
            let actual = collect_r(output);
            let r::ResponseOutputItem::Message(message) = &actual.output[0] else {
                unreachable!()
            };
            let i::OutputContent::Text(text) = &message.content[0] else {
                unreachable!()
            };
            assert_eq!(text.annotations, vec![target.clone()]);
            assert!(
                !serde_json::to_string(&actual)
                    .unwrap()
                    .contains("private-index")
            );
        }
    }
}
#[test]
fn initial_reasoning_is_not_frozen_as_final_and_missing_counters_are_not_zero() {
    for fact in [None, Some(2)] {
        let mut ctx = context();
        ctx.usage.reasoning_tokens = fact;
        let mut stream = ClaudeToResponsesStream::new(ctx, flow(), Default::default()).unwrap();
        let mut events = vec![
            cstart(),
            cblock(0, json!({"type":"text","text":"x"})),
            cstop(0),
        ];
        events.extend(cend("end_turn", None));
        let mut output = Vec::new();
        for event in events {
            output.extend(stream.push(event).unwrap().value);
        }
        if fact.is_none() {
            assert_eq!(
                stream.finish().unwrap_err().kind(),
                TransformErrorKind::MissingMetadata
            );
        } else {
            output.extend(stream.finish().unwrap().chunks);
            assert_eq!(
                collect_r(output)
                    .usage
                    .unwrap()
                    .unwrap()
                    .output_tokens_details
                    .reasoning_tokens,
                2
            );
        }
    }
}
#[test]
fn responses_refusal_parts_and_two_same_name_functions_are_all_preserved() {
    let source = response(
        json!([
            message(
                "m",
                json!([text("first"),{"type":"refusal","refusal":"No"},text("last")]),
                "completed"
            ),
            function("fc-a", "call-a", "{\"x\":1}", "completed"),
            function("fc-b", "call-b", "{\"x\":2}", "completed")
        ]),
        "completed",
        None,
    );
    let actual = r_to_c(r_events(source));
    assert_eq!(actual.content.len(), 5);
    assert_eq!(actual.stop_reason, c::StopReason::ToolUse);
    let c::ResponseContentBlock::Text(text) = &actual.content[1] else {
        unreachable!()
    };
    assert_eq!(text.text, "No");
}
#[test]
fn created_seed_and_progress_snapshot_do_not_duplicate_content() {
    let final_body = response(
        json!([message("m", json!([text("seed-tail")]), "completed")]),
        "completed",
        None,
    );
    let mut events = r_events(final_body);
    let seed: r::ResponseOutputItem =
        serde_json::from_value(message("m", json!([text("seed")]), "in_progress")).unwrap();
    let rs::StreamEvent::Created(created) = &mut events[0] else {
        unreachable!()
    };
    created.response.output = vec![seed];
    let mut progress = created.response.clone();
    let r::ResponseOutputItem::Message(message) = &mut progress.output[0] else {
        unreachable!()
    };
    let i::OutputContent::Text(part) = &mut message.content[0] else {
        unreachable!()
    };
    part.text = "seed-tail".into();
    events.retain(|v| {
        !matches!(
            v,
            rs::StreamEvent::OutputItemAdded(_) | rs::StreamEvent::ContentPartAdded(_)
        )
    });
    let position = events
        .iter()
        .position(|v| matches!(v, rs::StreamEvent::OutputTextDelta(_)))
        .unwrap();
    let rs::StreamEvent::OutputTextDelta(delta) = &mut events[position] else {
        unreachable!()
    };
    delta.delta = "-tail".into();
    events.insert(
        position + 1,
        rs::StreamEvent::InProgress(rs::ResponseInProgress::builder(0, progress).build()),
    );
    renumber(&mut events);
    let actual = r_to_c(events);
    let c::ResponseContentBlock::Text(text) = &actual.content[0] else {
        unreachable!()
    };
    assert_eq!(text.text, "seed-tail");
}
#[test]
fn partial_seeded_arguments_and_late_item_ids_do_not_change_call_identity() {
    let mut events = r_events(response(
        json!([function("fc", "call-native", "{\"x\":1}", "completed")]),
        "completed",
        None,
    ));
    for event in &mut events {
        match event {
            rs::StreamEvent::OutputItemAdded(v) => {
                if let r::ResponseOutputItem::FunctionCall(call) = &mut v.item {
                    call.id = None;
                    call.arguments = "{\"x\":".into();
                }
            }
            rs::StreamEvent::FunctionCallArgumentsDelta(v) => v.delta = "1}".into(),
            _ => {}
        }
    }
    let actual = r_to_c(events);
    let c::ResponseContentBlock::ToolUse(tool) = &actual.content[0] else {
        unreachable!()
    };
    assert_eq!(tool.id, "call-native");
    assert_eq!(tool.input["x"], 1);
}
#[test]
fn interleaved_response_items_wait_for_unknown_earlier_parts() {
    let final_body = response(
        json!([
            message(
                "m0",
                json!([text("ac"),{"type":"refusal","refusal":"No!"}]),
                "completed"
            ),
            message("m1", json!([text("b")]), "completed"),
            function("fc", "call", "{}", "completed")
        ]),
        "completed",
        None,
    );
    let native = r_events(final_body.clone());
    let mut events = vec![native[0].clone()];
    let mut values = vec![
        json!({"type":"response.output_item.added","output_index":0,"item":message("m0",json!([]),"in_progress")}),
        json!({"type":"response.content_part.added","item_id":"m0","output_index":0,"content_index":0,"part":text("")}),
        json!({"type":"response.output_text.delta","item_id":"m0","output_index":0,"content_index":0,"delta":"a","logprobs":[]}),
        json!({"type":"response.output_item.added","output_index":1,"item":message("m1",json!([]),"in_progress")}),
        json!({"type":"response.content_part.added","item_id":"m1","output_index":1,"content_index":0,"part":text("b")}),
        json!({"type":"response.output_text.done","item_id":"m1","output_index":1,"content_index":0,"text":"b","logprobs":[]}),
        json!({"type":"response.content_part.done","item_id":"m1","output_index":1,"content_index":0,"part":text("b")}),
        json!({"type":"response.output_item.done","output_index":1,"item":message("m1",json!([text("b")]),"completed")}),
        json!({"type":"response.output_item.added","output_index":2,"item":function("fc","call","{}","in_progress")}),
        json!({"type":"response.output_item.done","output_index":2,"item":function("fc","call","{}","completed")}),
        json!({"type":"response.output_text.delta","item_id":"m0","output_index":0,"content_index":0,"delta":"c","logprobs":[]}),
        json!({"type":"response.content_part.added","item_id":"m0","output_index":0,"content_index":1,"part":{"type":"refusal","refusal":"No"}}),
        json!({"type":"response.refusal.delta","item_id":"m0","output_index":0,"content_index":1,"delta":"!"}),
        json!({"type":"response.output_text.done","item_id":"m0","output_index":0,"content_index":0,"text":"ac","logprobs":[]}),
        json!({"type":"response.content_part.done","item_id":"m0","output_index":0,"content_index":0,"part":text("ac")}),
        json!({"type":"response.refusal.done","item_id":"m0","output_index":0,"content_index":1,"refusal":"No!"}),
        json!({"type":"response.content_part.done","item_id":"m0","output_index":0,"content_index":1,"part":{"type":"refusal","refusal":"No!"}}),
        json!({"type":"response.output_item.done","output_index":0,"item":message("m0",json!([text("ac"),{"type":"refusal","refusal":"No!"}]),"completed")}),
    ];
    for value in &mut values {
        value["sequence_number"] = json!(0);
        events.push(re(value.clone()));
    }
    events.push(native.last().unwrap().clone());
    renumber(&mut events);
    let actual = r_to_c(events);
    assert_eq!(actual.content.len(), 4);
    let c::ResponseContentBlock::Text(value) = &actual.content[2] else {
        unreachable!()
    };
    assert_eq!(value.text, "b");
}
fn restoration(field: OpaqueField) -> ClaudeRequestContext {
    let target = IdentityTarget::new("actual-model", Dialect::Claude)
        .unwrap()
        .with_origin("provider/user")
        .unwrap();
    let mut state = IdentityStateRecord::new(
        IdentityRole::OutputItem(OutputItemKind::Reasoning),
        target.clone(),
    );
    state.client_item_id = Some("rs-native".into());
    state.opaque_signature = Some(
        OpaqueSignature::new(field, "native-signature", "provider/user", "actual-model").unwrap(),
    );
    ClaudeRequestContext{target:Some(target),restored_thinking:std::collections::BTreeMap::from([("rs-native".into(),RestoredClaudeThinking{state,block:serde_json::from_value(json!({"type":"thinking","thinking":"signed text","signature":"native-signature","x-extra":"DROP"})).unwrap()})])}
}
#[test]
fn original_bound_reasoning_restores_native_signature_and_wrong_field_fails() {
    let body = response(
        json!([{"type":"reasoning","id":"rs-native","summary":[],"content":[{"type":"reasoning_text","text":"signed text"}],"status":"completed"},message("m",json!([text("answer")]),"completed")]),
        "completed",
        None,
    );
    let events = r_events(body.clone());
    for wrong in [false, true] {
        let field = if wrong {
            OpaqueField::ClaudeRedactedThinkingData
        } else {
            OpaqueField::ClaudeThinkingSignature
        };
        let mut stream = ResponsesToClaudeStream::new(
            ResponsesToClaudeContext {
                usage: Some(initial()),
                restoration: Some(restoration(field)),
            },
            flow(),
            Default::default(),
        )
        .unwrap();
        let mut output = Vec::new();
        let mut failed = false;
        for event in events.clone() {
            match stream.push(event) {
                Ok(v) => output.extend(v.value),
                Err(_) => {
                    failed = true;
                    break;
                }
            }
        }
        if wrong {
            assert!(failed);
            assert!(stream.finish().is_err());
        } else {
            assert!(!failed);
            let end = stream.finish().unwrap();
            output.extend(end.chunks);
            let actual = collect_c(output);
            let expected = pair::responses_to_claude_response_with_context(
                body.clone(),
                restoration(field),
                &mut end.identities.clone(),
                &cp(),
            )
            .unwrap()
            .value;
            assert_eq!(actual, expected);
            let c::ResponseContentBlock::Thinking(block) = &actual.content[0] else {
                unreachable!()
            };
            assert_eq!(block.signature, "native-signature");
        }
    }
}
#[test]
fn mcp_native_result_is_not_a_client_tool_or_an_untyped_text_wrapper() {
    let body = response(
        json!([{"type":"mcp_call","id":"mcp-id","name":"query","server_label":"server","arguments":"{\"x\":1}","output":"result","status":"completed"}]),
        "completed",
        None,
    );
    let actual = r_to_c(r_events(body));
    assert!(matches!(
        actual.content[0],
        c::ResponseContentBlock::McpToolUse(_)
    ));
    assert!(matches!(
        actual.content[1],
        c::ResponseContentBlock::McpToolResult(_)
    ));
    assert_eq!(actual.stop_reason, c::StopReason::EndTurn);
}
#[test]
fn incomplete_causes_are_preserved_and_initial_usage_does_not_replace_missing_final_usage() {
    for (reason, stop) in [
        ("max_output_tokens", c::StopReason::MaxTokens),
        ("content_filter", c::StopReason::Refusal),
    ] {
        let actual = r_to_c(r_events(response(
            json!([message("m", json!([text("partial")]), "incomplete")]),
            "incomplete",
            Some(reason),
        )));
        assert_eq!(actual.stop_reason, stop);
    }
    let mut body = response(
        json!([message("m", json!([text("x")]), "completed")]),
        "completed",
        None,
    );
    body.usage = Some(None);
    let events = r_events(body);
    let mut stream = ResponsesToClaudeStream::new(rc(), flow(), Default::default()).unwrap();
    for event in events {
        stream.push(event).unwrap();
    }
    assert_eq!(
        stream.finish().unwrap_err().kind(),
        TransformErrorKind::MissingMetadata
    );
}
#[test]
fn actual_usage_unlocks_pending_events_and_preserves_final_reasoning_growth() {
    let events = r_events(response(
        json!([message("m", json!([text("held")]), "completed")]),
        "completed",
        None,
    ));
    let mut stream =
        ResponsesToClaudeStream::new(Default::default(), flow(), Default::default()).unwrap();
    let mut output = Vec::new();
    for event in events {
        let terminal = matches!(event, rs::StreamEvent::Completed(_));
        let result = stream.push(event).unwrap();
        if !terminal {
            assert!(result.value.is_empty());
        }
        output.extend(result.value);
    }
    output.extend(stream.finish().unwrap().chunks);
    let actual = collect_c(output);
    assert_eq!(actual.usage.output_tokens, 5);
    assert_eq!(
        actual
            .usage
            .output_tokens_details
            .unwrap()
            .unwrap()
            .thinking_tokens,
        1
    );
}
#[test]
fn wrong_item_associations_unsupported_payloads_and_native_errors_poison() {
    let mut events = r_events(response(
        json!([function("fc", "call", "{}", "completed")]),
        "completed",
        None,
    ));
    for event in &mut events {
        if let rs::StreamEvent::FunctionCallArgumentsDelta(v) = event {
            v.item_id = "call".into();
        }
    }
    let mut stream = ResponsesToClaudeStream::new(rc(), flow(), Default::default()).unwrap();
    let mut failed = false;
    for event in events {
        if stream.push(event).is_err() {
            failed = true;
            break;
        }
    }
    assert!(failed);
    assert!(stream.finish().is_err());
    let mut stream = ResponsesToClaudeStream::new(rc(), flow(), Default::default()).unwrap();
    let error=stream.push(re(json!({"type":"error","sequence_number":0,"code":"native_code","param":"tools[0]","message":"native reason"}))).unwrap_err();
    assert!(error.detail().contains("native_code"));
    assert!(error.detail().contains("native reason"));
    assert!(stream.finish().is_err());
    let mut stream = ClaudeToResponsesStream::new(context(), flow(), Default::default()).unwrap();
    stream.push(cstart()).unwrap();
    assert!(
        stream
            .push(cblock(
                0,
                json!({"type":"server_tool_use","id":"server","name":"web_search","input":{}})
            ))
            .is_err()
    );
    assert!(stream.push(cstop(0)).is_err());
    assert!(stream.finish().is_err());
}
#[test]
fn context_bounds_rest_cleanup_and_pending_amplification_are_enforced() {
    let mut ctx = context();
    ctx.created_at = -1;
    assert!(ClaudeToResponsesStream::new(ctx, flow(), Default::default()).is_err());
    let mut ctx = context();
    ctx.effective_parallel_tool_calls = true;
    assert!(ClaudeToResponsesStream::new(ctx, flow(), Default::default()).is_err());
    let mut ctx = context();
    ctx.request
        .rest
        .insert("x-extra".into(), json!("X".repeat(20000)));
    let mut stream = ClaudeToResponsesStream::new(
        ctx,
        flow(),
        StreamLimits {
            max_bytes: 8192,
            ..Default::default()
        },
    )
    .unwrap();
    let mut events = cevents(
        vec![
            json!({"type":"tool_use","id":"c","name":"f","input":{"formal_extension":7},"x-extra":"DROP"}),
        ],
        "tool_use",
    );
    if let cs::StreamEvent::MessageStart(v) = &mut events[0] {
        v.rest.insert("x-extra".into(), json!("X".repeat(20000)));
    }
    let mut output = Vec::new();
    for event in events {
        output.extend(stream.push(event).unwrap().value);
    }
    output.extend(stream.finish().unwrap().chunks);
    let body = serde_json::to_string(&collect_r(output)).unwrap();
    assert!(body.contains("formal_extension"));
    assert!(!body.contains("DROP"));
    assert!(!body.contains("x-extra"));
    let limits = StreamLimits {
        max_pending: 1,
        ..Default::default()
    };
    let mut stream = ResponsesToClaudeStream::new(Default::default(), flow(), limits).unwrap();
    let mut failed = false;
    for event in r_events(response(
        json!([message("m", json!([text("text")]), "completed")]),
        "completed",
        None,
    )) {
        if stream.push(event).is_err() {
            failed = true;
            break;
        }
    }
    assert!(failed);
    assert!(stream.finish().is_err());
}

#[test]
fn parallel_response_tools_start_before_either_argument_stream_finishes() {
    let body = response(
        json!([
            function("fc0", "call0", "{\"x\":1}", "completed"),
            function("fc1", "call1", "{\"x\":2}", "completed")
        ]),
        "completed",
        None,
    );
    let native = r_events(body.clone());
    let mut events = vec![native[0].clone()];
    let values = vec![
        json!({"type":"response.output_item.added","output_index":0,"item":function("fc0","call0","","in_progress")}),
        json!({"type":"response.output_item.added","output_index":1,"item":function("fc1","call1","","in_progress")}),
        json!({"type":"response.function_call_arguments.delta","item_id":"fc0","output_index":0,"delta":"{\"x\":"}),
        json!({"type":"response.function_call_arguments.delta","item_id":"fc1","output_index":1,"delta":"{\"x\":"}),
        json!({"type":"response.function_call_arguments.delta","item_id":"fc1","output_index":1,"delta":"2}"}),
        json!({"type":"response.function_call_arguments.done","item_id":"fc1","output_index":1,"name":"same","arguments":"{\"x\":2}"}),
        json!({"type":"response.output_item.done","output_index":1,"item":function("fc1","call1","{\"x\":2}","completed")}),
        json!({"type":"response.function_call_arguments.delta","item_id":"fc0","output_index":0,"delta":"1}"}),
        json!({"type":"response.function_call_arguments.done","item_id":"fc0","output_index":0,"name":"same","arguments":"{\"x\":1}"}),
        json!({"type":"response.output_item.done","output_index":0,"item":function("fc0","call0","{\"x\":1}","completed")}),
    ];
    for mut value in values {
        value["sequence_number"] = json!(0);
        events.push(re(value));
    }
    events.push(native.last().unwrap().clone());
    renumber(&mut events);
    let mut probe = ResponsesToClaudeStream::new(rc(), flow(), Default::default()).unwrap();
    let mut starts = 0;
    for event in events.iter().take(3) {
        for output in probe.push(event.clone()).unwrap().value {
            if matches!(output,cs::StreamEvent::ContentBlockStart(v) if matches!(v.content_block,c::ResponseContentBlock::ToolUse(_)))
            {
                starts += 1;
            }
        }
    }
    assert_eq!(starts, 2);
    r_to_c(events);
}
#[test]
fn early_service_tier_is_preserved_and_late_tier_is_reported_without_fabricating_a_header() {
    for late in [false, true] {
        let mut body = response(
            json!([message("m", json!([text("x")]), "completed")]),
            "completed",
            None,
        );
        body.service_tier = Some(Some(
            gproxy_protocol::wire::openai::responses::ServiceTier::Priority,
        ));
        let mut events = r_events(body);
        if late {
            let rs::StreamEvent::Created(v) = &mut events[0] else {
                unreachable!()
            };
            v.response.service_tier = None;
        }
        let mut context = rc();
        context.usage.as_mut().unwrap().service_tier = Some(None);
        let mut stream = ResponsesToClaudeStream::new(context, flow(), Default::default()).unwrap();
        let mut output = Vec::new();
        for event in events {
            output.extend(stream.push(event).unwrap().value);
        }
        let end = stream.finish().unwrap();
        let reported = end
            .report
            .diagnostics
            .iter()
            .any(|v| v.field == "usage.service_tier");
        output.extend(end.chunks);
        let actual = collect_c(output);
        if late {
            assert_eq!(actual.usage.service_tier, Some(None));
            assert!(reported);
        } else {
            assert_eq!(
                actual.usage.service_tier,
                Some(Some(c::ResponseServiceTier::Priority))
            );
        }
    }
}
#[test]
fn terminal_and_seeded_output_events_are_charged_against_aggregate_event_limits() {
    let limits = StreamLimits {
        max_events: 5,
        ..Default::default()
    };
    let mut stream = ClaudeToResponsesStream::new(context(), flow(), limits).unwrap();
    for event in cevents(vec![json!({"type":"text","text":"x"})], "end_turn") {
        stream.push(event).unwrap();
    }
    assert_eq!(
        stream.finish().unwrap_err().kind(),
        TransformErrorKind::Limit
    );
    let body = response(
        json!([message("m", json!([text("x")]), "completed")]),
        "completed",
        None,
    );
    let native = r_events(body.clone());
    let mut created = native[0].clone();
    let rs::StreamEvent::Created(v) = &mut created else {
        unreachable!()
    };
    v.response.output =
        vec![serde_json::from_value(message("m", json!([text("x")]), "in_progress")).unwrap()];
    let mut events = vec![
        created,
        re(
            json!({"type":"response.output_item.done","sequence_number":1,"output_index":0,"item":message("m",json!([text("x")]),"completed")}),
        ),
        native.last().unwrap().clone(),
    ];
    renumber(&mut events);
    collect_r(events.clone());
    let mut stream = ResponsesToClaudeStream::new(rc(), flow(), limits).unwrap();
    for event in events {
        stream.push(event).unwrap();
    }
    assert_eq!(
        stream.finish().unwrap_err().kind(),
        TransformErrorKind::Limit
    );
}
#[test]
fn fixed_input_conflicts_source_failures_and_premature_eof_cannot_finish() {
    let body = response(
        json!([message("m", json!([text("x")]), "completed")]),
        "completed",
        None,
    );
    let mut initial_context = rc();
    initial_context.usage.as_mut().unwrap().input_tokens = 2;
    let mut stream =
        ResponsesToClaudeStream::new(initial_context, flow(), Default::default()).unwrap();
    for event in r_events(body.clone()) {
        stream.push(event).unwrap();
    }
    assert!(stream.finish().is_err());
    let mut stream = ResponsesToClaudeStream::new(rc(), flow(), Default::default()).unwrap();
    let mut events = r_events(body.clone());
    events.pop();
    for event in events {
        stream.push(event).unwrap();
    }
    assert!(stream.finish().is_err());
    let mut stream = ClaudeToResponsesStream::new(context(), flow(), Default::default()).unwrap();
    stream.push(cstart()).unwrap();
    assert!(stream.finish().is_err());
    let mut failure = body;
    failure.status = Some(r::ResponseStatus::Failed);
    failure.error = Some(
        serde_json::from_value(json!({"code":"server_error","message":"actual native failure"}))
            .unwrap(),
    );
    let created = r_events(response(json!([]), "completed", None)).remove(0);
    let mut stream = ResponsesToClaudeStream::new(rc(), flow(), Default::default()).unwrap();
    stream.push(created).unwrap();
    let error = stream
        .push(re(
            json!({"type":"response.failed","sequence_number":1,"response":failure}),
        ))
        .unwrap_err();
    assert!(error.detail().contains("actual native failure"));
    assert!(stream.finish().is_err());
}
