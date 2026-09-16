use gproxy_protocol::{
    transform::{
        generate::chat_responses::{
            ResponsesResponseContext,
            stream::{ChatToResponsesContext, ChatToResponsesStream, StreamLimits},
        },
        identity::{IdNamespace, IdentityFlow},
    },
    wire::openai::{chat::stream as cs, responses::input as ri},
};
use serde_json::json;

fn chat(value: serde_json::Value) -> cs::ChatCompletionChunk {
    serde_json::from_value(value).unwrap()
}
fn context() -> ChatToResponsesContext {
    ChatToResponsesContext {
        response: ResponsesResponseContext {
            request: serde_json::from_value(json!({"model":"responses-model","input":"hello"}))
                .unwrap(),
            effective_parallel_tool_calls: false,
            effective_tool_choice: ri::ToolChoice::Mode(ri::ToolChoiceMode::Auto),
            usage: Default::default(),
            effective_prompt_cache_options: None,
        },
    }
}

#[test]
fn chat_text_is_emitted_as_typed_responses_deltas_before_done() {
    let flow = IdentityFlow::new(IdNamespace::with_bytes([7; 16]));
    let mut adapter = ChatToResponsesStream::new(context(), flow, StreamLimits::default());
    let early = adapter.push(chat(json!({"id":"chat-1","object":"chat.completion.chunk","created":1700000000,"model":"chat-model","choices":[{"index":0,"delta":{"role":"assistant","content":"hel"},"finish_reason":null}]}))).unwrap();
    assert!(early.value.iter().any(|e| matches!(
        e,
        gproxy_protocol::wire::openai::responses::stream::StreamEvent::OutputTextDelta(_)
    )));
    let later = adapter.push(chat(json!({"id":"chat-1","object":"chat.completion.chunk","created":1700000000,"model":"chat-model","choices":[{"index":0,"delta":{"content":"lo"},"finish_reason":"stop"}]}))).unwrap();
    assert!(later.value.iter().any(|e| matches!(
        e,
        gproxy_protocol::wire::openai::responses::stream::StreamEvent::OutputTextDone(_)
    )));
    adapter.push_done().unwrap();
    let end = adapter.finish().unwrap();
    assert!(end.chunks.iter().any(|e| matches!(
        e,
        gproxy_protocol::wire::openai::responses::stream::StreamEvent::Completed(_)
    )));
}

#[test]
fn chat_tool_item_and_call_ids_are_distinct_and_arguments_are_lexical() {
    let flow = IdentityFlow::new(IdNamespace::with_bytes([8; 16]));
    let mut adapter = ChatToResponsesStream::new(context(), flow, StreamLimits::default());
    let first = chat(
        json!({"id":"chat-2","object":"chat.completion.chunk","created":1700000001,"model":"chat-model","choices":[{"index":0,"delta":{"role":"assistant","tool_calls":[{"index":0,"id":"call-source","type":"function","function":{"name":"lookup","arguments":"{\"q\":"}}]},"finish_reason":null}]}),
    );
    adapter.push(first).unwrap();
    let second = chat(
        json!({"id":"chat-2","object":"chat.completion.chunk","created":1700000001,"model":"chat-model","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"name":"","arguments":"1}"}}]},"finish_reason":"tool_calls"}]}),
    );
    let mut events = adapter.push(second).unwrap().value;
    adapter.push_done().unwrap();
    let end = adapter.finish().unwrap();
    events.extend(end.chunks);
    let item = events
        .iter()
        .find_map(|event| {
            if let gproxy_protocol::wire::openai::responses::stream::StreamEvent::OutputItemDone(
                v,
            ) = event
            {
                Some(&v.item)
            } else {
                None
            }
        })
        .unwrap();
    let value = serde_json::to_value(item).unwrap();
    assert_eq!(value["call_id"], "call-source");
    assert_ne!(value["id"], value["call_id"]);
    assert_eq!(value["arguments"], "{\"q\":1}");
}

#[test]
fn responses_stream_rejects_audio_and_requires_terminal_source_event() {
    use gproxy_protocol::transform::generate::chat_responses::stream::ResponsesToChatStream;
    let flow = IdentityFlow::new(IdNamespace::with_bytes([9; 16]));
    let mut adapter = ResponsesToChatStream::new(flow, StreamLimits::default());
    let event: gproxy_protocol::wire::openai::responses::stream::StreamEvent = serde_json::from_value(json!({"type":"response.created","sequence_number":0,"response":{"id":"resp-1","object":"response","created_at":1700000002,"model":"responses-model","output":[],"parallel_tool_calls":false,"tool_choice":"auto","tools":[],"status":"in_progress","error":null,"incomplete_details":null,"instructions":null,"metadata":null,"temperature":null,"top_p":null,"usage":null,"user":null}})).unwrap();
    adapter.push(event).unwrap();
    assert!(adapter.finish().is_err());
}

#[test]
fn responses_text_delta_reaches_chat_before_responses_terminal() {
    use gproxy_protocol::transform::generate::chat_responses::stream::{
        ResponsesToChatStream, StreamLimits,
    };
    use gproxy_protocol::transform::generate::stream::responses::synthesize_responses_stream;
    use gproxy_protocol::wire::openai::responses::response as rr;
    let response: rr::GenerateContentResponseBody = serde_json::from_value(json!({
        "id":"resp-text","object":"response","created_at":1700000003,"model":"responses-model",
        "output":[{"type":"message","id":"msg-1","role":"assistant","status":"completed","content":[{"type":"output_text","text":"hello","annotations":[],"logprobs":[]}]}],
        "parallel_tool_calls":false,"tool_choice":"auto","tools":[],"status":"completed","error":null,"incomplete_details":null,"instructions":null,"metadata":null,"temperature":null,"top_p":null,"usage":null,"user":null
    })).unwrap();
    let limits = StreamLimits::default();
    let mut source_flow = IdentityFlow::new(IdNamespace::with_bytes([10; 16]));
    let events = synthesize_responses_stream(
        response,
        &mut source_flow,
        gproxy_protocol::transform::generate::stream::responses::ResponsesStreamLimits::default(),
    )
    .unwrap()
    .value;
    let mut adapter =
        ResponsesToChatStream::new(IdentityFlow::new(IdNamespace::with_bytes([11; 16])), limits);
    let mut saw_text = false;
    let mut saw_terminal = false;
    for event in events {
        let converted = adapter.push(event).unwrap();
        saw_terminal |= converted
            .value
            .iter()
            .any(|chunk| chunk.choices.iter().any(|c| c.finish_reason.is_some()));
        saw_text |= converted.value.iter().any(|chunk| {
            chunk.choices.iter().any(|choice| {
                choice
                    .delta
                    .content
                    .as_ref()
                    .and_then(Option::as_ref)
                    .is_some_and(|v| v == "hello")
            })
        });
    }
    assert!(saw_text);
    let end = adapter.finish().unwrap();
    assert!(saw_terminal);
    assert!(end.chunks.is_empty());
}

use gproxy_protocol::transform::generate::{
    chat_responses::stream::ResponsesToChatStream,
    stream::{
        chat::ChatStreamCollector,
        responses::{ResponsesStreamCollector, ResponsesStreamLimits, synthesize_responses_stream},
    },
};
use gproxy_protocol::{
    Dialect,
    transform::identity::TargetIdPolicy,
    wire::openai::{
        chat::response as cr,
        responses::{response as rr, stream as rs},
    },
};
fn ids() -> IdentityFlow {
    IdentityFlow::new(IdNamespace::with_bytes([37; 16]))
}
fn cchunk(delta: serde_json::Value, finish: serde_json::Value) -> cs::ChatCompletionChunk {
    chat(
        json!({"id":"source-chat","object":"chat.completion.chunk","created":17,"model":"actual","choices":[{"index":0,"delta":delta,"finish_reason":finish}]}),
    )
}
fn finish_cr(
    chunks: Vec<cs::ChatCompletionChunk>,
    context: ChatToResponsesContext,
) -> rr::GenerateContentResponseBody {
    let mut adapter = ChatToResponsesStream::new(context, ids(), StreamLimits::default());
    let mut target = ResponsesStreamCollector::new(ResponsesStreamLimits::default());
    for chunk in chunks {
        for event in adapter.push(chunk).unwrap().value {
            target.push(event).unwrap();
        }
    }
    adapter.push_done().unwrap();
    for event in adapter.finish().unwrap().chunks {
        target.push(event).unwrap();
    }
    target.finish().unwrap().value
}
fn response(output: serde_json::Value) -> rr::GenerateContentResponseBody {
    serde_json::from_value(json!({"id":"resp-source","object":"response","created_at":17,"model":"actual","output":output,"parallel_tool_calls":false,"tool_choice":"auto","tools":[],"status":"completed","error":null,"incomplete_details":null,"instructions":null,"metadata":null,"temperature":null,"top_p":null,"usage":null,"user":null})).unwrap()
}
fn events(response: rr::GenerateContentResponseBody) -> Vec<rs::StreamEvent> {
    synthesize_responses_stream(response, &mut ids(), ResponsesStreamLimits::default())
        .unwrap()
        .value
}
fn finish_rc(events: Vec<rs::StreamEvent>) -> cr::GenerateContentResponseBody {
    let mut adapter = ResponsesToChatStream::new(ids(), StreamLimits::default());
    let mut target = ChatStreamCollector::new(ids(), TargetIdPolicy::new(Dialect::OpenAiChat));
    for event in events {
        for chunk in adapter.push(event).unwrap().value {
            target.push(chunk).unwrap();
        }
    }
    assert!(adapter.finish().unwrap().chunks.is_empty());
    target.push_done().unwrap();
    target.finish().unwrap().value
}
fn text_item(id: &str, text: &str) -> serde_json::Value {
    json!({"type":"message","id":id,"role":"assistant","status":"completed","content":[{"type":"output_text","text":text,"annotations":[],"logprobs":[]}]})
}
#[test]
fn created_is_first_and_finish_returns_only_the_terminal_tail() {
    let mut adapter = ChatToResponsesStream::new(context(), ids(), StreamLimits::default());
    let batch = adapter
        .push(cchunk(json!({"content":"early"}), serde_json::Value::Null))
        .unwrap()
        .value;
    assert!(matches!(batch.first(), Some(rs::StreamEvent::Created(_))));
    assert!(
        batch
            .iter()
            .any(|e| matches!(e, rs::StreamEvent::OutputTextDelta(_)))
    );
    adapter.push(cchunk(json!({}), json!("stop"))).unwrap();
    adapter.push_done().unwrap();
    let tail = adapter.finish().unwrap().chunks;
    assert_eq!(tail.len(), 1);
    assert!(matches!(tail[0], rs::StreamEvent::Completed(_)));
}
#[test]
fn refusal_only_empty_parts_and_refusal_before_text_have_correct_native_indices() {
    for refusal in ["", "no"] {
        let output = finish_cr(
            vec![cchunk(json!({"refusal":refusal}), json!("stop"))],
            context(),
        );
        assert_eq!(
            serde_json::to_value(output).unwrap()["output"][0]["content"][0]["refusal"],
            refusal
        );
    }
    let output = finish_cr(
        vec![
            cchunk(json!({"refusal":"no"}), serde_json::Value::Null),
            cchunk(json!({"content":"explanation"}), json!("stop")),
        ],
        context(),
    );
    let wire = serde_json::to_value(output).unwrap();
    assert_eq!(wire["output"][0]["content"][0]["text"], "explanation");
    assert_eq!(wire["output"][0]["content"][1]["refusal"], "no");
    let output = finish_cr(
        vec![cchunk(json!({"content":""}), json!("stop"))],
        context(),
    );
    assert_eq!(
        serde_json::to_value(output).unwrap()["output"][0]["content"][0]["text"],
        ""
    );
}
#[test]
fn out_of_order_parallel_tool_slots_and_text_keep_distinct_call_item_roles() {
    let output = finish_cr(
        vec![
            cchunk(
                json!({"tool_calls":[{"index":1,"type":"function","function":{"name":"same","arguments":"{\"b\":"}}]}),
                serde_json::Value::Null,
            ),
            cchunk(
                json!({"content":"text","tool_calls":[{"index":0,"id":"first","type":"function","function":{"name":"same","arguments":"{}"}},{"index":1,"id":"second","function":{"arguments":"2}"}}]}),
                json!("tool_calls"),
            ),
        ],
        context(),
    );
    let w = serde_json::to_value(output).unwrap();
    assert_eq!(w["output"][0]["type"], "message");
    assert_eq!(w["output"][1]["call_id"], "first");
    assert_eq!(w["output"][2]["call_id"], "second");
    assert_ne!(w["output"][1]["id"], w["output"][1]["call_id"]);
    assert_ne!(w["output"][1]["id"], w["output"][2]["id"]);
}
#[test]
fn late_response_id_attaches_without_changing_emitted_alias() {
    let mut a = ChatToResponsesStream::new(context(), ids(), StreamLimits::default());
    let mut first = cchunk(json!({"content":"a"}), serde_json::Value::Null);
    first.id.clear();
    let events = a.push(first).unwrap().value;
    let rs::StreamEvent::Created(v) = &events[0] else {
        panic!()
    };
    let alias = v.response.id.clone();
    assert!(alias.starts_with("resp_"));
    a.push(cchunk(json!({"content":"b"}), json!("stop")))
        .unwrap();
    a.push_done().unwrap();
    let tail = a.finish().unwrap();
    let rs::StreamEvent::Completed(v) = &tail.chunks[0] else {
        panic!()
    };
    assert_eq!(v.response.id, alias);
    let handle = tail
        .identities
        .lookup_source(
            &gproxy_protocol::transform::identity::SourceIdentity::new(
                Dialect::OpenAiChat,
                Some("source-chat".into()),
                0,
            ),
            gproxy_protocol::transform::identity::IdentityRole::Response,
        )
        .unwrap();
    assert_eq!(handle.emitted_id, alias);
}
#[test]
fn usage_tail_preserves_real_details_and_rejects_conflicting_supplements() {
    let mut tail = cchunk(json!({}), serde_json::Value::Null);
    tail.choices.clear();
    tail.usage = Some(Some(serde_json::from_value(json!({"prompt_tokens":7,"completion_tokens":3,"total_tokens":10,"prompt_tokens_details":{"cached_tokens":2,"cache_write_tokens":1},"completion_tokens_details":{"reasoning_tokens":1}})).unwrap()));
    let output = finish_cr(
        vec![cchunk(json!({"content":"x"}), json!("stop")), tail.clone()],
        context(),
    );
    assert_eq!(
        output
            .usage
            .unwrap()
            .unwrap()
            .input_tokens_details
            .cache_write_tokens,
        1
    );
    let mut ctx = context();
    ctx.response.usage.cached_tokens = Some(3);
    let mut a = ChatToResponsesStream::new(ctx, ids(), StreamLimits::default());
    a.push(cchunk(json!({"content":"x"}), json!("stop")))
        .unwrap();
    a.push(tail).unwrap();
    a.push_done().unwrap();
    assert!(a.finish().is_ok());
}
#[test]
fn output_budget_failure_poison_and_missing_done_are_not_success() {
    let mut a = ChatToResponsesStream::new(
        context(),
        ids(),
        StreamLimits {
            max_events: 2,
            ..Default::default()
        },
    );
    assert!(
        a.push(cchunk(json!({"content":"x"}), serde_json::Value::Null))
            .is_err()
    );
    assert!(a.push(cchunk(json!({}), json!("stop"))).is_err());
    assert!(a.push_done().is_err());
    assert!(a.finish().is_err());
    let mut a = ChatToResponsesStream::new(context(), ids(), StreamLimits::default());
    a.push(cchunk(json!({"content":"x"}), json!("stop")))
        .unwrap();
    assert!(a.finish().is_err());
}
#[test]
fn multi_choice_and_sparse_tools_are_rejected_without_selecting_one() {
    let mut a = ChatToResponsesStream::new(context(), ids(), StreamLimits::default());
    let mut chunk = cchunk(json!({"content":"x"}), json!("stop"));
    let mut other = chunk.choices[0].clone();
    other.index = 1;
    chunk.choices.push(other);
    assert!(a.push(chunk).is_err());
    let mut a = ChatToResponsesStream::new(context(), ids(), StreamLimits::default());
    assert!(
        a.push(cchunk(
            json!({"tool_calls":[{"index":2,"function":{"name":"f","arguments":"{}"}}]}),
            json!("tool_calls")
        ))
        .is_err()
    );
}
#[test]
fn nested_rest_is_removed_before_bounds_and_has_no_effect() {
    let plain = cchunk(json!({"content":"x"}), json!("stop"));
    let mut dirty = plain.clone();
    dirty
        .rest
        .insert("unknown".into(), json!("z".repeat(40_000)));
    dirty.choices[0]
        .delta
        .rest
        .insert("unknown".into(), json!({"nested":42}));
    assert_eq!(
        finish_cr(vec![plain], context()),
        finish_cr(vec![dirty], context())
    );
}
#[test]
fn responses_parallel_functions_emit_before_terminal_and_keep_actual_arguments() {
    let source = response(json!([
        {"type":"function_call","id":"fc-a","call_id":"same-a","name":"same","arguments":"{\"a\": 1}","status":"completed"},
        {"type":"function_call","id":"fc-b","call_id":"same-b","name":"same","arguments":"{}","status":"completed"}
    ]));
    let mut a = ResponsesToChatStream::new(ids(), StreamLimits::default());
    let mut tool_before_end = false;
    for event in events(source.clone()) {
        let terminal = matches!(event, rs::StreamEvent::Completed(_));
        let out = a.push(event).unwrap();
        if !terminal {
            tool_before_end |= out
                .value
                .iter()
                .any(|c| c.choices.iter().any(|v| v.delta.tool_calls.is_some()));
        }
    }
    a.finish().unwrap();
    assert!(tool_before_end);
    let output = finish_rc(events(source));
    let tools = output.choices[0].message.tool_calls.as_ref().unwrap();
    let w = serde_json::to_value(tools).unwrap();
    assert_eq!(w[0]["id"], "same-a");
    assert_eq!(w[1]["id"], "same-b");
    assert_eq!(w[0]["function"]["arguments"], "{\"a\": 1}");
}
#[test]
fn responses_seeded_text_is_not_lost_or_replayed() {
    let final_response = response(json!([text_item("msg", "prefix")]));
    let mut source = events(final_response);
    let mut replay = Vec::new();
    for mut event in source.drain(..) {
        match &mut event {
            rs::StreamEvent::Created(v) => {
                let mut seed = text_item("msg", "pre");
                seed["status"] = json!("in_progress");
                v.response.output = vec![serde_json::from_value(seed).unwrap()];
            }
            rs::StreamEvent::OutputItemAdded(_) | rs::StreamEvent::ContentPartAdded(_) => continue,
            rs::StreamEvent::OutputTextDelta(v) => v.delta = "fix".into(),
            _ => {}
        }
        replay.push(event);
    }
    let output = finish_rc(replay);
    assert_eq!(output.choices[0].message.content.as_deref(), Some("prefix"));
}
#[test]
fn responses_error_sequence_and_item_call_confusion_poison() {
    let mut a = ResponsesToChatStream::new(ids(), StreamLimits::default());
    let mut source = events(response(json!([text_item("msg", "x")])));
    for event in &mut source {
        if let rs::StreamEvent::OutputTextDelta(v) = event {
            v.item_id = "call-not-item".into();
        }
    }
    assert!(
        source
            .into_iter()
            .try_for_each(|event| a.push(event).map(|_| ()))
            .is_err()
    );
    assert!(a.finish().is_err());
    let mut a = ResponsesToChatStream::new(ids(), StreamLimits::default());
    let first = events(response(json!([]))).remove(0);
    a.push(first.clone()).unwrap();
    assert!(a.push(first).is_err());
    let mut a = ResponsesToChatStream::new(ids(), StreamLimits::default());
    let error: rs::StreamEvent = serde_json::from_value(json!({"type":"error","sequence_number":0,"message":"real upstream failure","code":"quota","param":null})).unwrap();
    assert!(
        a.push(error)
            .unwrap_err()
            .to_string()
            .contains("real upstream failure")
    );
    assert!(a.finish().is_err());
}
#[test]
fn response_final_usage_and_logprobs_match_direct_buffered_mapping() {
    let mut source = response(
        json!([{"type":"message","id":"msg","role":"assistant","status":"completed","content":[{"type":"output_text","text":"x","annotations":[],"logprobs":[{"token":"x","bytes":[120],"logprob":-0.2,"top_logprobs":[]}]}]}]),
    );
    source.usage = Some(Some(serde_json::from_value(json!({"input_tokens":5,"output_tokens":2,"total_tokens":7,"input_tokens_details":{"cached_tokens":1,"cache_write_tokens":1},"output_tokens_details":{"reasoning_tokens":1}})).unwrap()));
    let expected =
        gproxy_protocol::transform::generate::chat_responses::responses_to_chat_response(
            source.clone(),
            &mut ids(),
            &TargetIdPolicy::new(Dialect::OpenAiChat),
        )
        .unwrap()
        .value;
    assert_eq!(finish_rc(events(source)), expected);
}

#[test]
fn interleaved_response_messages_emit_in_canonical_item_order() {
    let original = events(response(json!([
        text_item("m1", "first"),
        text_item("m2", "second")
    ])));
    let created = original[0].clone();
    let final_event = original.last().unwrap().clone();
    let mut first = Vec::new();
    let mut second = Vec::new();
    for event in original.into_iter().skip(1) {
        let wire = serde_json::to_value(&event).unwrap();
        match wire.get("output_index").and_then(|v| v.as_i64()) {
            Some(0) => first.push(event),
            Some(1) => second.push(event),
            _ => {}
        }
    }
    let mut ordered = vec![created];
    ordered.extend(first.drain(..3));
    ordered.extend(second);
    ordered.extend(first);
    ordered.push(final_event);
    let reordered = ordered
        .into_iter()
        .enumerate()
        .map(|(index, event)| {
            let mut value = serde_json::to_value(event).unwrap();
            value["sequence_number"] = json!(index);
            serde_json::from_value(value).unwrap()
        })
        .collect();
    assert_eq!(
        finish_rc(reordered).choices[0].message.content.as_deref(),
        Some("firstsecond")
    );
}
#[test]
fn seeded_function_arguments_and_late_item_id_preserve_the_call_id() {
    let original = events(response(
        json!([{"type":"function_call","id":"fc-known-late","call_id":"call-original","name":"f","arguments":"{\"x\":1}","status":"completed"}]),
    ));
    let modified = original
        .into_iter()
        .map(|mut event| {
            match &mut event {
                rs::StreamEvent::OutputItemAdded(v) => {
                    if let rr::ResponseOutputItem::FunctionCall(f) = &mut v.item {
                        f.id = None;
                        f.arguments = "{\"x\":".into();
                    }
                }
                rs::StreamEvent::FunctionCallArgumentsDelta(v) => v.delta = "1}".into(),
                _ => {}
            }
            event
        })
        .collect();
    let output = finish_rc(modified);
    let tool = serde_json::to_value(&output.choices[0].message.tool_calls).unwrap();
    assert_eq!(tool[0]["id"], "call-original");
    assert_eq!(tool[0]["function"]["arguments"], "{\"x\":1}");
}
#[test]
fn chat_logprobs_are_attached_at_done_with_original_token_bytes() {
    let mut chunk = cchunk(json!({"content":"x"}), json!("stop"));
    chunk.choices[0].logprobs = Some(Some(serde_json::from_value(json!({"content":[{"token":"x","bytes":[120],"logprob":-0.2,"top_logprobs":[{"token":"y","bytes":[121],"logprob":-0.3}]}],"refusal":null})).unwrap()));
    let output = finish_cr(vec![chunk], context());
    let wire = serde_json::to_value(output).unwrap();
    assert_eq!(
        wire["output"][0]["content"][0]["logprobs"][0]["bytes"],
        json!([120])
    );
    assert_eq!(
        wire["output"][0]["content"][0]["logprobs"][0]["top_logprobs"][0]["bytes"],
        json!([121])
    );
}
#[test]
fn legacy_functions_missing_modern_ids_and_empty_messages_are_stable() {
    let legacy = finish_cr(
        vec![cchunk(
            json!({"function_call":{"name":"f","arguments":"{}"}}),
            json!("function_call"),
        )],
        context(),
    );
    let w = serde_json::to_value(legacy).unwrap();
    assert!(
        w["output"][0]["call_id"]
            .as_str()
            .unwrap()
            .starts_with("call_")
    );
    let missing = finish_cr(
        vec![cchunk(
            json!({"tool_calls":[{"index":0,"function":{"name":"f","arguments":"{}"}},{"index":1,"function":{"name":"f","arguments":"{}"}}]}),
            json!("tool_calls"),
        )],
        context(),
    );
    let w = serde_json::to_value(missing).unwrap();
    assert_ne!(w["output"][0]["call_id"], w["output"][1]["call_id"]);
    assert!(
        finish_cr(
            vec![cchunk(json!({"role":"assistant"}), json!("stop"))],
            context()
        )
        .output
        .is_empty()
    );
}
#[test]
fn target_context_contradiction_and_tool_server_ownership_fail_before_exposure() {
    let mut ctx = context();
    ctx.response.request.parallel_tool_calls = Some(Some(true));
    let mut a = ChatToResponsesStream::new(ctx, ids(), StreamLimits::default());
    assert!(
        a.push(cchunk(json!({"content":"x"}), json!("stop")))
            .is_err()
    );
    let source = response(
        json!([{"type":"function_call","id":"fc","call_id":"call","name":"f","arguments":"{}","namespace":"host-tools","status":"completed"}]),
    );
    let mut a = ResponsesToChatStream::new(ids(), StreamLimits::default());
    assert!(
        events(source)
            .into_iter()
            .try_for_each(|event| a.push(event).map(|_| ()))
            .is_err()
    );
    assert!(a.finish().is_err());
}

#[test]
fn late_model_uses_bounded_pending_fragments_without_fake_start_facts() {
    let mut adapter = ChatToResponsesStream::new(context(), ids(), StreamLimits::default());
    let mut first = cchunk(json!({"content":"early"}), serde_json::Value::Null);
    first.model.clear();
    assert!(adapter.push(first).unwrap().value.is_empty());
    let output = adapter
        .push(cchunk(json!({"content":"late"}), json!("stop")))
        .unwrap()
        .value;
    let rs::StreamEvent::Created(v) = &output[0] else {
        panic!()
    };
    assert_eq!(v.response.model, "actual");
    let mut native = ResponsesStreamCollector::new(ResponsesStreamLimits::default());
    for event in output {
        native.push(event).unwrap();
    }
    adapter.push_done().unwrap();
    for event in adapter.finish().unwrap().chunks {
        native.push(event).unwrap();
    }
    assert_eq!(
        serde_json::to_value(native.finish().unwrap().value).unwrap()["output"][0]["content"][0]["text"],
        "earlylate"
    );
}
#[test]
fn incomplete_usage_overflow_and_content_filter_keep_terminal_semantics() {
    let output = finish_cr(
        vec![cchunk(
            json!({"refusal":"blocked"}),
            json!("content_filter"),
        )],
        context(),
    );
    assert_eq!(output.status, Some(rr::ResponseStatus::Incomplete));
    assert_eq!(
        finish_rc(events(output)).choices[0].finish_reason,
        cr::FinishReason::ContentFilter
    );
    let mut source = response(json!([text_item("msg", "x")]));
    let mut wire_events = events(source.clone());
    source.usage = Some(Some(serde_json::from_value(json!({"input_tokens":9223372036854775807_i64,"output_tokens":1,"total_tokens":9223372036854775807_i64,"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},"output_tokens_details":{"reasoning_tokens":0}})).unwrap()));
    for event in &mut wire_events {
        if let rs::StreamEvent::Completed(v) = event {
            v.response.usage = source.usage.clone();
        }
    }
    let mut a = ResponsesToChatStream::new(ids(), StreamLimits::default());
    assert!(
        wire_events
            .into_iter()
            .try_for_each(|e| a.push(e).map(|_| ()))
            .is_err()
    );
    assert!(a.finish().is_err());
}

#[test]
fn shared_v2_v3_v4_fixtures_keep_tool_links_and_reject_missing_framing_terminator() {
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "fixtures/chat_responses_incremental_parity.json"
    ))
    .unwrap();
    for case in cases.as_array().unwrap() {
        let mut a = ChatToResponsesStream::new(context(), ids(), StreamLimits::default());
        let mut native = ResponsesStreamCollector::new(ResponsesStreamLimits::default());
        for event in case["events"].as_array().unwrap() {
            for target in a.push(chat(event.clone())).unwrap().value {
                native.push(target).unwrap();
            }
        }
        if case["done"] != true {
            assert!(a.finish().is_err());
            continue;
        }
        a.push_done().unwrap();
        for event in a.finish().unwrap().chunks {
            native.push(event).unwrap();
        }
        let value = serde_json::to_value(native.finish().unwrap().value).unwrap();
        match case["name"].as_str().unwrap() {
            "text" => assert_eq!(value["output"][0]["content"][0]["text"], "hello world"),
            "refusal_before_text" => {
                assert_eq!(value["output"][0]["content"][0]["text"], "why");
                assert_eq!(value["output"][0]["content"][1]["refusal"], "no");
            }
            "parallel_fragmented_tools" => {
                assert_eq!(value["output"][0]["name"], "look");
                assert_eq!(value["output"][1]["name"], "look");
                assert_eq!(value["output"][0]["call_id"], "call-a");
                assert_eq!(value["output"][1]["call_id"], "call-b");
                assert_ne!(value["output"][0]["id"], value["output"][0]["call_id"]);
            }
            "late_tool_id" => assert_eq!(value["output"][0]["call_id"], "call-late"),
            "usage_tail" => assert_eq!(
                value["usage"]["input_tokens_details"]["cache_write_tokens"],
                1
            ),
            name => panic!("unexpected fixture {name}"),
        }
    }
}
