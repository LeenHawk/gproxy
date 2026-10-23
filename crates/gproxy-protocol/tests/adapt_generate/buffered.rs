use super::*;

#[test]
fn h_via_c_executes_original_directed_return_mapping() {
    let store = Store::default();
    let state = state(&store, Dialect::Claude);
    let host = Host::new(output("c"));
    let mut prepared = ChatViaClaude::prepare(
        serde_json::from_value(input("h")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::OpenAiChat, Dialect::Claude),
    )
    .unwrap();
    let mut progress = GenerationProgress::default();
    let result = ready(
        prepared.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
            Ok(claude_chat::ResponseSupplement {
                created_unix_seconds: Some(123),
            })
        }),
    )
    .unwrap();
    assert!(progress.send_started);
    assert!(progress.raw_response.is_some());
    assert!(progress.native_response.is_some());
    check(result, &host, "h");
}

#[test]
fn c_via_h_executes_original_directed_return_mapping() {
    let store = Store::default();
    let state = state(&store, Dialect::OpenAiChat);
    let host = Host::new(output("h"));
    let mut prepared = ClaudeViaChat::prepare(
        serde_json::from_value(input("c")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::Claude, Dialect::OpenAiChat),
    )
    .unwrap();
    let mut progress = GenerationProgress::default();
    let result = ready(
        prepared.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
            Ok(claude_chat::ResponseSupplement::default())
        }),
    )
    .unwrap();
    assert!(progress.send_started);
    assert!(progress.raw_response.is_some());
    assert!(progress.native_response.is_some());
    check(result, &host, "c");
}

#[test]
fn h_via_g_executes_original_directed_return_mapping() {
    let store = Store::default();
    let state = state(&store, Dialect::Gemini);
    let host = Host::new(output("g"));
    let mut prepared = ChatViaGemini::prepare(
        serde_json::from_value(input("h")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::OpenAiChat, Dialect::Gemini),
        &Default::default(),
    )
    .unwrap();
    let mut progress = GenerationProgress::default();
    let result = ready(
        prepared.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
            Ok(gemini_chat::GeminiChatResponseSupplement {
                created_unix_seconds: Some(123),
            })
        }),
    )
    .unwrap();
    assert!(progress.send_started);
    assert!(progress.raw_response.is_some());
    assert!(progress.native_response.is_some());
    check(result, &host, "h");
}

#[test]
fn g_via_h_executes_original_directed_return_mapping() {
    let store = Store::default();
    let state = state(&store, Dialect::OpenAiChat);
    let host = Host::new(output("h"));
    let mut prepared = GeminiViaChat::prepare(
        serde_json::from_value(input("g")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::Gemini, Dialect::OpenAiChat),
    )
    .unwrap();
    let mut progress = GenerationProgress::default();
    let result =
        ready(prepared.invoke(
            &host,
            &(),
            codec_limits(),
            &state,
            &mut progress,
            |_| Ok(()),
        ))
        .unwrap();
    assert!(progress.send_started);
    assert!(progress.raw_response.is_some());
    assert!(progress.native_response.is_some());
    check(result, &host, "g");
}

#[test]
fn h_via_r_executes_original_directed_return_mapping() {
    let store = Store::default();
    let state = state(&store, Dialect::OpenAi);
    let host = Host::new(output("r"));
    let mut prepared = ChatViaResponses::prepare(
        serde_json::from_value(input("h")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::OpenAiChat, Dialect::OpenAi),
    )
    .unwrap();
    let mut progress = GenerationProgress::default();
    let result =
        ready(prepared.invoke(
            &host,
            &(),
            codec_limits(),
            &state,
            &mut progress,
            |_| Ok(()),
        ))
        .unwrap();
    assert!(progress.send_started);
    assert!(progress.raw_response.is_some());
    assert!(progress.native_response.is_some());
    check(result, &host, "h");
}

#[test]
fn r_via_h_executes_original_directed_return_mapping() {
    let store = Store::default();
    let state = state(&store, Dialect::OpenAiChat);
    let host = Host::new(output("h"));
    let mut prepared = ResponsesViaChat::prepare(
        serde_json::from_value(input("r")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::OpenAi, Dialect::OpenAiChat),
    )
    .unwrap();
    let mut progress = GenerationProgress::default();
    let result = ready(
        prepared.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
            Ok(ChatReturnFacts {
                parallel_tool_calls: true,
                tool_choice: auto(),
                prompt_cache_options: None,
                usage: gproxy_protocol::transform::generate::chat_responses::ChatUsageSupplement {
                    cache_write_tokens: Some(0),
                    ..Default::default()
                },
            })
        }),
    )
    .unwrap();
    assert!(progress.send_started);
    assert!(progress.raw_response.is_some());
    assert!(progress.native_response.is_some());
    check(result, &host, "r");
}

#[test]
fn c_via_g_executes_original_directed_return_mapping() {
    let store = Store::default();
    let state = state(&store, Dialect::Gemini);
    let host = Host::new(output("g"));
    let mut prepared = ClaudeViaGemini::prepare(
        serde_json::from_value(input("c")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::Claude, Dialect::Gemini),
        Default::default(),
    )
    .unwrap();
    let mut progress = GenerationProgress::default();
    let result = ready(
        prepared.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
            Ok(claude_gemini::ClaudeGeminiUsageFacts {
                cache_creation_input_tokens: Some(0),
                cache_read_input_tokens: Some(0),
                thinking_tokens: Some(0),
            })
        }),
    )
    .unwrap();
    assert!(progress.send_started);
    assert!(progress.raw_response.is_some());
    assert!(progress.native_response.is_some());
    check(result, &host, "c");
}

#[test]
fn g_via_c_executes_original_directed_return_mapping() {
    let store = Store::default();
    let state = state(&store, Dialect::Claude);
    let host = Host::new(output("c"));
    let mut prepared = GeminiViaClaude::prepare(
        serde_json::from_value(input("g")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::Gemini, Dialect::Claude),
        Some(64),
    )
    .unwrap();
    let mut progress = GenerationProgress::default();
    let result = ready(
        prepared.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
            Ok(claude_gemini::ClaudeGeminiUsageFacts {
                cache_creation_input_tokens: Some(0),
                cache_read_input_tokens: Some(0),
                thinking_tokens: Some(0),
            })
        }),
    )
    .unwrap();
    assert!(progress.send_started);
    assert!(progress.raw_response.is_some());
    assert!(progress.native_response.is_some());
    check(result, &host, "g");
}

#[test]
fn c_via_r_executes_original_directed_return_mapping() {
    let store = Store::default();
    let state = state(&store, Dialect::OpenAi);
    let host = Host::new(output("r"));
    let mut prepared = ClaudeViaResponses::prepare(
        serde_json::from_value(input("c")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::Claude, Dialect::OpenAi),
    )
    .unwrap();
    let mut progress = GenerationProgress::default();
    let result = ready(
        prepared.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
            Ok(Default::default())
        }),
    )
    .unwrap();
    assert!(progress.send_started);
    assert!(progress.raw_response.is_some());
    assert!(progress.native_response.is_some());
    check(result, &host, "c");
}

#[test]
fn r_via_c_executes_original_directed_return_mapping() {
    let store = Store::default();
    let state = state(&store, Dialect::Claude);
    let host = Host::new(output("c"));
    let mut prepared = ResponsesViaClaude::prepare(
        serde_json::from_value(input("r")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::OpenAi, Dialect::Claude),
        Default::default(),
    )
    .unwrap();
    let mut progress = GenerationProgress::default();
    let result = ready(
        prepared.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
            Ok(ClaudeReturnFacts {
                parallel_tool_calls: true,
                tool_choice: auto(),
                prompt_cache_options: None,
                usage: Default::default(),
                created_at: 123,
            })
        }),
    )
    .unwrap();
    assert!(progress.send_started);
    assert!(progress.raw_response.is_some());
    assert!(progress.native_response.is_some());
    check(result, &host, "r");
}

#[test]
fn g_via_r_executes_original_directed_return_mapping() {
    let store = Store::default();
    let state = state(&store, Dialect::OpenAi);
    let host = Host::new(output("r"));
    let mut prepared = GeminiViaResponses::prepare(
        serde_json::from_value(input("g")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::Gemini, Dialect::OpenAi),
    )
    .unwrap();
    let mut progress = GenerationProgress::default();
    let result = ready(
        prepared.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
            Ok(Default::default())
        }),
    )
    .unwrap();
    assert!(progress.send_started);
    assert!(progress.raw_response.is_some());
    assert!(progress.native_response.is_some());
    check(result, &host, "g");
}

#[test]
fn r_via_g_executes_original_directed_return_mapping() {
    let store = Store::default();
    let state = state(&store, Dialect::Gemini);
    let host = Host::new(output("g"));
    let mut prepared = ResponsesViaGemini::prepare(
        serde_json::from_value(input("r")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::OpenAi, Dialect::Gemini),
        Default::default(),
    )
    .unwrap();
    let mut progress = GenerationProgress::default();
    let result = ready(
        prepared.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
            Ok(GeminiReturnFacts {
                parallel_tool_calls: true,
                tool_choice: auto(),
                prompt_cache_options: None,
                usage: gproxy_protocol::transform::generate::gemini_responses::GeminiUsageFacts {
                    cache_write_tokens: Some(0),
                    cached_tokens: None,
                },
                created_at: 123,
            })
        }),
    )
    .unwrap();
    assert!(progress.send_started);
    assert!(progress.raw_response.is_some());
    assert!(progress.native_response.is_some());
    check(result, &host, "r");
}

#[test]
fn claude_sampling_is_omitted_for_all_request_pairs_even_with_old_client_headers() {
    for version in [None, Some("2023-06-01"), Some("2026-10-21")] {
        let mut endpoint = endpoint();
        if let Some(version) = version {
            endpoint
                .headers
                .insert("anthropic-version", http::HeaderValue::from_static(version));
        }
        let mut chat = input("h");
        chat["temperature"] = json!(0.4);
        chat["top_p"] = json!(0.8);
        let chat = ChatViaClaude::prepare(
            serde_json::from_value(chat).unwrap(),
            "claude-sonnet-4-6",
            endpoint.clone(),
            ids(Dialect::OpenAiChat, Dialect::Claude),
        )
        .unwrap();
        let mut responses = input("r");
        responses["temperature"] = json!(0.4);
        responses["top_p"] = json!(0.8);
        let responses = ResponsesViaClaude::prepare(
            serde_json::from_value(responses).unwrap(),
            "claude-sonnet-4-6",
            endpoint.clone(),
            ids(Dialect::OpenAi, Dialect::Claude),
            Default::default(),
        )
        .unwrap();
        let mut gemini = input("g");
        gemini["generationConfig"] =
            json!({"maxOutputTokens":32, "temperature":0.4, "topP":0.8, "topK":20});
        let gemini = GeminiViaClaude::prepare(
            serde_json::from_value(gemini).unwrap(),
            "claude-sonnet-4-6",
            endpoint,
            ids(Dialect::Gemini, Dialect::Claude),
            None,
        )
        .unwrap();
        for (target, report) in [
            (chat.target_request(), chat.report()),
            (responses.target_request(), responses.report()),
            (gemini.target_request(), gemini.report()),
        ] {
            let value = serde_json::to_value(target).unwrap();
            for field in ["temperature", "top_p", "top_k"] {
                assert!(value.get(field).is_none());
            }
            assert!(
                report
                    .diagnostics
                    .iter()
                    .any(|item| item.field == "temperature")
            );
        }
    }
}

#[test]
fn claude_control_only_system_message_reaches_all_generation_targets() {
    let input = json!({"model":"claude", "max_tokens":32, "messages":[
        {"role":"user", "content":"hello"}, {"role":"system", "output_config":{"effort":"high"}}
    ]});
    let chat = ClaudeViaChat::prepare(
        serde_json::from_value(input.clone()).unwrap(),
        "gpt",
        endpoint(),
        ids(Dialect::Claude, Dialect::OpenAiChat),
    )
    .unwrap();
    let responses = ClaudeViaResponses::prepare(
        serde_json::from_value(input.clone()).unwrap(),
        "gpt",
        endpoint(),
        ids(Dialect::Claude, Dialect::OpenAi),
    )
    .unwrap();
    let gemini = ClaudeViaGemini::prepare(
        serde_json::from_value(input).unwrap(),
        "gemini",
        endpoint(),
        ids(Dialect::Claude, Dialect::Gemini),
        Default::default(),
    )
    .unwrap();
    let chat = serde_json::to_value(chat.target_request()).unwrap();
    let responses = serde_json::to_value(responses.target_request()).unwrap();
    let gemini = serde_json::to_value(gemini.target_request()).unwrap();
    assert_eq!(chat["messages"].as_array().unwrap().len(), 1);
    assert_eq!(chat["reasoning_effort"], "high");
    assert_eq!(responses["input"].as_array().unwrap().len(), 1);
    assert_eq!(responses["reasoning"]["effort"], "high");
    assert_eq!(gemini["contents"].as_array().unwrap().len(), 1);
    assert_eq!(
        gemini["generationConfig"]["thinkingConfig"]["thinkingLevel"],
        "HIGH"
    );
}

#[test]
fn native_claude_upgrade_preserves_extensions_and_tool_payloads() {
    use gproxy_protocol::{Operation, OperationKey, adapt::generate::claude_compat};
    let citation = json!({"type":"char_location", "cited_text":"text", "document_index":0, "document_title":"d", "start_char_index":0, "end_char_index":4});
    let source = json!({
        "model":"claude", "max_tokens":32, "temperature":0.2, "top_p":0.8, "top_k":20,
        "x-provider":{"type":"text", "citations":[citation.clone()]},
        "messages":[
            {"role":"assistant", "x-message":"keep", "content":[
                {"type":"text", "text":"answer", "citations":[citation.clone()]},
                {"type":"document", "source":{"type":"text", "media_type":"text/plain", "data":"doc"}, "citations":{"enabled":true}},
                {"type":"tool_use", "id":"call", "name":"f", "input":{"type":"text", "citations":[citation.clone()]}}
            ]},
            {"role":"user", "content":[{"type":"tool_result", "tool_use_id":"call", "content":[
                {"type":"text", "text":"result", "citations":[citation]},
                {"type":"search_result", "source":"https://example.com", "title":"source", "citations":{"enabled":true}, "content":[{"type":"text", "text":"found", "citations":[{"type":"future"}]}]}
            ]}]}
        ]
    });
    for operation in [
        Operation::GenerateContent,
        Operation::StreamGenerateContent,
        Operation::CountTokens,
    ] {
        let mut request = WireRequest {
            method: http::Method::POST,
            path: "/v1/messages".into(),
            query: None,
            headers: http::HeaderMap::from_iter([(
                http::header::CONTENT_LENGTH,
                http::HeaderValue::from_static("1"),
            )]),
            body: HttpBody::Bytes(source.to_string().into()),
        };
        claude_compat::prepare(
            OperationKey {
                operation,
                dialect: Dialect::Claude,
            },
            &mut request,
        );
        assert!(!request.headers.contains_key(http::header::CONTENT_LENGTH));
        let HttpBody::Bytes(bytes) = request.body else {
            panic!("buffered body expected")
        };
        let actual: Value = serde_json::from_slice(&bytes).unwrap();
        for field in ["temperature", "top_p", "top_k"] {
            assert!(actual.get(field).is_none());
        }
        assert_eq!(actual["x-provider"], source["x-provider"]);
        assert_eq!(
            actual["messages"][0]["content"][0],
            source["messages"][0]["content"][0]
        );
        assert_eq!(actual["messages"][1]["role"], "user");
        assert_eq!(actual["messages"][1]["x-message"], "keep");
        assert_eq!(
            actual["messages"][1]["content"][0]["citations"]["enabled"],
            true
        );
        assert_eq!(
            actual["messages"][2]["content"][0]["input"],
            source["messages"][0]["content"][2]["input"]
        );
        let result = &actual["messages"][3]["content"][0]["content"];
        assert!(result[0].get("citations").is_none());
        assert!(result[1]["content"][0].get("citations").is_none());
        assert_eq!(result[1]["citations"]["enabled"], true);
    }
}

#[test]
fn native_claude_upgrade_keeps_credit_replays_and_other_dialects_unchanged() {
    use gproxy_protocol::{Operation, OperationKey, adapt::generate::claude_compat};
    for (dialect, body) in [
        (
            Dialect::Claude,
            "{ \"fallback_credit_token\":\"bound\", \"temperature\":0.2 }",
        ),
        (
            Dialect::Claude,
            "{ \"messages\":[{\"role\":\"user\",\"content\":\"hello\"}] }",
        ),
        (Dialect::OpenAiChat, "{ \"temperature\":0.2 }"),
    ] {
        let mut request = WireRequest {
            method: http::Method::POST,
            path: "/v1/messages".into(),
            query: None,
            headers: Default::default(),
            body: HttpBody::Bytes(body.into()),
        };
        claude_compat::prepare(
            OperationKey {
                operation: Operation::GenerateContent,
                dialect,
            },
            &mut request,
        );
        let HttpBody::Bytes(bytes) = request.body else {
            panic!("buffered body expected")
        };
        assert_eq!(bytes.as_ref(), body.as_bytes());
    }
}
