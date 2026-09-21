use gproxy_protocol::{
    Dialect,
    transform::{
        TransformErrorKind,
        generate::{
            claude_gemini::{self as pair, ClaudeGeminiUsageFacts as Facts, stream::*},
            stream::{claude::ClaudeStreamCollector, gemini::GeminiStreamCollector},
        },
        identity::{
            IdNamespace, IdentityFlow, IdentityRole, KnownIdPrefix, SourceIdentity, TargetIdPolicy,
        },
    },
    wire::{
        claude::{generate_content as c, stream as s},
        gemini as g,
    },
};
use serde_json::{Value, json};
fn ce(value: Value) -> s::StreamEvent {
    serde_json::from_value(value).unwrap()
}
fn ge(value: Value) -> g::GenerateContentResponseBody {
    serde_json::from_value(value).unwrap()
}
fn flow() -> IdentityFlow {
    IdentityFlow::new(IdNamespace::with_bytes([31; 16]))
}
fn cp() -> TargetIdPolicy {
    TargetIdPolicy::new(Dialect::Claude)
        .with_generated_prefix(IdentityRole::Response, KnownIdPrefix::Message)
        .with_generated_prefix(IdentityRole::ToolCall, KnownIdPrefix::Tool)
}
fn initial() -> c::Usage {
    serde_json::from_value(json!({"input_tokens":1,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens_details":{"thinking_tokens":0}})).unwrap()
}
fn fixed() -> Facts {
    Facts {
        cache_creation_input_tokens: Some(0),
        cache_read_input_tokens: Some(0),
        thinking_tokens: None,
    }
}
fn gc() -> GeminiToClaudeContext {
    GeminiToClaudeContext {
        usage: Some(initial()),
        ..Default::default()
    }
}
fn start() -> s::StreamEvent {
    ce(
        json!({"type":"message_start","message":{"type":"message","id":"msg-source","model":"claude","role":"assistant","content":[],"usage":initial()}}),
    )
}
fn block(index: i64, value: Value) -> s::StreamEvent {
    ce(json!({"type":"content_block_start","index":index,"content_block":value}))
}
fn delta(index: i64, value: Value) -> s::StreamEvent {
    ce(json!({"type":"content_block_delta","index":index,"delta":value}))
}
fn block_stop(index: i64) -> s::StreamEvent {
    ce(json!({"type":"content_block_stop","index":index}))
}
fn terminal(stop: &str, output: i64, thinking: Option<i64>) -> Vec<s::StreamEvent> {
    let mut usage = json!({"output_tokens":output});
    if let Some(n) = thinking {
        usage["output_tokens_details"] = json!({"thinking_tokens":n});
    }
    let mut value = json!({"stop_reason":stop});
    if stop == "stop_sequence" {
        value["stop_sequence"] = json!("END");
    }
    vec![
        ce(json!({"type":"message_delta","delta":value,"usage":usage})),
        ce(json!({"type":"message_stop"})),
    ]
}
fn events(blocks: Vec<Value>, stop: &str) -> Vec<s::StreamEvent> {
    let mut out = vec![start()];
    for (index, value) in blocks.into_iter().enumerate() {
        out.extend([block(index as i64, value), block_stop(index as i64)]);
    }
    out.extend(terminal(stop, 4, Some(1)));
    out
}
fn gu(output: i64, thinking: i64) -> Value {
    json!({"promptTokenCount":1,"toolUsePromptTokenCount":0,"cachedContentTokenCount":0,"candidatesTokenCount":output-thinking,"thoughtsTokenCount":thinking,"totalTokenCount":1+output})
}
fn gparts(parts: Value) -> g::GenerateContentResponseBody {
    ge(
        json!({"responseId":"g-source","modelVersion":"gemini","candidates":[{"index":0,"content":{"role":"model","parts":parts}}]}),
    )
}
fn gend(reason: &str) -> g::GenerateContentResponseBody {
    ge(json!({"candidates":[{"index":0,"finishReason":reason}],"usageMetadata":gu(4,1)}))
}
fn collect_c(events: Vec<s::StreamEvent>) -> c::GenerateContentResponseBody {
    let mut native = ClaudeStreamCollector::new(Default::default());
    for event in events {
        native.push(event).unwrap();
    }
    native.finish().unwrap().value
}
fn collect_g(events: Vec<g::GenerateContentResponseBody>) -> g::GenerateContentResponseBody {
    let mut native = GeminiStreamCollector::new(Default::default());
    for event in events {
        native.push(event).unwrap();
    }
    native.finish().unwrap().value
}
fn normalize(mut body: g::GenerateContentResponseBody) -> Value {
    // Compare complete native objects, only coalescing adjacent text fragments.
    for candidate in body.candidates.iter_mut().flatten() {
        if let Some(parts) = candidate.content.as_mut().and_then(|v| v.parts.as_mut()) {
            let mut output: Vec<g::Part> = Vec::new();
            for part in std::mem::take(parts) {
                if part.text.is_some()
                    && let Some(previous) = output.last_mut()
                    && previous.text.is_some()
                    && previous.thought == part.thought
                {
                    previous
                        .text
                        .as_mut()
                        .unwrap()
                        .push_str(part.text.as_ref().unwrap());
                } else {
                    output.push(part);
                }
            }
            *parts = output;
        }
    }
    serde_json::to_value(body).unwrap()
}
fn c_to_g(source: Vec<s::StreamEvent>) -> g::GenerateContentResponseBody {
    let complete = collect_c(source.clone());
    let mut converter =
        ClaudeToGeminiStream::new(Default::default(), flow(), Default::default()).unwrap();
    let mut output = Vec::new();
    for event in source {
        output.extend(converter.push(event).unwrap().value);
    }
    let end = converter.finish().unwrap();
    output.extend(end.chunks);
    let actual = collect_g(output);
    let expected = pair::claude_to_gemini_response(
        complete,
        Default::default(),
        &mut end.identities.clone(),
        &TargetIdPolicy::new(Dialect::Gemini),
    )
    .unwrap()
    .value;
    assert_eq!(normalize(actual.clone()), normalize(expected));
    actual
}
fn g_to_c(
    source: Vec<g::GenerateContentResponseBody>,
    context: GeminiToClaudeContext,
    facts: Facts,
) -> c::GenerateContentResponseBody {
    let complete = collect_g(source.clone());
    let fallback = context.model.clone();
    let mut converter = GeminiToClaudeStream::new(context, flow(), Default::default()).unwrap();
    let mut output = Vec::new();
    for event in source {
        output.extend(converter.push(event).unwrap().value);
    }
    let end = converter.finish().unwrap();
    output.extend(end.chunks);
    let actual = collect_c(output);
    let expected = pair::gemini_to_claude_response(
        complete,
        fallback,
        facts,
        &mut end.identities.clone(),
        &cp(),
    )
    .unwrap()
    .value;
    assert_eq!(actual, expected);
    actual
}
#[test]
fn initial_claude_text_thinking_and_two_same_name_tool_inputs_match_complete_native_pair() {
    let source = events(
        vec![
            json!({"type":"text","text":"intro"}),
            json!({"type":"thinking","thinking":"reason","signature":"opaque"}),
            json!({"type":"tool_use","id":"call-a","name":"same","input":{"x":1}}),
            json!({"type":"text","text":"after"}),
            json!({"type":"tool_use","id":"call-b","name":"same","input":{"x":2}}),
        ],
        "tool_use",
    );
    let mut probe =
        ClaudeToGeminiStream::new(Default::default(), flow(), Default::default()).unwrap();
    probe.push(source[0].clone()).unwrap();
    let early = probe.push(source[1].clone()).unwrap();
    assert_eq!(
        early.value[0].candidates.as_ref().unwrap()[0]
            .content
            .as_ref()
            .unwrap()
            .parts
            .as_ref()
            .unwrap()[0]
            .text
            .as_deref(),
        Some("intro")
    );
    let actual = c_to_g(source);
    let parts = actual
        .candidates
        .unwrap()
        .remove(0)
        .content
        .unwrap()
        .parts
        .unwrap();
    let calls: Vec<_> = parts
        .iter()
        .filter_map(|v| v.function_call.as_ref())
        .collect();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].args.as_ref().unwrap()["x"], 1);
    assert_eq!(calls[1].args.as_ref().unwrap()["x"], 2);
    assert_ne!(calls[0].id, calls[1].id);
}
#[test]
fn interleaved_claude_blocks_preserve_order_and_json_fragments() {
    let mut source = vec![
        start(),
        block(0, json!({"type":"text","text":"a"})),
        block(
            1,
            json!({"type":"tool_use","id":"call","name":"f","input":{}}),
        ),
        block(2, json!({"type":"text","text":"b"})),
        delta(
            1,
            json!({"type":"input_json_delta","partial_json":"{\"x\":"}),
        ),
        delta(1, json!({"type":"input_json_delta","partial_json":"1}"})),
        block_stop(1),
        delta(0, json!({"type":"text_delta","text":"c"})),
        block_stop(0),
        block_stop(2),
    ];
    source.extend(terminal("tool_use", 4, Some(1)));
    let value = normalize(c_to_g(source));
    let parts = &value["candidates"][0]["content"]["parts"];
    assert_eq!(parts[0]["text"], "ac");
    assert_eq!(parts[1]["functionCall"]["args"], json!({"x":1}));
    assert_eq!(parts[2]["text"], "b");
}
#[test]
fn redacted_blocks_are_omitted_and_empty_text_still_has_native_content() {
    let actual = c_to_g(events(
        vec![
            json!({"type":"redacted_thinking","data":"opaque"}),
            json!({"type":"text","text":""}),
        ],
        "end_turn",
    ));
    assert_eq!(
        actual.candidates.unwrap()[0]
            .content
            .as_ref()
            .unwrap()
            .parts
            .as_ref()
            .unwrap()
            .len(),
        1
    );
    let actual = c_to_g(events(
        vec![json!({"type":"redacted_thinking","data":"opaque"})],
        "end_turn",
    ));
    assert!(
        actual.candidates.unwrap()[0]
            .content
            .as_ref()
            .unwrap()
            .parts
            .as_ref()
            .unwrap()
            .is_empty()
    );
}
#[test]
fn actual_web_and_search_citations_arrive_as_terminal_gemini_metadata() {
    let mut source = vec![
        start(),
        block(
            0,
            json!({"type":"text","text":"evidence","citations":[{"type":"web_search_result_location","cited_text":"evidence","encrypted_index":"opaque-index","url":"https://one.test","title":"One"}]}),
        ),
        delta(
            0,
            json!({"type":"citations_delta","citation":{"type":"search_result_location","cited_text":"evidence","end_block_index":1,"search_result_index":0,"source":"https://two.test","start_block_index":0,"title":"Two"}}),
        ),
        block_stop(0),
    ];
    source.extend(terminal("end_turn", 4, Some(1)));
    let actual = c_to_g(source);
    let sources = actual
        .candidates
        .unwrap()
        .remove(0)
        .citation_metadata
        .unwrap()
        .citation_sources
        .unwrap();
    assert_eq!(sources[0].uri.as_deref(), Some("https://one.test"));
    assert_eq!(sources[1].uri.as_deref(), Some("https://two.test"));
}
#[test]
fn repeated_gemini_text_and_thought_marked_functions_are_not_dropped() {
    let source = vec![
        gparts(
            json!([{"text":"ha"},{"text":"ha"},{"text":"private thought","thought":true,"thoughtSignature":"opaque"},{"functionCall":{"name":"same","id":"a","args":{"x":1}},"thought":true},{"functionCall":{"name":"same","id":"b","args":{"x":2}}}]),
        ),
        gend("STOP"),
    ];
    let actual = g_to_c(source, gc(), fixed());
    assert_eq!(actual.stop_reason, c::StopReason::ToolUse);
    assert_eq!(actual.content.len(), 4);
    let raw = serde_json::to_string(&actual).unwrap();
    assert!(!raw.contains("private thought"));
    assert!(!raw.contains("opaque"));
}
#[test]
fn missing_response_and_tool_ids_emit_stable_aliases_before_late_native_id() {
    let first = ge(
        json!({"modelVersion":"gemini","candidates":[{"content":{"role":"model","parts":[{"text":"early"},{"functionCall":{"name":"same","args":{}}},{"functionCall":{"name":"same","args":{}}}]}}]}),
    );
    let mut converter = GeminiToClaudeStream::new(gc(), flow(), Default::default()).unwrap();
    let first = converter.push(first).unwrap().value;
    let s::StreamEvent::MessageStart(start) = &first[0] else {
        panic!("start must be emitted without a native ID")
    };
    let alias = start.message.id.clone();
    assert!(alias.starts_with("msg_"));
    assert!(
        first
            .iter()
            .any(|v| matches!(v, s::StreamEvent::ContentBlockDelta(_)))
    );
    let last = ge(
        json!({"responseId":"late","candidates":[{"index":0,"finishReason":"STOP"}],"usageMetadata":gu(4,1)}),
    );
    let mut output = first;
    output.extend(converter.push(last).unwrap().value);
    let mut end = converter.finish().unwrap();
    output.extend(end.chunks);
    let actual = collect_c(output);
    assert_eq!(actual.id, alias);
    let raw = serde_json::to_string(&actual).unwrap();
    assert_eq!(raw.matches("toolu_").count(), 2);
    let rebound = end
        .identities
        .resolve_or_allocate(
            IdentityRole::Response,
            SourceIdentity::new(Dialect::Gemini, Some("late".into()), 0),
            &cp(),
        )
        .unwrap();
    assert_eq!(rebound.emitted_id, alias);
}
#[test]
fn missing_start_facts_wait_then_release_at_actual_usage_snapshot() {
    let mut converter = GeminiToClaudeStream::new(
        GeminiToClaudeContext {
            facts: fixed(),
            ..Default::default()
        },
        flow(),
        Default::default(),
    )
    .unwrap();
    assert!(
        converter
            .push(gparts(json!([{"text":"held"}])))
            .unwrap()
            .value
            .is_empty()
    );
    let snapshot = ge(json!({"usageMetadata":gu(3,1)}));
    let released = converter.push(snapshot).unwrap().value;
    assert!(
        released
            .iter()
            .any(|v| matches!(v, s::StreamEvent::ContentBlockDelta(_)))
    );
    let mut output = released;
    output.extend(converter.push(gend("STOP")).unwrap().value);
    output.extend(converter.finish().unwrap().chunks);
    assert_eq!(collect_c(output).usage.output_tokens, 4);
}
#[test]
fn native_model_overrides_unemitted_fallback_but_late_contradiction_poisons() {
    let mut context = gc();
    context.model = Some("fallback".into());
    let mut converter = GeminiToClaudeStream::new(context, flow(), Default::default()).unwrap();
    let out = converter.push(gparts(json!([{"text":"x"}]))).unwrap().value;
    let s::StreamEvent::MessageStart(start) = &out[0] else {
        unreachable!()
    };
    assert_eq!(start.message.model, "gemini");
    let bad = ge(json!({"modelVersion":"different"}));
    assert!(converter.push(bad).is_err());
    assert!(converter.push(gend("STOP")).is_err());
    assert!(converter.finish().is_err());
    let mut context = gc();
    context.model = Some("fallback".into());
    let mut converter = GeminiToClaudeStream::new(context, flow(), Default::default()).unwrap();
    converter
        .push(ge(
            json!({"candidates":[{"content":{"role":"model","parts":[{"text":"x"}]}}]}),
        ))
        .unwrap();
    assert!(
        converter
            .push(ge(json!({"modelVersion":"different"})))
            .is_err()
    );
}
#[test]
fn unsupported_valid_native_payloads_and_bad_roles_poison_all_future_operations() {
    for bad in [
        gparts(json!([{"inlineData":{"mimeType":"image/png","data":"AA=="}}])),
        ge(json!({"candidates":[{"content":{"role":"user","parts":[{"text":"bad"}]}}]})),
        gparts(json!([{"text":"bad","functionCall":{"name":"f"}}])),
    ] {
        let mut converter = GeminiToClaudeStream::new(gc(), flow(), Default::default()).unwrap();
        converter.push(gparts(json!([{"text":"good"}]))).unwrap();
        let wrong_role = bad.candidates.as_ref().is_some_and(|v| {
            v.iter()
                .any(|v| v.content.as_ref().and_then(|v| v.role.as_deref()) == Some("user"))
        });
        assert_eq!(converter.push(bad).is_err(), wrong_role);
        assert_eq!(converter.push(gend("STOP")).is_err(), wrong_role);
        assert_eq!(converter.finish().is_err(), wrong_role);
    }
    for bad in [
        json!({"type":"server_tool_use","id":"server","name":"web_search","input":{}}),
        json!({"type":"tool_use","id":"call","name":"f","input":{},"caller":{"type":"code_execution_20250825","tool_id":"server"}}),
    ] {
        let mut converter =
            ClaudeToGeminiStream::new(Default::default(), flow(), Default::default()).unwrap();
        converter.push(start()).unwrap();
        assert!(converter.push(block(0, bad)).is_ok());
        assert!(converter.push(block_stop(0)).is_ok());
        assert!(converter.finish().is_err());
    }
}
#[test]
fn output_aggregate_and_pending_caps_are_checked_before_retaining_unstarted_events() {
    let source = gparts(json!([{"text":"a"},{"text":"b"},{"text":"c"},{"text":"d"}]));
    let source_bytes = serde_json::to_vec(&source).unwrap().len();
    let limits = StreamLimits {
        max_bytes: source_bytes + 80,
        ..Default::default()
    };
    let mut converter = GeminiToClaudeStream::new(Default::default(), flow(), limits).unwrap();
    assert_eq!(
        converter.push(source).unwrap_err().kind(),
        TransformErrorKind::Limit
    );
    assert!(converter.finish().is_err());
    let limits = StreamLimits {
        max_pending: 1,
        ..Default::default()
    };
    let mut converter = GeminiToClaudeStream::new(Default::default(), flow(), limits).unwrap();
    assert_eq!(
        converter
            .push(gparts(json!([{"text":"x"}])))
            .unwrap_err()
            .kind(),
        TransformErrorKind::Limit
    );
}
#[test]
fn start_and_terminal_events_count_toward_output_limits() {
    let limits = StreamLimits {
        max_events: 3,
        ..Default::default()
    };
    let mut converter = GeminiToClaudeStream::new(gc(), flow(), limits).unwrap();
    assert_eq!(
        converter
            .push(gparts(json!([{"text":"x"}])))
            .unwrap_err()
            .kind(),
        TransformErrorKind::Limit
    );
    let limits = StreamLimits {
        max_events: 4,
        ..Default::default()
    };
    let mut converter = GeminiToClaudeStream::new(gc(), flow(), limits).unwrap();
    converter.push(gparts(json!([{"text":"x"}]))).unwrap();
    converter.push(gend("STOP")).unwrap();
    assert!(converter.finish().is_err());
}
#[test]
fn source_and_initial_fact_rest_are_removed_before_limits_but_tool_json_remains() {
    let mut context = gc();
    context
        .usage
        .as_mut()
        .unwrap()
        .rest
        .insert("large-extension".into(), json!("X".repeat(8192)));
    let limits = StreamLimits {
        max_bytes: 2048,
        ..Default::default()
    };
    let mut converter = GeminiToClaudeStream::new(context, flow(), limits).unwrap();
    let mut source = gparts(
        json!([{"functionCall":{"id":"c","name":"f","args":{"formal_extension":7}},"x-extension":"DROP"}]),
    );
    source
        .rest
        .insert("large-extension".into(), json!("X".repeat(8192)));
    let mut output = converter.push(source).unwrap().value;
    output.extend(converter.push(gend("STOP")).unwrap().value);
    output.extend(converter.finish().unwrap().chunks);
    let value = serde_json::to_string(&collect_c(output)).unwrap();
    assert!(value.contains("formal_extension"));
    assert!(!value.contains("DROP"));
    assert!(!value.contains("large-extension"));
}
#[test]
fn known_initial_cache_facts_fill_missing_native_details_and_real_remainder_is_derived() {
    for known_write in [true, false] {
        let mut usage:c::Usage=serde_json::from_value(json!({"input_tokens":3,"output_tokens":0,"cache_read_input_tokens":2,"output_tokens_details":{"thinking_tokens":0}})).unwrap();
        if known_write {
            usage.cache_creation_input_tokens = Some(Some(1));
        }
        let source = vec![
            gparts(json!([{"text":"x"}])),
            ge(
                json!({"candidates":[{"index":0,"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":6,"toolUsePromptTokenCount":0,"cachedContentTokenCount":2,"candidatesTokenCount":2,"thoughtsTokenCount":0,"totalTokenCount":8}}),
            ),
        ];
        let actual = g_to_c(
            source,
            GeminiToClaudeContext {
                usage: Some(usage),
                ..Default::default()
            },
            Facts {
                cache_creation_input_tokens: Some(1),
                cache_read_input_tokens: Some(2),
                thinking_tokens: None,
            },
        );
        assert_eq!(actual.usage.input_tokens, 3);
        assert_eq!(actual.usage.cache_creation_input_tokens, Some(Some(1)));
    }
}
#[test]
fn final_thinking_fact_is_distinct_from_initial_zero_and_inconsistent_cache_rejects() {
    let mut context = gc();
    context.facts.thinking_tokens = Some(1);
    let actual = g_to_c(
        vec![gparts(json!([{"text":"x"}])), gend("STOP")],
        context,
        Facts {
            thinking_tokens: Some(1),
            ..fixed()
        },
    );
    assert_eq!(
        actual
            .usage
            .output_tokens_details
            .unwrap()
            .unwrap()
            .thinking_tokens,
        1
    );
    let mut context = gc();
    context.usage.as_mut().unwrap().cache_creation = Some(Some(
        serde_json::from_value(
            json!({"ephemeral_1h_input_tokens":1,"ephemeral_5m_input_tokens":1}),
        )
        .unwrap(),
    ));
    assert!(GeminiToClaudeStream::new(context, flow(), Default::default()).is_ok());
    let mut context = gc();
    context.facts.cache_read_input_tokens = Some(2);
    assert!(GeminiToClaudeStream::new(context, flow(), Default::default()).is_ok());
}
#[test]
fn a_partial_initial_claude_thinking_counter_is_not_fabricated_as_final_zero() {
    for fact in [None, Some(2)] {
        let mut converter = ClaudeToGeminiStream::new(
            ClaudeToGeminiContext {
                usage: Facts {
                    thinking_tokens: fact,
                    ..Default::default()
                },
            },
            flow(),
            Default::default(),
        )
        .unwrap();
        let mut source = vec![
            start(),
            block(0, json!({"type":"text","text":"x"})),
            block_stop(0),
        ];
        source.extend(terminal("end_turn", 4, None));
        let mut output = Vec::new();
        for event in source {
            output.extend(converter.push(event).unwrap().value);
        }
        if fact.is_none() {
            assert_eq!(
                converter.finish().unwrap_err().kind(),
                TransformErrorKind::MissingMetadata
            );
        } else {
            output.extend(converter.finish().unwrap().chunks);
            let actual = collect_g(output).usage_metadata.unwrap();
            assert_eq!(actual.thoughts_token_count, Some(2));
            assert_eq!(actual.candidates_token_count, Some(2));
        }
    }
    let mut converter =
        ClaudeToGeminiStream::new(Default::default(), flow(), Default::default()).unwrap();
    converter.push(start()).unwrap();
    converter
        .push(terminal("end_turn", 2, Some(1)).remove(0))
        .unwrap();
    assert!(
        converter
            .push(terminal("end_turn", 3, Some(0)).remove(0))
            .is_ok()
    );
    assert!(converter.finish().is_err());
}
#[test]
fn all_supported_finish_mappings_have_complete_native_parity_and_invalid_tool_finishes_fail() {
    for stop in [
        "end_turn",
        "stop_sequence",
        "max_tokens",
        "model_context_window_exceeded",
        "refusal",
    ] {
        c_to_g(events(vec![json!({"type":"text","text":"x"})], stop));
    }
    for finish in [
        "STOP",
        "MAX_TOKENS",
        "SAFETY",
        "RECITATION",
        "LANGUAGE",
        "BLOCKLIST",
        "PROHIBITED_CONTENT",
        "SPII",
        "IMAGE_SAFETY",
        "IMAGE_PROHIBITED_CONTENT",
        "IMAGE_RECITATION",
    ] {
        g_to_c(
            vec![gparts(json!([{"text":"x"}])), gend(finish)],
            gc(),
            fixed(),
        );
    }
    for (blocks, stop) in [
        (
            vec![json!({"type":"tool_use","id":"c","name":"f","input":{}})],
            "end_turn",
        ),
        (vec![json!({"type":"text","text":"x"})], "tool_use"),
    ] {
        let mut converter =
            ClaudeToGeminiStream::new(Default::default(), flow(), Default::default()).unwrap();
        for event in events(blocks, stop) {
            converter.push(event).unwrap();
        }
        assert!(converter.finish().is_ok());
    }
}
#[test]
fn eof_blocked_prompts_and_missing_required_usage_never_become_success() {
    let mut converter =
        ClaudeToGeminiStream::new(Default::default(), flow(), Default::default()).unwrap();
    converter.push(start()).unwrap();
    assert!(converter.finish().is_err());
    let mut converter = GeminiToClaudeStream::new(gc(), flow(), Default::default()).unwrap();
    converter.push(gparts(json!([{"text":"x"}]))).unwrap();
    assert!(converter.finish().is_err());
    let mut converter =
        GeminiToClaudeStream::new(Default::default(), flow(), Default::default()).unwrap();
    converter
        .push(ge(json!({"promptFeedback":{"blockReason":"SAFETY"}})))
        .unwrap();
    assert!(converter.finish().is_err());
    let mut converter =
        GeminiToClaudeStream::new(Default::default(), flow(), Default::default()).unwrap();
    converter
        .push(ge(
            json!({"modelVersion":"gemini","candidates":[{"index":0,"finishReason":"STOP"}]}),
        ))
        .unwrap();
    assert!(converter.finish().is_err());
}

#[test]
fn final_thinking_facts_do_not_conflict_with_an_actual_earlier_usage_snapshot() {
    let mut converter = GeminiToClaudeStream::new(
        GeminiToClaudeContext {
            facts: Facts {
                thinking_tokens: Some(2),
                ..fixed()
            },
            ..Default::default()
        },
        flow(),
        Default::default(),
    )
    .unwrap();
    let mut first = gparts(json!([{"text":"early"}]));
    first.usage_metadata = Some(serde_json::from_value(gu(1, 0)).unwrap());
    let mut output = converter.push(first).unwrap().value;
    assert!(matches!(
        output.first(),
        Some(s::StreamEvent::MessageStart(_))
    ));
    let last =
        ge(json!({"candidates":[{"index":0,"finishReason":"STOP"}],"usageMetadata":gu(4,2)}));
    output.extend(converter.push(last).unwrap().value);
    output.extend(converter.finish().unwrap().chunks);
    assert_eq!(
        collect_c(output)
            .usage
            .output_tokens_details
            .unwrap()
            .unwrap()
            .thinking_tokens,
        2
    );
}
#[test]
fn stale_gemini_thinking_snapshot_is_not_compared_as_a_final_counter() {
    let mut converter = GeminiToClaudeStream::new(gc(), flow(), Default::default()).unwrap();
    let mut first = gparts(json!([{"text":"early"}]));
    first.usage_metadata = Some(serde_json::from_value(gu(1, 0)).unwrap());
    let mut output = converter.push(first).unwrap().value;
    let last = ge(
        json!({"candidates":[{"index":0,"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":1,"toolUsePromptTokenCount":0,"cachedContentTokenCount":0,"candidatesTokenCount":3,"totalTokenCount":5}}),
    );
    output.extend(converter.push(last).unwrap().value);
    output.extend(converter.finish().unwrap().chunks);
    assert_eq!(
        collect_c(output)
            .usage
            .output_tokens_details
            .unwrap()
            .unwrap()
            .thinking_tokens,
        1
    );
}

#[test]
fn final_fact_can_release_pending_output_with_an_actual_identity_fallback() {
    let mut converter = GeminiToClaudeStream::new(
        GeminiToClaudeContext {
            model: Some("gemini".into()),
            response_id: Some("actual-id-fact".into()),
            facts: Facts {
                thinking_tokens: Some(2),
                ..fixed()
            },
            ..Default::default()
        },
        flow(),
        Default::default(),
    )
    .unwrap();
    assert!(
        converter
            .push(ge(
                json!({"candidates":[{"content":{"role":"model","parts":[{"text":"held"}]}}]})
            ))
            .unwrap()
            .value
            .is_empty()
    );
    assert!(converter.push(ge(json!({"candidates":[{"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":1,"toolUsePromptTokenCount":0,"cachedContentTokenCount":0,"totalTokenCount":5}}))).unwrap().value.is_empty());
    let end = converter.finish().unwrap();
    let s::StreamEvent::MessageStart(start) = &end.chunks[0] else {
        unreachable!()
    };
    assert_eq!(start.message.id, "actual-id-fact");
    assert_eq!(start.message.usage.output_tokens, 4);
    let actual = collect_c(end.chunks);
    assert_eq!(
        actual
            .usage
            .output_tokens_details
            .unwrap()
            .unwrap()
            .thinking_tokens,
        2
    );
}
#[test]
fn ambiguous_stale_components_and_fresh_inconsistent_counters_are_not_guessed() {
    for final_usage in [
        json!({"promptTokenCount":1,"toolUsePromptTokenCount":0,"cachedContentTokenCount":0,"totalTokenCount":5}),
        json!({"promptTokenCount":1,"toolUsePromptTokenCount":0,"cachedContentTokenCount":0,"candidatesTokenCount":3,"thoughtsTokenCount":0,"totalTokenCount":5}),
    ] {
        let has_split = final_usage.get("candidatesTokenCount").is_some();
        let mut converter = GeminiToClaudeStream::new(gc(), flow(), Default::default()).unwrap();
        let mut first = gparts(json!([{"text":"x"}]));
        first.usage_metadata = Some(serde_json::from_value(gu(1, 0)).unwrap());
        converter.push(first).unwrap();
        converter
            .push(ge(
                json!({"candidates":[{"finishReason":"STOP"}],"usageMetadata":final_usage}),
            ))
            .unwrap();
        assert_eq!(converter.finish().is_ok(), has_split);
    }
}
#[test]
fn fixed_initial_cache_breakdown_remains_in_the_canonical_target() {
    let mut initial = initial();
    let breakdown = serde_json::from_value(
        json!({"ephemeral_1h_input_tokens":1,"ephemeral_5m_input_tokens":1}),
    )
    .unwrap();
    initial.cache_creation_input_tokens = Some(Some(2));
    initial.cache_creation = Some(Some(breakdown));
    let source = vec![
        gparts(json!([{"text":"x"}])),
        ge(
            json!({"candidates":[{"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":3,"toolUsePromptTokenCount":0,"cachedContentTokenCount":0,"candidatesTokenCount":3,"thoughtsTokenCount":1,"totalTokenCount":7}}),
        ),
    ];
    let complete = collect_g(source.clone());
    let mut converter = GeminiToClaudeStream::new(
        GeminiToClaudeContext {
            usage: Some(initial.clone()),
            ..Default::default()
        },
        flow(),
        Default::default(),
    )
    .unwrap();
    let mut output = Vec::new();
    for event in source {
        output.extend(converter.push(event).unwrap().value);
    }
    let end = converter.finish().unwrap();
    output.extend(end.chunks);
    let actual = collect_c(output);
    let mut expected = pair::gemini_to_claude_response(
        complete,
        None,
        Facts {
            cache_creation_input_tokens: Some(2),
            ..fixed()
        },
        &mut end.identities.clone(),
        &cp(),
    )
    .unwrap()
    .value;
    expected.usage.cache_creation = initial.cache_creation;
    assert_eq!(actual, expected);
}
#[test]
fn native_failures_and_each_bound_remain_terminal_without_tool_limits_blocking_text() {
    let mut converter =
        ClaudeToGeminiStream::new(Default::default(), flow(), Default::default()).unwrap();
    converter.push(start()).unwrap();
    assert!(
        converter
            .push(ce(
                json!({"type":"error","error":{"type":"overloaded_error","message":"busy"}})
            ))
            .is_err()
    );
    assert!(
        converter
            .push(block(0, json!({"type":"text","text":"x"})))
            .is_err()
    );
    assert!(converter.finish().is_err());
    let mut converter = GeminiToClaudeStream::new(gc(), flow(), Default::default()).unwrap();
    let mut first = gparts(json!([{"text":"x"}]));
    first.usage_metadata = Some(serde_json::from_value(gu(2, 1)).unwrap());
    converter.push(first).unwrap();
    assert!(
        converter
            .push(ge(json!({"usageMetadata":gu(1,0)})))
            .is_err()
    );
    assert!(converter.finish().is_err());
    let mut converter = GeminiToClaudeStream::new(gc(), flow(), Default::default()).unwrap();
    assert!(converter.push(ge(json!({"modelVersion":"g","candidates":[{"content":{"role":"user","parts":[{"text":"x"}]}}]}))).is_err());
    assert!(converter.finish().is_err());
    let mut converter = ClaudeToGeminiStream::new(
        Default::default(),
        flow(),
        StreamLimits {
            max_pending: 3,
            ..Default::default()
        },
    )
    .unwrap();
    converter.push(start()).unwrap();
    converter
        .push(block(0, json!({"type":"text","text":""})))
        .unwrap();
    assert_eq!(
        converter
            .push(block(1, json!({"type":"text","text":"four"})))
            .unwrap_err()
            .kind(),
        TransformErrorKind::Limit
    );
    assert!(converter.finish().is_err());
    let limits = StreamLimits {
        max_tools: 0,
        ..Default::default()
    };
    let mut converter = ClaudeToGeminiStream::new(Default::default(), flow(), limits).unwrap();
    for event in events(vec![json!({"type":"text","text":"x"})], "end_turn") {
        converter.push(event).unwrap();
    }
    converter.finish().unwrap();
    let mut converter = GeminiToClaudeStream::new(gc(), flow(), limits).unwrap();
    converter.push(gparts(json!([{"text":"x"}]))).unwrap();
    converter.push(gend("STOP")).unwrap();
    converter.finish().unwrap();
}
