use gproxy_protocol::{
    Dialect,
    transform::{
        TransformErrorKind,
        generate::{
            gemini_chat::{self, stream::*},
            stream::{chat::ChatStreamCollector, gemini::GeminiStreamCollector},
        },
        identity::{IdNamespace, IdentityFlow, TargetIdPolicy},
    },
    wire::{
        gemini as g,
        openai::chat::{self as c, stream as s},
    },
};
use serde_json::{Value, json};
fn flow() -> IdentityFlow {
    IdentityFlow::new(IdNamespace::with_bytes([8; 16]))
}
fn chat(choices: Value, usage: Option<Value>) -> s::ChatCompletionChunk {
    serde_json::from_value(json!({"id":"response","created":7,"model":"m","object":"chat.completion.chunk","choices":choices,"usage":usage})).unwrap()
}
fn choice(index: i64, delta: Value, finish: Option<&str>) -> Value {
    json!({"index":index,"delta":delta,"finish_reason":finish})
}
fn gemini(value: Value) -> g::GenerateContentResponseBody {
    serde_json::from_value(value).unwrap()
}
fn g_collected(chunks: Vec<g::GenerateContentResponseBody>) -> g::GenerateContentResponseBody {
    let mut c = GeminiStreamCollector::new(Default::default());
    for chunk in chunks {
        c.push(chunk).unwrap();
    }
    c.finish().unwrap().value
}
fn c_collected(chunks: Vec<s::ChatCompletionChunk>) -> c::GenerateContentResponseBody {
    let mut c = ChatStreamCollector::new(flow(), TargetIdPolicy::new(Dialect::OpenAiChat));
    for chunk in chunks {
        c.push(chunk).unwrap();
    }
    c.push_done().unwrap();
    c.finish().unwrap().value
}
fn context() -> GeminiToChatContext {
    GeminiToChatContext {
        created: 7,
        model: Some("m".into()),
    }
}
fn normalize_g(input: g::GenerateContentResponseBody) -> Value {
    let mut value = serde_json::to_value(input).unwrap();
    for candidate in value["candidates"].as_array_mut().unwrap() {
        if let Some(parts) = candidate
            .get_mut("content")
            .and_then(|v| v.get_mut("parts"))
            .and_then(Value::as_array_mut)
        {
            let mut out: Vec<Value> = Vec::new();
            for part in std::mem::take(parts) {
                if part
                    .as_object()
                    .is_some_and(|v| v.len() == 1 && v.contains_key("text"))
                    && out.last().is_some_and(|v| {
                        v.as_object()
                            .is_some_and(|v| v.len() == 1 && v.contains_key("text"))
                    })
                {
                    let old = out.last_mut().unwrap();
                    old["text"] = json!(format!(
                        "{}{}",
                        old["text"].as_str().unwrap(),
                        part["text"].as_str().unwrap()
                    ));
                } else {
                    out.push(part);
                }
            }
            *parts = out;
        }
    }
    value
}
#[test]
fn chat_text_is_preterminal_and_usage_tail_matches_buffered_conversion() {
    let source = vec![
        chat(
            json!([choice(0, json!({"role":"assistant","content":"ha"}), None)]),
            None,
        ),
        chat(json!([choice(0, json!({"content":"ha"}), None)]), None),
        chat(json!([choice(0, json!({}), Some("stop"))]), None),
        chat(
            json!([]),
            Some(json!({"prompt_tokens":3,"completion_tokens":2,"total_tokens":5})),
        ),
    ];
    let mut stream = ChatToGeminiStream::new(flow(), Default::default());
    let mut output = Vec::new();
    for (n, chunk) in source.iter().cloned().enumerate() {
        let converted = stream.push(chunk).unwrap();
        if n == 0 {
            assert_eq!(
                converted.value[0].candidates.as_ref().unwrap()[0]
                    .content
                    .as_ref()
                    .unwrap()
                    .parts
                    .as_ref()
                    .unwrap()[0]
                    .text
                    .as_deref(),
                Some("ha")
            );
            assert!(
                converted.value[0].candidates.as_ref().unwrap()[0]
                    .finish_reason
                    .is_none()
            );
        }
        output.extend(converted.value);
    }
    stream.push_done().unwrap();
    output.extend(stream.finish().unwrap().chunks);
    let actual = g_collected(output);
    let expected = gemini_chat::openai_to_gemini_response(&c_collected(source))
        .unwrap()
        .value;
    assert_eq!(normalize_g(actual), normalize_g(expected));
}
#[test]
fn chat_parallel_tools_have_distinct_late_ids_names_and_complete_object_arguments() {
    let mut stream = ChatToGeminiStream::new(flow(), Default::default());
    let mut out = Vec::new();
    out.extend(stream.push(chat(json!([choice(0,json!({"role":"assistant","content":"run","tool_calls":[{"index":0,"function":{"name":"same","arguments":"{\"x\":"}},{"index":1,"function":{"name":"same","arguments":"{"}}]}),None),choice(1,json!({"role":"assistant","content":"second"}),None)]),None)).unwrap().value);
    assert_eq!(out.len(), 2);
    out.extend(stream.push(chat(json!([choice(0,json!({"tool_calls":[{"index":0,"id":"native-a","function":{"arguments":"1}"}},{"index":1,"id":"native-b","function":{"arguments":"\"x\":2}"}}]}),Some("tool_calls"))]),None)).unwrap().value);
    // The other choice and real usage still arrive after choice zero finishes.
    out.extend(
        stream
            .push(chat(json!([choice(1, json!({}), Some("stop"))]), None))
            .unwrap()
            .value,
    );
    stream.push_done().unwrap();
    out.extend(stream.finish().unwrap().chunks);
    let result = serde_json::to_value(g_collected(out)).unwrap();
    let parts = result["candidates"][0]["content"]["parts"]
        .as_array()
        .unwrap();
    let calls: Vec<_> = parts.iter().filter_map(|v| v.get("functionCall")).collect();
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0]["name"], "same");
    assert_eq!(calls[1]["name"], "same");
    assert_eq!(calls[0]["id"], "native-a");
    assert_eq!(calls[1]["id"], "native-b");
    assert_eq!(calls[0]["args"]["x"], 1);
    assert_eq!(calls[1]["args"]["x"], 2);
}
#[test]
fn missing_ids_generate_unique_calls_and_legacy_fragments_remain_one_call() {
    let mut stream = ChatToGeminiStream::new(flow(), Default::default());
    let mut out=stream.push(chat(json!([choice(0,json!({"tool_calls":[{"index":0,"function":{"name":"f","arguments":"{}"}},{"index":1,"function":{"name":"f","arguments":"{}"}}]}),Some("tool_calls"))]),None)).unwrap().value;
    stream.push_done().unwrap();
    out.extend(stream.finish().unwrap().chunks);
    let v = serde_json::to_value(g_collected(out)).unwrap();
    let p = &v["candidates"][0]["content"]["parts"];
    assert_ne!(p[0]["functionCall"]["id"], p[1]["functionCall"]["id"]);
    let mut stream = ChatToGeminiStream::new(flow(), Default::default());
    assert!(
        stream
            .push(chat(
                json!([choice(
                    0,
                    json!({"function_call":{"name":"leg","arguments":"{"}}),
                    None
                )]),
                None
            ))
            .unwrap()
            .value
            .is_empty()
    );
    let mut out = stream
        .push(chat(
            json!([choice(
                0,
                json!({"function_call":{"name":"acy","arguments":"}"}}),
                Some("function_call")
            )]),
            None,
        ))
        .unwrap()
        .value;
    stream.push_done().unwrap();
    let end = stream.finish().unwrap();
    out.extend(end.chunks);
    let v = serde_json::to_value(g_collected(out)).unwrap();
    assert_eq!(
        v["candidates"][0]["content"]["parts"][0]["functionCall"]["name"],
        "legacy"
    );
    let id = v["candidates"][0]["content"]["parts"][0]["functionCall"]["id"]
        .as_str()
        .unwrap();
    let handle = end
        .identities
        .lookup_emitted_as(
            gproxy_protocol::transform::identity::IdentityRole::ToolCall,
            id,
        )
        .unwrap();
    assert!(
        handle.source_id().is_none(),
        "client alias must not become a fabricated native legacy ID"
    );
}

#[test]
fn gemini_repeated_fragments_and_tools_emit_preterminal_and_match_buffered() {
    let source = vec![
        gemini(
            json!({"responseId":"response","modelVersion":"m","candidates":[{"index":0,"content":{"role":"model","parts":[{"text":"ha"}]}}]}),
        ),
        gemini(
            json!({"candidates":[{"index":0,"content":{"parts":[{"text":"ha"},{"functionCall":{"name":"same","id":"call-a","args":{"x":1}}},{"functionCall":{"name":"same","id":"call-b","args":{"x":2}}}]},"finishReason":"STOP"},{"index":1,"content":{"role":"model","parts":[{"text":"second"}]},"finishReason":"MAX_TOKENS"}]}),
        ),
        gemini(
            json!({"usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":2,"totalTokenCount":5}}),
        ),
    ];
    let mut stream = GeminiToChatStream::new(context(), flow(), Default::default()).unwrap();
    let mut out = Vec::new();
    for (n, chunk) in source.iter().cloned().enumerate() {
        let converted = stream.push(chunk).unwrap();
        if n == 0 {
            assert_eq!(
                converted.value[0].choices[0]
                    .delta
                    .content
                    .as_ref()
                    .unwrap()
                    .as_deref(),
                Some("ha")
            );
            assert!(converted.value[0].choices[0].finish_reason.is_none());
        }
        out.extend(converted.value);
    }
    out.extend(stream.finish().unwrap().chunks);
    let actual = c_collected(out);
    let expected = gemini_chat::gemini_to_openai_response(
        g_collected(source),
        "m",
        &gemini_chat::GeminiChatResponseSupplement {
            created_unix_seconds: Some(7),
        },
        &mut flow(),
        &TargetIdPolicy::new(Dialect::OpenAiChat),
    )
    .unwrap()
    .value;
    assert_eq!(actual, expected);
    assert_eq!(actual.choices[0].message.content.as_deref(), Some("haha"));
}
#[test]
fn gemini_missing_late_response_identity_keeps_already_emitted_id() {
    let mut stream = GeminiToChatStream::new(context(), flow(), Default::default()).unwrap();
    let first = stream
        .push(gemini(
            json!({"candidates":[{"content":{"parts":[{"text":"first"}]}}]}),
        ))
        .unwrap()
        .value;
    let emitted = first[0].id.clone();
    assert!(!emitted.is_empty());
    let next=stream.push(gemini(json!({"responseId":"late-native","modelVersion":"m","candidates":[{"content":{"parts":[{"text":"second"},{"functionCall":{"name":"same","args":{}}},{"functionCall":{"name":"same","args":{}}}]},"finishReason":"STOP"}]}))).unwrap().value;
    assert!(next.iter().all(|v| v.id == emitted));
    let end = stream.finish().unwrap();
    assert!(end.chunks.iter().all(|v| v.id == emitted));
    let mut all = first;
    all.extend(next);
    all.extend(end.chunks);
    let actual = c_collected(all);
    let calls = actual.choices[0].message.tool_calls.as_ref().unwrap();
    let value = serde_json::to_value(calls).unwrap();
    assert_ne!(value[0]["id"], value[1]["id"]);
}
#[test]
fn missing_model_waits_for_factual_source_model_and_missing_usage_stays_missing() {
    let mut stream = GeminiToChatStream::new(
        GeminiToChatContext {
            created: 7,
            model: None,
        },
        flow(),
        Default::default(),
    )
    .unwrap();
    assert!(
        stream
            .push(gemini(
                json!({"candidates":[{"content":{"parts":[{"text":"pending"}]}}]})
            ))
            .unwrap()
            .value
            .is_empty()
    );
    let mut out = stream
        .push(gemini(
            json!({"modelVersion":"m","candidates":[{"finishReason":"STOP"}]}),
        ))
        .unwrap()
        .value;
    assert_eq!(out[0].model, "m");
    out.extend(stream.finish().unwrap().chunks);
    assert!(c_collected(out).usage.is_none());
    let source = chat(
        json!([choice(0, json!({"content":"x"}), Some("stop"))]),
        None,
    );
    let mut stream = ChatToGeminiStream::new(flow(), Default::default());
    let mut out = stream.push(source.clone()).unwrap().value;
    stream.push_done().unwrap();
    out.extend(stream.finish().unwrap().chunks);
    assert!(g_collected(out).usage_metadata.is_none());
    assert!(
        gemini_chat::openai_to_gemini_response(&c_collected(vec![source]))
            .unwrap()
            .value
            .usage_metadata
            .is_none()
    );
}
#[test]
fn source_lifecycle_and_limits_poison_both_directions() {
    let chunk = chat(json!([choice(0, json!({"content":"hello"}), None)]), None);
    for limits in [
        StreamLimits {
            max_bytes: 1,
            ..Default::default()
        },
        StreamLimits {
            max_events: 0,
            ..Default::default()
        },
        StreamLimits {
            max_choices: 0,
            ..Default::default()
        },
    ] {
        let mut stream = ChatToGeminiStream::new(flow(), limits);
        assert_eq!(
            stream.push(chunk.clone()).unwrap_err().kind(),
            TransformErrorKind::Limit
        );
        assert!(stream.push_done().is_err());
        assert!(stream.finish().is_err());
    }
    let mut stream = ChatToGeminiStream::new(flow(), Default::default());
    stream.push(chunk.clone()).unwrap();
    assert!(stream.push_done().is_err());
    assert!(stream.push(chunk.clone()).is_err());
    let mut stream = ChatToGeminiStream::new(flow(), Default::default());
    stream
        .push(chat(
            json!([choice(0, json!({"content":"x"}), Some("stop"))]),
            None,
        ))
        .unwrap();
    assert!(stream.finish().is_err());
    let mut stream = GeminiToChatStream::new(context(), flow(), Default::default()).unwrap();
    stream
        .push(gemini(
            json!({"candidates":[{"content":{"parts":[{"text":"x"}]},"finishReason":"STOP"}]}),
        ))
        .unwrap();
    assert!(
        stream
            .push(gemini(
                json!({"candidates":[{"content":{"parts":[{"text":"illegal"}]}}]})
            ))
            .is_err()
    );
    assert!(stream.finish().is_err());
    let mut stream = GeminiToChatStream::new(
        GeminiToChatContext {
            created: 7,
            model: None,
        },
        flow(),
        StreamLimits {
            max_bytes: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        stream
            .push(gemini(json!({"candidates":[]})))
            .unwrap_err()
            .kind(),
        TransformErrorKind::Limit
    );
}
#[test]
fn malformed_tool_json_is_not_defaulted_and_foreign_rest_never_becomes_output() {
    let mut stream = ChatToGeminiStream::new(flow(), Default::default());
    assert!(
        stream
            .push(chat(
                json!([choice(
                    0,
                    json!({"tool_calls":[{"index":0,"function":{"name":"f","arguments":""}}]}),
                    Some("tool_calls")
                )]),
                None
            ))
            .is_err()
    );
    assert!(stream.push_done().is_err());
    let plain = chat(
        json!([choice(0, json!({"content":"ok"}), Some("stop"))]),
        None,
    );
    let mut marked = serde_json::to_value(&plain).unwrap();
    marked["x-unknown"] = json!("DROP");
    marked["choices"][0]["x-unknown"] = json!("DROP");
    marked["choices"][0]["delta"]["reasoning_content"] = json!("DROP");
    let run = |chunk| {
        let mut stream = ChatToGeminiStream::new(flow(), Default::default());
        let mut out = stream.push(chunk).unwrap().value;
        stream.push_done().unwrap();
        out.extend(stream.finish().unwrap().chunks);
        normalize_g(g_collected(out))
    };
    assert_eq!(run(plain), run(serde_json::from_value(marked).unwrap()));
}

#[test]
fn missing_reasoning_breakdown_preserves_known_totals_without_invented_zero() {
    let native = c_collected(vec![chat(
        json!([choice(0, json!({"content":"ok"}), Some("stop"))]),
        Some(json!({"prompt_tokens":3,"completion_tokens":2,"total_tokens":5})),
    )]);
    let mapped = gemini_chat::openai_to_gemini_response(&native)
        .unwrap()
        .value;
    let usage = mapped.usage_metadata.as_ref().unwrap();
    assert_eq!(usage.total_token_count, Some(5));
    assert_eq!(usage.prompt_token_count, Some(3));
    assert!(usage.thoughts_token_count.is_none());
    assert!(usage.candidates_token_count.is_none());
    let back = gemini_chat::gemini_to_openai_response(
        mapped,
        "m",
        &gemini_chat::GeminiChatResponseSupplement {
            created_unix_seconds: Some(7),
        },
        &mut flow(),
        &TargetIdPolicy::new(Dialect::OpenAiChat),
    )
    .unwrap()
    .value;
    assert_eq!(back.usage.as_ref().unwrap().completion_tokens, 2);
    assert!(back.usage.unwrap().completion_tokens_details.is_none());
}

#[test]
fn refusal_and_reasoning_flags_preserve_representable_payloads() {
    let source = vec![
        chat(json!([choice(0, json!({"refusal":"not "}), None)]), None),
        chat(
            json!([choice(
                0,
                json!({"refusal":"allowed"}),
                Some("content_filter")
            )]),
            None,
        ),
    ];
    let mut stream = ChatToGeminiStream::new(flow(), Default::default());
    let mut out = Vec::new();
    let mut reports = Vec::new();
    for chunk in source.iter().cloned() {
        let r = stream.push(chunk).unwrap();
        out.extend(r.value);
        reports.extend(r.report.diagnostics);
    }
    stream.push_done().unwrap();
    out.extend(stream.finish().unwrap().chunks);
    assert!(!reports.is_empty());
    let actual = g_collected(out);
    assert_eq!(
        actual.candidates.as_ref().unwrap()[0].finish_reason,
        Some(g::FinishReason::Safety)
    );
    assert_eq!(
        normalize_g(actual),
        normalize_g(
            gemini_chat::openai_to_gemini_response(&c_collected(source))
                .unwrap()
                .value
        )
    );
    let source = gemini(
        json!({"modelVersion":"m","responseId":"r","candidates":[{"content":{"parts":[{"text":"private reasoning","thought":true,"functionCall":{"id":"call","name":"f","args":{}}},{"text":"visible"}]},"finishReason":"STOP"}]}),
    );
    let mut stream = GeminiToChatStream::new(context(), flow(), Default::default()).unwrap();
    let first = stream.push(source.clone()).unwrap();
    assert!(!first.report.diagnostics.is_empty());
    let mut out = first.value;
    out.extend(stream.finish().unwrap().chunks);
    let actual = c_collected(out);
    assert_eq!(
        actual.choices[0].message.content.as_deref(),
        Some("visible")
    );
    assert_eq!(
        actual.choices[0].message.tool_calls.as_ref().unwrap().len(),
        1
    );
    let expected = gemini_chat::gemini_to_openai_response(
        source,
        "m",
        &gemini_chat::GeminiChatResponseSupplement {
            created_unix_seconds: Some(7),
        },
        &mut flow(),
        &TargetIdPolicy::new(Dialect::OpenAiChat),
    )
    .unwrap()
    .value;
    assert_eq!(actual, expected);
}

#[test]
fn late_logprob_snapshot_after_candidate_finish_is_emitted_once() {
    let source = vec![
        gemini(
            json!({"modelVersion":"m","responseId":"r","candidates":[{"content":{"parts":[{"text":"ok"}]},"finishReason":"STOP"}]}),
        ),
        gemini(
            json!({"candidates":[{"logprobsResult":{"chosenCandidates":[{"token":"ok","logProbability":-0.2}],"topCandidates":[{"candidates":[{"token":"other","logProbability":-2.0}]}]}}]}),
        ),
    ];
    let mut stream = GeminiToChatStream::new(context(), flow(), Default::default()).unwrap();
    let mut out = Vec::new();
    for chunk in source.iter().cloned() {
        out.extend(stream.push(chunk).unwrap().value);
    }
    out.extend(stream.finish().unwrap().chunks);
    let actual = c_collected(out);
    let expected = gemini_chat::gemini_to_openai_response(
        g_collected(source),
        "m",
        &gemini_chat::GeminiChatResponseSupplement {
            created_unix_seconds: Some(7),
        },
        &mut flow(),
        &TargetIdPolicy::new(Dialect::OpenAiChat),
    )
    .unwrap()
    .value;
    assert_eq!(actual, expected);
}

#[test]
fn empty_chat_choice_has_same_explicit_gemini_content_as_buffered() {
    let source = vec![chat(
        json!([choice(0, json!({"role":"assistant"}), Some("stop"))]),
        None,
    )];
    let mut stream = ChatToGeminiStream::new(flow(), Default::default());
    let mut out = stream.push(source[0].clone()).unwrap().value;
    stream.push_done().unwrap();
    out.extend(stream.finish().unwrap().chunks);
    let actual = g_collected(out);
    let expected = gemini_chat::openai_to_gemini_response(&c_collected(source))
        .unwrap()
        .value;
    assert_eq!(normalize_g(actual), normalize_g(expected));
}
#[test]
fn sparse_gemini_candidate_set_cannot_finish_as_successful_chat() {
    let mut stream = GeminiToChatStream::new(context(), flow(), Default::default()).unwrap();
    stream.push(gemini(json!({"modelVersion":"m","candidates":[{"index":2,"content":{"parts":[{"text":"third"}]},"finishReason":"STOP"}]}))).unwrap();
    assert!(
        stream.finish().is_err(),
        "a successful mapped Chat tail must be accepted by the native Chat collector"
    );
}

#[test]
fn audio_usage_facts_survive_both_stream_directions_without_unknown_reasoning_zero() {
    let source = [chat(
        json!([choice(0, json!({"content":"audio"}), Some("stop"))]),
        Some(
            json!({"prompt_tokens":6,"completion_tokens":4,"total_tokens":10,"prompt_tokens_details":{"audio_tokens":3,"cached_tokens":1},"completion_tokens_details":{"audio_tokens":2}}),
        ),
    )];
    let mut stream = ChatToGeminiStream::new(flow(), Default::default());
    let mut out = stream.push(source[0].clone()).unwrap().value;
    stream.push_done().unwrap();
    out.extend(stream.finish().unwrap().chunks);
    let native = g_collected(out);
    let usage = native.usage_metadata.as_ref().unwrap();
    assert!(usage.thoughts_token_count.is_none());
    assert!(usage.candidates_token_count.is_none());
    assert_eq!(
        usage.prompt_tokens_details.as_ref().unwrap()[0].modality,
        Some(g::Modality::Audio)
    );
    assert_eq!(
        usage.prompt_tokens_details.as_ref().unwrap()[0].token_count,
        Some(3)
    );
    assert_eq!(
        usage.candidates_tokens_details.as_ref().unwrap()[0].token_count,
        Some(2)
    );
    let mut stream = GeminiToChatStream::new(context(), flow(), Default::default()).unwrap();
    let mut out = stream.push(native).unwrap().value;
    out.extend(stream.finish().unwrap().chunks);
    let actual = c_collected(out);
    let usage = actual.usage.unwrap();
    assert_eq!(
        usage.prompt_tokens_details.as_ref().unwrap().audio_tokens,
        Some(3)
    );
    assert_eq!(usage.prompt_tokens_details.unwrap().cached_tokens, Some(1));
    assert_eq!(
        usage
            .completion_tokens_details
            .as_ref()
            .unwrap()
            .audio_tokens,
        Some(2)
    );
    assert!(
        usage
            .completion_tokens_details
            .unwrap()
            .reasoning_tokens
            .is_none()
    );
}
#[test]
fn combined_prompt_audio_requires_both_initial_and_tool_facts() {
    for complete in [false, true] {
        let mut usage = json!({"promptTokenCount":4,"toolUsePromptTokenCount":2,"candidatesTokenCount":3,"thoughtsTokenCount":1,"totalTokenCount":10,"promptTokensDetails":[{"modality":"AUDIO","tokenCount":2}],"candidatesTokensDetails":[{"modality":"AUDIO","tokenCount":1}]});
        if complete {
            usage["toolUsePromptTokensDetails"] = json!([{"modality":"AUDIO","tokenCount":1}]);
        }
        let source = gemini(
            json!({"modelVersion":"m","candidates":[{"content":{"parts":[{"text":"done"}]},"finishReason":"STOP"}],"usageMetadata":usage}),
        );
        let mut stream = GeminiToChatStream::new(context(), flow(), Default::default()).unwrap();
        let mut out = stream.push(source).unwrap().value;
        out.extend(stream.finish().unwrap().chunks);
        let usage = c_collected(out).usage.unwrap();
        assert_eq!(usage.prompt_tokens, 6);
        assert_eq!(
            usage.prompt_tokens_details.and_then(|d| d.audio_tokens),
            complete.then_some(3)
        );
        assert_eq!(
            usage.completion_tokens_details.unwrap().audio_tokens,
            Some(1)
        );
    }
}
#[test]
fn out_of_order_candidate_arrival_is_valid_when_the_final_set_is_complete() {
    let mut stream = GeminiToChatStream::new(context(), flow(), Default::default()).unwrap();
    let mut out = stream
        .push(gemini(
            json!({"candidates":[{"index":1,"content":{"parts":[{"text":"second"}]}}]}),
        ))
        .unwrap()
        .value;
    out.extend(stream.push(gemini(json!({"candidates":[{"index":0,"content":{"parts":[{"text":"first"}]},"finishReason":"STOP"},{"index":1,"finishReason":"STOP"}]}))).unwrap().value);
    out.extend(stream.finish().unwrap().chunks);
    let actual = c_collected(out);
    assert_eq!(actual.choices[0].message.content.as_deref(), Some("first"));
    assert_eq!(actual.choices[1].message.content.as_deref(), Some("second"));
}
#[test]
fn target_output_and_tool_caps_do_not_allow_success_after_overflow() {
    let result = GeminiToChatStream::new(
        GeminiToChatContext {
            created: 7,
            model: Some("m".repeat(1024)),
        },
        flow(),
        StreamLimits {
            max_bytes: 512,
            ..Default::default()
        },
    );
    assert!(matches!(result,Err(e) if e.kind()==TransformErrorKind::Limit));
    let mut stream = GeminiToChatStream::new(
        GeminiToChatContext {
            created: 7,
            model: Some("m".repeat(400)),
        },
        flow(),
        StreamLimits {
            max_bytes: 1000,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(stream.push(gemini(json!({"candidates":[{"index":0,"content":{"parts":[{"text":"first"}]}},{"index":1,"content":{"parts":[{"text":"second"}]}}]}))).unwrap_err().kind(),TransformErrorKind::Limit);
    assert!(stream.finish().is_err());
    let mut stream = GeminiToChatStream::new(
        context(),
        flow(),
        StreamLimits {
            max_tools: 0,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(stream.push(gemini(json!({"candidates":[{"content":{"parts":[{"functionCall":{"name":"f","args":{}}}]},"finishReason":"STOP"}]}))).unwrap_err().kind(),TransformErrorKind::Limit);
    assert!(stream.finish().is_err());
    let mut stream = ChatToGeminiStream::new(
        flow(),
        StreamLimits {
            max_tools: 0,
            ..Default::default()
        },
    );
    assert_eq!(
        stream
            .push(chat(
                json!([choice(
                    0,
                    json!({"tool_calls":[{"index":0,"function":{"name":"f","arguments":"{}"}}]}),
                    Some("tool_calls")
                )]),
                None
            ))
            .unwrap_err()
            .kind(),
        TransformErrorKind::Limit
    );
    assert!(stream.push_done().is_err());
}
#[test]
fn late_model_facts_cannot_relabel_an_emitted_chat_stream() {
    let mut stream = GeminiToChatStream::new(context(), flow(), Default::default()).unwrap();
    let first = stream
        .push(gemini(
            json!({"candidates":[{"content":{"parts":[{"text":"early"}]}}]}),
        ))
        .unwrap()
        .value;
    assert_eq!(first[0].model, "m");
    assert!(stream.push(gemini(json!({"modelVersion":"different-actual-version","candidates":[{"finishReason":"STOP"}]}))).is_err());
    assert!(stream.finish().is_err());
}

#[test]
fn audio_modality_counts_must_fit_known_non_reasoning_output() {
    for count in [-1, 3] {
        let source = gemini(
            json!({"candidates":[{"content":{"parts":[{"text":"x"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":1,"totalTokenCount":5,"thoughtsTokenCount":3,"candidatesTokensDetails":[{"modality":"AUDIO","tokenCount":count}]}}),
        );
        let mut stream = GeminiToChatStream::new(context(), flow(), Default::default()).unwrap();
        stream.push(source).unwrap();
        assert!(stream.finish().is_err());
    }
}
