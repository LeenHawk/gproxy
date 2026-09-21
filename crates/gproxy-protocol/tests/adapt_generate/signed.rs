use super::*;

#[test]
fn signed_claude_thinking_is_saved_before_response_and_restored_only_to_original() {
    let mut body = output("c");
    body["content"] = json!([{"type":"thinking","thinking":"private reasoning","signature":"native-claude-signature","foreign":"ignore"},{"type":"text","text":"answer"}]);
    let host = Host::new(body);
    let store = Store::default();
    let state = state(&store, Dialect::Claude);
    let mut progress = GenerationProgress::default();
    let mut p = ResponsesViaClaude::prepare(
        serde_json::from_value(input("r")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::OpenAi, Dialect::Claude),
        Default::default(),
    )
    .unwrap();
    let result = ready(
        p.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
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
    let GenerationOutcome::Success { response, .. } = result else {
        panic!("rejected")
    };
    let wire = serde_json::to_value(response.body).unwrap();
    assert!(wire["output"][0].get("encrypted_content").is_none());
    let mut next = input("r");
    next["input"] = wire["output"].clone();
    let restored = ready(ResponsesViaClaude::prepare_with_state(
        serde_json::from_value(next.clone()).unwrap(),
        endpoint(),
        ids(Dialect::OpenAi, Dialect::Claude),
        &state,
        Default::default(),
    ))
    .unwrap();
    let sent = serde_json::to_value(restored.target_request()).unwrap();
    assert_eq!(
        sent["messages"][0]["content"][0]["signature"],
        "native-claude-signature"
    );
    assert!(
        store
            .entries
            .lock()
            .unwrap()
            .values()
            .all(|e| !std::str::from_utf8(&e.payload).unwrap().contains("foreign"))
    );
    next["input"][0]["content"][0]["text"] = json!("modified reasoning");
    assert!(
        ready(ResponsesViaClaude::prepare_with_state(
            serde_json::from_value(next).unwrap(),
            endpoint(),
            ids(Dialect::OpenAi, Dialect::Claude),
            &state,
            Default::default()
        ))
        .is_err()
    );
}

#[test]
fn signed_gemini_function_replays_exact_native_part_and_tool_result_name() {
    let mut body = output("g");
    body["candidates"][0]["content"]["parts"] = json!([{"functionCall":{"id":"native-call","name":"lookup","args":{"x":1}},"thoughtSignature":"native-gemini-signature","foreign":"ignore"}]);
    let host = Host::new(body);
    let store = Store::default();
    let state = state(&store, Dialect::Gemini);
    let mut progress = GenerationProgress::default();
    let mut p = ResponsesViaGemini::prepare(
        serde_json::from_value(input("r")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::OpenAi, Dialect::Gemini),
        Default::default(),
    )
    .unwrap();
    let result = ready(
        p.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
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
    let GenerationOutcome::Success { response, .. } = result else {
        panic!("rejected")
    };
    let wire = serde_json::to_value(response.body).unwrap();
    let mut next = input("r");
    let mut items = wire["output"].as_array().unwrap().clone();
    items.push(
        json!({"type":"function_call_output","call_id":"native-call","output":"actual result"}),
    );
    next["input"] = json!(items);
    let restored = ready(ResponsesViaGemini::prepare_with_state(
        serde_json::from_value(next.clone()).unwrap(),
        endpoint(),
        ids(Dialect::OpenAi, Dialect::Gemini),
        &state,
        Default::default(),
    ))
    .unwrap();
    let target = serde_json::to_value(restored.target_request()).unwrap();
    assert_eq!(
        target["contents"][0]["parts"][0]["thoughtSignature"],
        "native-gemini-signature"
    );
    assert_eq!(
        target["contents"][1]["parts"][0]["functionResponse"]["name"],
        "lookup"
    );
    next["input"][0]["arguments"] = json!("{\"x\":2}");
    assert!(
        ready(ResponsesViaGemini::prepare_with_state(
            serde_json::from_value(next).unwrap(),
            endpoint(),
            ids(Dialect::OpenAi, Dialect::Gemini),
            &state,
            Default::default()
        ))
        .is_err()
    );
}
#[test]
fn chat_history_restores_gemini_signed_call_without_inventing_native_id() {
    let mut body = output("g");
    body["candidates"][0]["content"]["parts"] = json!([{"functionCall":{"name":"lookup","args":{"x":1}},"thoughtSignature":"actual-signature"}]);
    let host = Host::new(body);
    let store = Store::default();
    let state = state(&store, Dialect::Gemini);
    let mut p = chat_gemini();
    let mut progress = GenerationProgress::default();
    let GenerationOutcome::Success { response, .. } = ready(p.invoke(
        &host,
        &(),
        codec_limits(),
        &state,
        &mut progress,
        gemini_facts,
    ))
    .unwrap() else {
        panic!("rejected")
    };
    let response = serde_json::to_value(response.body).unwrap();
    let call = response["choices"][0]["message"]["tool_calls"][0].clone();
    let mut next = input("h");
    next["messages"] = json!([{"role":"assistant","tool_calls":[call.clone()]},{"role":"tool","tool_call_id":call["id"],"content":"actual result"}]);
    let prepared = ready(ChatViaGemini::prepare_with_state(
        serde_json::from_value(next.clone()).unwrap(),
        endpoint(),
        ids(Dialect::OpenAiChat, Dialect::Gemini),
        &state,
        &Default::default(),
    ))
    .unwrap();
    let target = serde_json::to_value(prepared.target_request()).unwrap();
    let part = &target["contents"][0]["parts"][0];
    assert_eq!(part["thoughtSignature"], "actual-signature");
    assert!(part["functionCall"].get("id").is_none());
    assert!(
        target["contents"][1]["parts"][0]["functionResponse"]
            .get("id")
            .is_none()
    );
    next["messages"][0]["tool_calls"][0]["function"]["arguments"] = json!("{\"x\":2}");
    assert!(
        ready(ChatViaGemini::prepare_with_state(
            serde_json::from_value(next).unwrap(),
            endpoint(),
            ids(Dialect::OpenAiChat, Dialect::Gemini),
            &state,
            &Default::default()
        ))
        .is_err()
    );
}
