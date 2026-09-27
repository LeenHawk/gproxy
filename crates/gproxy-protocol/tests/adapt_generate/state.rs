use super::*;

#[test]
fn conversion_failure_retains_native_and_recovery_does_not_resend() {
    let store = Store::default();
    let state = state(&store, Dialect::Claude);
    let host = Host::new(output("c"));
    let mut p = chat_via_claude();
    let mut progress = GenerationProgress::default();
    let error = ready(
        p.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
            Err(TransformError::missing_metadata("clock"))
        }),
    )
    .unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::MissingMetadata);
    assert!(progress.native_response.is_some());
    let converted = p
        .convert_response(
            progress.native_response.clone().unwrap(),
            claude_chat::ResponseSupplement {
                created_unix_seconds: Some(123),
            },
        )
        .unwrap();
    assert_eq!(
        converted.value.choices[0].message.content.as_deref(),
        Some("answer")
    );
    let error = ready(
        p.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
            Ok(Default::default())
        }),
    )
    .unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::Conflict);
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}

#[test]
fn malformed_native_retains_raw_result() {
    let store = Store::default();
    let state = state(&store, Dialect::Claude);
    let host = Host::raw(200, "{broken".into());
    let mut p = chat_via_claude();
    let mut progress = GenerationProgress::default();
    let error = ready(
        p.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
            Ok(Default::default())
        }),
    )
    .unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::InvalidResult);
    assert_eq!(progress.raw_response.unwrap().body, "{broken");
    assert!(progress.native_response.is_none());
}

#[test]
fn rejected_http_is_retained_without_response_conversion() {
    let store = Store::default();
    let state = state(&store, Dialect::Claude);
    let host = Host::raw(429, "quota details".into());
    let mut p = chat_via_claude();
    let mut progress = GenerationProgress::default();
    let result = ready(
        p.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
            panic!("must not map an error")
        }),
    )
    .unwrap();
    let GenerationOutcome::Rejected(raw) = result else {
        panic!("unexpected success")
    };
    assert_eq!(raw.status, 429);
    assert_eq!(raw.body, "quota details");
}

#[test]
fn cancelled_send_records_side_effect_and_cannot_repost() {
    let store = Store::default();
    let state = state(&store, Dialect::Claude);
    let host = Host {
        response: Mutex::new(None),
        sent: Mutex::new(Vec::new()),
    };
    let mut p = chat_via_claude();
    let mut progress = GenerationProgress::default();
    {
        let future = p.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
            Ok(Default::default())
        });
        let mut future = Box::pin(future);
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(future.as_mut().poll(&mut cx).is_pending());
    }
    assert!(progress.send_started);
    assert_eq!(host.sent.lock().unwrap().len(), 1);
    let error = ready(
        p.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
            Ok(Default::default())
        }),
    )
    .unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::Conflict);
}

#[test]
fn invalid_endpoint_policy_or_stream_rejects_before_post() {
    assert!(Endpoint::new("//foreign/path").is_ok());
    assert!(
        ChatViaClaude::prepare(
            serde_json::from_value(input("h")).unwrap(),
            "selected",
            endpoint(),
            ids(Dialect::Claude, Dialect::OpenAiChat)
        )
        .is_ok()
    );
    let mut request = input("h");
    request["stream"] = json!(true);
    assert!(
        ChatViaClaude::prepare(
            serde_json::from_value(request).unwrap(),
            "selected",
            endpoint(),
            ids(Dialect::OpenAiChat, Dialect::Claude)
        )
        .is_err()
    );
}

#[test]
fn duplicate_names_get_distinct_aliases_and_nothing_is_saved() {
    let host = gemini_tools_host();
    let store = Store::default();
    let state = state(&store, Dialect::Gemini);
    let mut prepared = chat_gemini();
    let mut progress = GenerationProgress::default();
    let result = ready(prepared.invoke(
        &host,
        &(),
        codec_limits(),
        &state,
        &mut progress,
        gemini_facts,
    ))
    .unwrap();
    let GenerationOutcome::Success { response, .. } = result else {
        panic!("rejected")
    };
    let value = serde_json::to_value(response.body).unwrap();
    let ids: Vec<String> = value["choices"][0]["message"]["tool_calls"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["id"].as_str().unwrap().into())
        .collect();
    // Gemini sent neither call an ID; each alias says so and carries its
    // position, so the two stay apart without a record.
    assert_eq!(
        ids,
        ["call_gpn_0202020202020202_0", "call_gpn_0202020202020202_1"]
    );
    assert!(store.entries.lock().unwrap().is_empty());
}

#[test]
fn recovery_rejects_changed_scope_request_or_exposed_response() {
    let host = Host::new(output("c"));
    let store = Store::default();
    let mut state = state(&store, Dialect::Claude);
    let mut p = chat_via_claude();
    let mut progress = GenerationProgress::default();
    ready(
        p.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
            Ok(claude_chat::ResponseSupplement {
                created_unix_seconds: Some(123),
            })
        }),
    )
    .unwrap();
    let error = ready(p.recover(codec_limits(), &state, &mut progress, |_| {
        Ok(claude_chat::ResponseSupplement {
            created_unix_seconds: Some(124),
        })
    }))
    .unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::Conflict);
    state.conversation_key = "different".into();
    let error = ready(p.recover(codec_limits(), &state, &mut progress, |_| {
        Ok(Default::default())
    }))
    .unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::Conflict);
}

#[test]
fn complete_history_supplies_missing_names_but_ambiguous_truncation_fails() {
    let store = Store::default();
    let state = state(&store, Dialect::Gemini);
    let prepare = |messages: serde_json::Value| {
        let mut next = input("c");
        next["messages"] = messages;
        ready(ClaudeViaGemini::prepare_with_state(
            serde_json::from_value(next).unwrap(),
            endpoint(),
            ids(Dialect::Claude, Dialect::Gemini),
            &state,
            Default::default(),
        ))
    };
    let result = json!({"role":"user","content":[{"type":"tool_result","tool_use_id":"exact_client_id","content":"done"}]});
    assert_eq!(
        prepare(json!([result.clone()])).unwrap_err().kind(),
        TransformErrorKind::MissingState
    );
    let prepared = prepare(json!([
        {"role":"assistant","content":[{"type":"tool_use","id":"exact_client_id","name":"lookup","input":{}}]},
        result
    ]))
    .unwrap();
    let target = serde_json::to_value(prepared.target_request()).unwrap();
    assert_eq!(
        target["contents"][1]["parts"][0]["functionResponse"]["name"],
        "lookup"
    );
    assert!(store.entries.lock().unwrap().is_empty());
}

#[test]
fn chat_tool_result_takes_its_name_from_history_and_a_truncated_one_is_refused() {
    let host = gemini_tools_host();
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
    let call_id = call["id"].as_str().unwrap();
    let mut next = input("h");
    next["messages"] = json!([
        {"role":"assistant","tool_calls":[call]},
        {"role":"tool","tool_call_id":call_id,"content":"actual result"}
    ]);
    let mut p = ready(ChatViaGemini::prepare_with_state(
        serde_json::from_value(next).unwrap(),
        endpoint(),
        ids(Dialect::OpenAiChat, Dialect::Gemini),
        &state,
        &Default::default(),
    ))
    .unwrap();
    let target = serde_json::to_value(p.target_request()).unwrap();
    assert_eq!(
        target["contents"][0]["parts"][0]["functionCall"]["name"],
        "same"
    );
    assert_eq!(
        target["contents"][1]["parts"][0]["functionResponse"]["name"],
        "same"
    );
    let mut new_body = output("g");
    new_body["responseId"] = json!("different-next-response");
    let next_host = Host::new(new_body);
    let mut progress = GenerationProgress::default();
    ready(p.invoke(
        &next_host,
        &(),
        codec_limits(),
        &state,
        &mut progress,
        gemini_facts,
    ))
    .unwrap();
    assert_eq!(next_host.sent.lock().unwrap().len(), 1);
    assert!(store.entries.lock().unwrap().is_empty());
    // A Chat result carries no name, and nothing was saved to supply one, so
    // a result without its call cannot become a Gemini functionResponse.
    let mut truncated = input("h");
    truncated["messages"] =
        json!([{"role":"tool","tool_call_id":call_id,"content":"actual result"}]);
    assert_eq!(
        ready(ChatViaGemini::prepare_with_state(
            serde_json::from_value(truncated).unwrap(),
            endpoint(),
            ids(Dialect::OpenAiChat, Dialect::Gemini),
            &state,
            &Default::default(),
        ))
        .unwrap_err()
        .kind(),
        TransformErrorKind::MissingState
    );
}

#[test]
fn claude_chat_policy_escapes_collision_prone_ids_and_decodes_them_back() {
    let mut body = output("c");
    body["stop_reason"] = json!("tool_use");
    body["content"] = json!([{"type":"tool_use","id":"a.b","name":"same","input":{}},{"type":"tool_use","id":"a_b","name":"same","input":{}}]);
    let host = Host::new(body);
    let store = Store::default();
    let state = state(&store, Dialect::Claude);
    let mut identities = ids(Dialect::OpenAiChat, Dialect::Claude);
    identities.response_policy = identities
        .response_policy
        .with_syntax(gproxy_protocol::transform::identity::IdSyntax::AsciiIdentifier);
    let mut p = ChatViaClaude::prepare(
        serde_json::from_value(input("h")).unwrap(),
        "selected",
        endpoint(),
        identities,
    )
    .unwrap();
    let mut progress = GenerationProgress::default();
    let GenerationOutcome::Success { response, .. } =
        ready(
            p.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
                Ok(claude_chat::ResponseSupplement {
                    created_unix_seconds: Some(123),
                })
            }),
        )
        .unwrap()
    else {
        panic!("rejected")
    };
    let value = serde_json::to_value(response.body).unwrap();
    let calls = value["choices"][0]["message"]["tool_calls"].clone();
    // `a_b` reaches the client as Claude sent it; `a.b` is escaped, and its
    // escape spells `_` as `__`, so it cannot collide with `a_b`.
    assert_eq!(calls[0]["id"], "call_gpe_a_2eb");
    assert_eq!(calls[1]["id"], "a_b");
    assert!(store.entries.lock().unwrap().is_empty());
    let mut next = input("h");
    next["messages"] = json!([
        {"role":"assistant","tool_calls":calls},
        {"role":"tool","tool_call_id":"call_gpe_a_2eb","content":"one"},
        {"role":"tool","tool_call_id":"a_b","content":"two"}
    ]);
    let p = ready(ChatViaClaude::prepare_with_state(
        serde_json::from_value(next).unwrap(),
        endpoint(),
        ids(Dialect::OpenAiChat, Dialect::Claude),
        &state,
    ))
    .unwrap();
    let target = serde_json::to_value(p.target_request()).unwrap();
    let messages = target["messages"].as_array().unwrap();
    let blocks = messages
        .iter()
        .flat_map(|m| m["content"].as_array().into_iter().flatten())
        .collect::<Vec<_>>();
    let uses: Vec<_> = blocks
        .iter()
        .filter(|b| b["type"] == "tool_use")
        .map(|b| b["id"].clone())
        .collect();
    let results: Vec<_> = blocks
        .iter()
        .filter(|b| b["type"] == "tool_result")
        .map(|b| b["tool_use_id"].clone())
        .collect();
    assert_eq!(uses, [json!("a.b"), json!("a_b")]);
    assert_eq!(results, [json!("a.b"), json!("a_b")]);
    assert!(store.entries.lock().unwrap().is_empty());
}

#[test]
fn state_expiry_is_enforced_before_exposure() {
    let host = signed_gemini_tools_host();
    let store = Store::default();
    let mut state = state(&store, Dialect::Gemini);
    let mut p = chat_gemini();
    let mut progress = GenerationProgress::default();
    let error = ready(
        p.invoke(&host, &(), codec_limits(), &state, &mut progress, |_| {
            Err(TransformError::missing_metadata("clock"))
        }),
    )
    .unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::MissingMetadata);
    assert!(progress.native_response.is_some());
    state.now = state.expires_at;
    let error = ready(p.recover(codec_limits(), &state, &mut progress, gemini_facts)).unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::InvalidInput);
    assert!(store.entries.lock().unwrap().is_empty());
}

/// The non-stream path hit the same collision: a repeated upstream response
/// ID made the second response's identity CAS fail after its POST.
#[test]
fn repeated_upstream_ids_across_responses_in_one_conversation_both_succeed() {
    let store = Store::default();
    let state = state(&store, Dialect::Claude);
    for _ in 0..2 {
        let mut body = output("c");
        body["content"] = json!([{"type":"tool_use","id":"toolu_mock","name":"same","input":{}}]);
        body["stop_reason"] = json!("tool_use");
        let host = Host::new(body);
        let mut p = chat_via_claude();
        let GenerationOutcome::Success { response, .. } = ready(p.invoke(
            &host,
            &(),
            codec_limits(),
            &state,
            &mut GenerationProgress::default(),
            |_| {
                Ok(claude_chat::ResponseSupplement {
                    created_unix_seconds: Some(123),
                })
            },
        ))
        .unwrap() else {
            panic!("rejected")
        };
        let value = serde_json::to_value(response.body).unwrap();
        assert_eq!(value["id"], "msg-native");
        assert_eq!(
            value["choices"][0]["message"]["tool_calls"][0]["id"],
            "toolu_mock"
        );
        assert!(store.entries.lock().unwrap().is_empty());
    }
}
