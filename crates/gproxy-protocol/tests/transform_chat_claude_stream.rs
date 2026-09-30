use gproxy_protocol::{
    Dialect,
    transform::{
        TransformErrorKind,
        generate::{
            claude_chat::{self, stream::*},
            stream::{
                chat::ChatStreamCollector,
                claude::{ClaudeStreamCollector, synthesize_claude_stream},
            },
        },
        identity::{IdNamespace, IdentityFlow, TargetIdPolicy},
    },
    wire::{
        claude::{generate_content as c, stream as s},
        openai::chat::{self as o, stream as q},
    },
};
use serde_json::{Value, json};
fn flow() -> IdentityFlow {
    IdentityFlow::new(IdNamespace::with_bytes([9; 16]))
}
fn c_usage(input: i64, output: i64) -> c::Usage {
    serde_json::from_value(json!({"input_tokens":input,"output_tokens":output,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens_details":{"thinking_tokens":0}})).unwrap()
}
fn context() -> ChatToClaudeContext {
    ChatToClaudeContext {
        start_usage: Some(c_usage(3, 0)),
    }
}
fn chat(delta: Value, finish: Option<&str>, usage: Option<Value>) -> q::ChatCompletionChunk {
    serde_json::from_value(json!({"id":"r","model":"m","created":7,"object":"chat.completion.chunk","choices":[{"index":0,"delta":delta,"finish_reason":finish}],"usage":usage})).unwrap()
}
fn q_usage() -> Value {
    json!({"prompt_tokens":3,"completion_tokens":2,"total_tokens":5,"prompt_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},"completion_tokens_details":{"reasoning_tokens":0}})
}
fn source_reply(content: Value, stop: &str) -> c::GenerateContentResponseBody {
    serde_json::from_value(json!({"id":"msg","type":"message","model":"m","role":"assistant","content":content,"stop_reason":stop,"stop_sequence":null,"usage":c_usage(3,2)})).unwrap()
}
fn collect_c(events: Vec<s::StreamEvent>) -> c::GenerateContentResponseBody {
    let mut c = ClaudeStreamCollector::new(Default::default());
    for e in events {
        c.push(e).unwrap();
    }
    c.finish().unwrap().value
}
fn collect_q(chunks: Vec<q::ChatCompletionChunk>) -> o::GenerateContentResponseBody {
    let mut c = ChatStreamCollector::new(flow(), TargetIdPolicy::new(Dialect::OpenAiChat));
    for e in chunks {
        c.push(e).unwrap();
    }
    c.push_done().unwrap();
    c.finish().unwrap().value
}
#[test]
fn chat_text_emits_before_finish_and_source_done_is_required() {
    let source = vec![
        chat(json!({"role":"assistant","content":"he"}), None, None),
        chat(json!({"content":"llo"}), Some("stop"), Some(q_usage())),
    ];
    let mut stream = ChatToClaudeStream::new(context(), flow(), Default::default()).unwrap();
    let mut out = Vec::new();
    for (n, chunk) in source.iter().cloned().enumerate() {
        let mapped = stream.push(chunk).unwrap();
        if n == 0 {
            assert!(mapped.value.iter().any(|v|matches!(v,s::StreamEvent::ContentBlockDelta(v) if matches!(&v.delta,s::ContentBlockDelta::Text(v) if v.text=="he"))));
            assert!(
                !mapped
                    .value
                    .iter()
                    .any(|v| matches!(v, s::StreamEvent::MessageStop(_)))
            );
        }
        out.extend(mapped.value);
    }
    stream.push_done().unwrap();
    out.extend(stream.finish().unwrap().chunks);
    let actual = collect_c(out);
    let expected =
        claude_chat::openai_response_to_claude(&collect_q(source), "m", &Default::default())
            .unwrap()
            .value;
    assert_eq!(actual, expected);
}
#[test]
fn unknown_initial_usage_holds_text_until_actual_usage_arrives() {
    let mut stream =
        ChatToClaudeStream::new(ChatToClaudeContext::default(), flow(), Default::default())
            .unwrap();
    assert!(
        stream
            .push(chat(json!({"content":"pending"}), None, None))
            .unwrap()
            .value
            .is_empty()
    );
    let mut out = stream
        .push(chat(json!({}), Some("stop"), Some(q_usage())))
        .unwrap()
        .value;
    assert!(
        out.iter()
            .any(|e| matches!(e, s::StreamEvent::ContentBlockDelta(_)))
    );
    let s::StreamEvent::MessageStart(start) = &out[0] else {
        panic!("start first")
    };
    assert_eq!(start.message.usage.output_tokens, 2);
    stream.push_done().unwrap();
    out.extend(stream.finish().unwrap().chunks);
    assert_eq!(collect_c(out).usage.output_tokens, 2);
}
#[test]
fn chat_parallel_same_name_tools_keep_ids_and_split_names_until_immutable_start() {
    let first = chat(
        json!({"tool_calls":[{"index":0,"function":{"name":"sa","arguments":"{\"x\":"}},{"index":1,"function":{"name":"same","arguments":"{"}}]}),
        None,
        None,
    );
    let last = chat(
        json!({"tool_calls":[{"index":0,"id":"native-a","function":{"name":"me","arguments":"1}"}},{"index":1,"id":"native-b","function":{"arguments":"\"x\":2}"}}]}),
        Some("tool_calls"),
        Some(q_usage()),
    );
    let mut stream = ChatToClaudeStream::new(context(), flow(), Default::default()).unwrap();
    let mut out = stream.push(first.clone()).unwrap().value;
    assert!(!out.iter().any(|e|matches!(e,s::StreamEvent::ContentBlockStart(v) if matches!(v.content_block,c::ResponseContentBlock::ToolUse(_)))));
    let mapped = stream.push(last.clone()).unwrap();
    assert!(mapped.value.iter().any(|e|matches!(e,s::StreamEvent::ContentBlockDelta(v) if matches!(v.delta,s::ContentBlockDelta::InputJson(_)))));
    out.extend(mapped.value);
    stream.push_done().unwrap();
    out.extend(stream.finish().unwrap().chunks);
    let actual = collect_c(out);
    let v = serde_json::to_value(&actual).unwrap();
    assert_eq!(v["content"][0]["name"], "same");
    assert_eq!(v["content"][1]["name"], "same");
    assert_eq!(v["content"][0]["id"], "native-a");
    assert_eq!(v["content"][1]["id"], "native-b");
    assert_eq!(
        actual,
        claude_chat::openai_response_to_claude(
            &collect_q(vec![first, last]),
            "m",
            &Default::default()
        )
        .unwrap()
        .value
    );
}
#[test]
fn missing_modern_ids_and_legacy_fragments_use_validated_claude_prefixes() {
    for legacy in [false, true] {
        let mut stream = ChatToClaudeStream::new(context(), flow(), Default::default()).unwrap();
        let delta = if legacy {
            json!({"function_call":{"name":"f","arguments":"{}"}})
        } else {
            json!({"tool_calls":[{"index":0,"function":{"name":"f","arguments":"{}"}},{"index":1,"function":{"name":"f","arguments":"{}"}}]})
        };
        let mut out = stream
            .push(chat(
                delta,
                Some(if legacy {
                    "function_call"
                } else {
                    "tool_calls"
                }),
                Some(q_usage()),
            ))
            .unwrap()
            .value;
        stream.push_done().unwrap();
        out.extend(stream.finish().unwrap().chunks);
        let v = serde_json::to_value(collect_c(out)).unwrap();
        assert!(
            v["content"][0]["id"]
                .as_str()
                .unwrap()
                .starts_with("toolu_")
        );
        if !legacy {
            assert_ne!(v["content"][0]["id"], v["content"][1]["id"]);
        }
    }
}
#[test]
fn claude_text_and_tool_deltas_are_preterminal_and_match_buffered_output() {
    let response = source_reply(
        json!([{"type":"text","text":"hello"},{"type":"tool_use","id":"tool-a","name":"same","input":{"x":1}},{"type":"tool_use","id":"tool-b","name":"same","input":{"x":2}}]),
        "tool_use",
    );
    let source = synthesize_claude_stream(response.clone(), Default::default())
        .unwrap()
        .value;
    let mut stream = ClaudeToChatStream::new(
        ClaudeToChatContext { created: 7 },
        flow(),
        Default::default(),
    )
    .unwrap();
    let mut out = Vec::new();
    let mut preterminal = false;
    for event in source {
        let stop = matches!(event, s::StreamEvent::MessageStop(_));
        let value = stream.push(event).unwrap();
        if !stop
            && value.value.iter().any(|c| {
                c.choices.iter().any(|v| {
                    v.delta
                        .content
                        .as_ref()
                        .and_then(Option::as_ref)
                        .is_some_and(|v| !v.is_empty())
                })
            })
        {
            preterminal = true;
        }
        out.extend(value.value);
    }
    assert!(preterminal);
    out.extend(stream.finish().unwrap().chunks);
    let actual = collect_q(out);
    let expected = claude_chat::claude_response_to_openai(
        &response,
        "m",
        &claude_chat::ResponseSupplement {
            created_unix_seconds: Some(7),
        },
    )
    .unwrap()
    .value;
    assert_eq!(actual, expected);
}
#[test]
fn claude_empty_input_without_json_delta_becomes_one_complete_empty_object() {
    let response = source_reply(
        json!([{"type":"tool_use","id":"tool","name":"f","input":{}}]),
        "tool_use",
    );
    let mut source = synthesize_claude_stream(response, Default::default())
        .unwrap()
        .value;
    source.retain(|e|!matches!(e,s::StreamEvent::ContentBlockDelta(v) if matches!(v.delta,s::ContentBlockDelta::InputJson(_))));
    let mut stream = ClaudeToChatStream::new(
        ClaudeToChatContext { created: 7 },
        flow(),
        Default::default(),
    )
    .unwrap();
    let mut out = Vec::new();
    for e in source {
        out.extend(stream.push(e).unwrap().value);
    }
    out.extend(stream.finish().unwrap().chunks);
    let v = serde_json::to_value(collect_q(out)).unwrap();
    assert_eq!(
        v["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"],
        "{}"
    );
}
#[test]
fn late_claude_refusal_keeps_streamed_text_and_reports_content_filter() {
    let source = synthesize_claude_stream(
        source_reply(json!([{"type":"text","text":"cannot"}]), "refusal"),
        Default::default(),
    )
    .unwrap()
    .value;
    let mut stream = ClaudeToChatStream::new(
        ClaudeToChatContext { created: 7 },
        flow(),
        Default::default(),
    )
    .unwrap();
    let mut out = Vec::new();
    for e in source {
        out.extend(stream.push(e).unwrap().value);
    }
    let end = stream.finish().unwrap();
    assert!(end.report.diagnostics.iter().any(|v| v.field == "refusal"));
    out.extend(end.chunks);
    let v = collect_q(out);
    assert_eq!(v.choices[0].finish_reason, o::FinishReason::ContentFilter);
    assert_eq!(v.choices[0].message.content.as_deref(), Some("cannot"));
    assert!(v.choices[0].message.refusal.is_none());
}
#[test]
fn visible_reasoning_is_typed_and_opaque_signatures_and_rest_are_not_text() {
    let response = source_reply(
        json!([{"type":"thinking","thinking":"secret","signature":"opaque","x-extension":"DROP"},{"type":"text","text":"visible","x-extension":"DROP"},{"type":"tool_use","id":"tool","name":"f","input":{"x-formal":"KEEP"},"x-extension":"DROP"}]),
        "tool_use",
    );
    let events = synthesize_claude_stream(response, Default::default())
        .unwrap()
        .value;
    let mut stream = ClaudeToChatStream::new(
        ClaudeToChatContext { created: 7 },
        flow(),
        Default::default(),
    )
    .unwrap();
    let mut out = Vec::new();
    let mut diagnostics = Vec::new();
    for event in events {
        let v = stream.push(event).unwrap();
        out.extend(v.value);
        diagnostics.extend(v.report.diagnostics);
    }
    out.extend(stream.finish().unwrap().chunks);
    let collected = collect_q(out);
    assert_eq!(
        collected.choices[0].message.reasoning_content,
        Some(Some("secret".into()))
    );
    assert!(
        !collected.choices[0]
            .message
            .content
            .as_deref()
            .unwrap_or("")
            .contains("secret")
    );
    let json = serde_json::to_string(&collected).unwrap();
    assert_eq!(
        collected.choices[0]
            .message
            .reasoning_details
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()[0]
            .signature,
        Some(Some("opaque".into()))
    );
    assert!(
        !collected.choices[0]
            .message
            .content
            .as_deref()
            .unwrap_or("")
            .contains("opaque")
    );
    assert!(!json.contains("DROP"));
    assert!(json.contains("KEEP"));
}
#[test]
fn missing_terminals_malformed_arguments_and_limits_poison() {
    let mut stream = ChatToClaudeStream::new(context(), flow(), Default::default()).unwrap();
    assert!(
        stream
            .push(chat(
                json!({"tool_calls":[{"index":0,"function":{"name":"f","arguments":""}}]}),
                Some("tool_calls"),
                Some(q_usage())
            ))
            .is_err()
    );
    assert!(stream.push_done().is_err());
    assert!(stream.finish().is_err());
    let mut stream = ChatToClaudeStream::new(context(), flow(), Default::default()).unwrap();
    stream
        .push(chat(json!({"content":"x"}), Some("stop"), Some(q_usage())))
        .unwrap();
    assert!(stream.finish().is_err());
    let events = synthesize_claude_stream(
        source_reply(json!([{"type":"text","text":"x"}]), "end_turn"),
        Default::default(),
    )
    .unwrap()
    .value;
    let mut stream = ClaudeToChatStream::new(
        ClaudeToChatContext { created: 7 },
        flow(),
        Default::default(),
    )
    .unwrap();
    for e in events
        .into_iter()
        .filter(|e| !matches!(e, s::StreamEvent::MessageStop(_)))
    {
        stream.push(e).unwrap();
    }
    assert!(stream.finish().is_err());
    let mut stream = ClaudeToChatStream::new(
        ClaudeToChatContext { created: 7 },
        flow(),
        StreamLimits { max_bytes: 1 },
    )
    .unwrap();
    let event = serde_json::from_value(json!({"type":"ping"})).unwrap();
    assert_eq!(
        stream.push(event).unwrap_err().kind(),
        TransformErrorKind::Limit
    );
    assert!(stream.finish().is_err());
}
#[test]
fn caller_final_usage_is_required_if_source_omits_it() {
    let mut stream = ChatToClaudeStream::new(context(), flow(), Default::default()).unwrap();
    let mut out = stream
        .push(chat(json!({"content":"x"}), Some("stop"), None))
        .unwrap()
        .value;
    stream.push_done().unwrap();
    out.extend(
        stream
            .finish_with_usage(Some(c_usage(3, 2)))
            .unwrap()
            .chunks,
    );
    assert_eq!(collect_c(out).usage.output_tokens, 2);
    let mut stream = ChatToClaudeStream::new(context(), flow(), Default::default()).unwrap();
    stream
        .push(chat(json!({"content":"x"}), Some("stop"), None))
        .unwrap();
    stream.push_done().unwrap();
    assert!(stream.finish().is_err());
}

#[test]
fn factual_cache_details_fill_omissions_and_conflicts_fail() {
    let mut input: c::Usage=serde_json::from_value(json!({"input_tokens":1,"output_tokens":0,"cache_read_input_tokens":2,"cache_creation_input_tokens":0})).unwrap();
    let mut stream = ChatToClaudeStream::new(
        ChatToClaudeContext {
            start_usage: Some(input.clone()),
        },
        flow(),
        Default::default(),
    )
    .unwrap();
    let mut out = stream
        .push(chat(
            json!({"content":"x"}),
            Some("stop"),
            Some(json!({"prompt_tokens":3,"completion_tokens":2,"total_tokens":5})),
        ))
        .unwrap()
        .value;
    stream.push_done().unwrap();
    out.extend(stream.finish().unwrap().chunks);
    let usage = collect_c(out).usage;
    assert_eq!(usage.input_tokens, 1);
    assert_eq!(usage.cache_read_input_tokens, Some(Some(2)));
    assert!(usage.output_tokens_details.is_none());
    input.input_tokens = 3;
    input.cache_read_input_tokens = Some(Some(0));
    let mut stream = ChatToClaudeStream::new(
        ChatToClaudeContext {
            start_usage: Some(input),
        },
        flow(),
        Default::default(),
    )
    .unwrap();
    stream.push(chat(json!({"content":"x"}),Some("stop"),Some(json!({"prompt_tokens":3,"completion_tokens":2,"total_tokens":5,"prompt_tokens_details":{"cached_tokens":2,"cache_write_tokens":0}})))).unwrap();
    stream.push_done().unwrap();
    assert!(stream.finish().is_err());
}

#[test]
fn actual_usage_facts_cannot_replace_conflicting_native_cache_or_reasoning() {
    for kind in 0..2 {
        let mut stream = ChatToClaudeStream::new(context(), flow(), Default::default()).unwrap();
        stream
            .push(chat(json!({"content":"x"}), Some("stop"), Some(q_usage())))
            .unwrap();
        stream.push_done().unwrap();
        let mut facts = c_usage(3, 2);
        if kind == 0 {
            facts.input_tokens = 2;
            facts.cache_read_input_tokens = Some(Some(1));
        } else {
            facts.output_tokens_details = Some(Some(
                serde_json::from_value(json!({"thinking_tokens":1})).unwrap(),
            ));
        }
        assert!(stream.finish_with_usage(Some(facts)).is_err());
    }
}

#[test]
fn chat_refusal_marker_with_stop_is_a_claude_refusal_and_preserves_channel_order() {
    let source = vec![
        chat(json!({"refusal":"declined"}), None, None),
        chat(json!({"content":"context"}), Some("stop"), Some(q_usage())),
    ];
    let mut stream = ChatToClaudeStream::new(context(), flow(), Default::default()).unwrap();
    let mut out = Vec::new();
    for chunk in source.iter().cloned() {
        out.extend(stream.push(chunk).unwrap().value);
    }
    stream.push_done().unwrap();
    out.extend(stream.finish().unwrap().chunks);
    let actual = collect_c(out);
    assert_eq!(actual.stop_reason, c::StopReason::Refusal);
    let expected =
        claude_chat::openai_response_to_claude(&collect_q(source), "m", &Default::default())
            .unwrap()
            .value;
    assert_eq!(actual, expected);
}

#[test]
fn late_response_id_does_not_change_emitted_claude_identity() {
    let mut stream = ChatToClaudeStream::new(context(), flow(), Default::default()).unwrap();
    let mut first = chat(json!({"content":"a"}), None, None);
    first.id.clear();
    let mut out = stream.push(first).unwrap().value;
    let s::StreamEvent::MessageStart(start) = &out[0] else {
        panic!("expected start")
    };
    let emitted = start.message.id.clone();
    assert!(emitted.starts_with("msg_"));
    out.extend(
        stream
            .push(chat(json!({"content":"b"}), Some("stop"), Some(q_usage())))
            .unwrap()
            .value,
    );
    stream.push_done().unwrap();
    out.extend(stream.finish().unwrap().chunks);
    assert_eq!(collect_c(out).id, emitted);
}

#[test]
fn source_errors_and_unsupported_server_state_never_finish_successfully() {
    let mut stream = ClaudeToChatStream::new(
        ClaudeToChatContext { created: 7 },
        flow(),
        Default::default(),
    )
    .unwrap();
    let error = serde_json::from_value(
        json!({"type":"error","error":{"type":"overloaded_error","message":"busy"}}),
    )
    .unwrap();
    assert!(stream.push(error).is_err());
    assert!(stream.finish().is_err());
    let mut source =
        synthesize_claude_stream(source_reply(json!([]), "end_turn"), Default::default())
            .unwrap()
            .value;
    let mut stream = ClaudeToChatStream::new(
        ClaudeToChatContext { created: 7 },
        flow(),
        Default::default(),
    )
    .unwrap();
    stream.push(source.remove(0)).unwrap();
    let event=serde_json::from_value(json!({"type":"content_block_start","index":0,"content_block":{"type":"server_tool_use","id":"tool","name":"web_search","input":{}}})).unwrap();
    assert!(stream.push(event).is_ok());
    assert!(stream.finish().is_err());
    let mut stream = ChatToClaudeStream::new(context(), flow(), Default::default()).unwrap();
    let mut source = chat(json!({"content":"x"}), Some("stop"), Some(q_usage()));
    source.choices[0].index = 1;
    assert!(stream.push(source).is_err());
    assert!(stream.push_done().is_err());
}

#[test]
fn final_fact_extensions_are_removed_before_bounds_and_breakdown_has_real_total() {
    let mut stream = ChatToClaudeStream::new(
        ChatToClaudeContext::default(),
        flow(),
        StreamLimits { max_bytes: 8192 },
    )
    .unwrap();
    assert!(
        stream
            .push(chat(json!({"content":"x"}), Some("stop"), None))
            .unwrap()
            .value
            .is_empty()
    );
    stream.push_done().unwrap();
    let facts=serde_json::from_value(json!({"input_tokens":1,"output_tokens":2,"cache_read_input_tokens":0,"cache_creation":{"ephemeral_1h_input_tokens":1,"ephemeral_5m_input_tokens":1},"noise":"x".repeat(20_000)})).unwrap();
    let out = stream.finish_with_usage(Some(facts)).unwrap().chunks;
    let result = collect_c(out);
    assert_eq!(result.usage.cache_creation_input_tokens, Some(Some(2)));
    assert!(!serde_json::to_string(&result).unwrap().contains("noise"));
}

#[test]
fn claude_finish_cannot_return_a_target_chat_tool_finish_conflict() {
    for (content, stop) in [
        (
            json!([{"type":"tool_use","id":"toolu_x","name":"f","input":{}}]),
            "end_turn",
        ),
        (json!([{"type":"text","text":"x"}]), "tool_use"),
    ] {
        let source = source_reply(content, stop);
        let events = synthesize_claude_stream(source, Default::default())
            .unwrap()
            .value;
        let mut converter = ClaudeToChatStream::new(
            ClaudeToChatContext { created: 7 },
            flow(),
            Default::default(),
        )
        .unwrap();
        for event in events {
            converter.push(event).unwrap();
        }
        assert!(
            converter.finish().is_err(),
            "native target Chat must reject contradictory tool finish"
        );
    }
}
#[test]
fn final_usage_supplement_keeps_known_initial_cache_facts_before_native_comparison() {
    let initial: c::Usage = serde_json::from_value(json!({"input_tokens":3,"output_tokens":0,"cache_creation_input_tokens":1,"cache_read_input_tokens":2})).unwrap();
    let native = json!({"prompt_tokens":6,"completion_tokens":2,"total_tokens":8,"prompt_tokens_details":{"cached_tokens":2}});
    let mut converter = ChatToClaudeStream::new(
        ChatToClaudeContext {
            start_usage: Some(initial),
        },
        flow(),
        Default::default(),
    )
    .unwrap();
    let mut output = converter
        .push(chat(json!({"content":"x"}), None, None))
        .unwrap()
        .value;
    output.extend(
        converter
            .push(chat(json!({}), Some("stop"), Some(native)))
            .unwrap()
            .value,
    );
    converter.push_done().unwrap();
    output.extend(
        converter
            .finish_with_usage(Some(c::Usage::builder(3, 2).build()))
            .unwrap()
            .chunks,
    );
    let collected = collect_c(output);
    assert_eq!(collected.usage.input_tokens, 3);
    assert_eq!(collected.usage.cache_read_input_tokens, Some(Some(2)));
    assert_eq!(collected.usage.cache_creation_input_tokens, Some(Some(1)));
}
#[test]
fn inconsistent_initial_cache_facts_fail_before_any_source_event() {
    let usage: c::Usage = serde_json::from_value(json!({"input_tokens":3,"output_tokens":0,"cache_creation_input_tokens":1,"cache_creation":{"ephemeral_1h_input_tokens":1,"ephemeral_5m_input_tokens":1}})).unwrap();
    assert!(
        ChatToClaudeStream::new(
            ChatToClaudeContext {
                start_usage: Some(usage)
            },
            flow(),
            Default::default()
        )
        .is_ok()
    );
}

#[test]
fn interleaved_claude_text_blocks_preserve_native_block_order_with_early_text() {
    let expected = source_reply(
        json!([{"type":"text","text":"ac"},{"type":"text","text":"b"}]),
        "end_turn",
    );
    let mut native = synthesize_claude_stream(expected.clone(), Default::default())
        .unwrap()
        .value;
    let stop = native.pop().unwrap();
    let terminal = native.pop().unwrap();
    let mut events = vec![native.remove(0)];
    for event in [
        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"a"}}),
        json!({"type":"content_block_start","index":1,"content_block":{"type":"text","text":"b"}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"c"}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"content_block_stop","index":1}),
    ] {
        events.push(serde_json::from_value(event).unwrap());
    }
    events.extend([terminal, stop]);
    assert_eq!(collect_c(events.clone()), expected);
    let mut converter = ClaudeToChatStream::new(
        ClaudeToChatContext { created: 7 },
        flow(),
        Default::default(),
    )
    .unwrap();
    let mut output = Vec::new();
    for (index, event) in events.into_iter().enumerate() {
        let mapped = converter.push(event).unwrap().value;
        if index == 2 {
            assert!(mapped.iter().any(|v| v.choices.iter().any(|v| {
                v.delta
                    .content
                    .as_ref()
                    .and_then(Option::as_ref)
                    .is_some_and(|v| v == "a")
            })));
        }
        output.extend(mapped);
    }
    output.extend(converter.finish().unwrap().chunks);
    assert_eq!(
        collect_q(output).choices[0].message.content.as_deref(),
        Some("acb")
    );
}

#[test]
fn empty_claude_json_deltas_do_not_erase_a_known_empty_tool_object() {
    let response = source_reply(
        json!([{"type":"tool_use","id":"tool","name":"f","input":{}}]),
        "tool_use",
    );
    let mut source = synthesize_claude_stream(response, Default::default())
        .unwrap()
        .value;
    for event in &mut source {
        if let s::StreamEvent::ContentBlockDelta(event) = event
            && let s::ContentBlockDelta::InputJson(delta) = &mut event.delta
        {
            delta.partial_json.clear();
        }
    }
    let mut converter = ClaudeToChatStream::new(
        ClaudeToChatContext { created: 7 },
        flow(),
        Default::default(),
    )
    .unwrap();
    let mut output = Vec::new();
    for event in source {
        output.extend(converter.push(event).unwrap().value);
    }
    output.extend(converter.finish().unwrap().chunks);
    let value = serde_json::to_value(collect_q(output)).unwrap();
    assert_eq!(
        value["choices"][0]["message"]["tool_calls"][0]["function"]["arguments"],
        "{}"
    );
}
