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
