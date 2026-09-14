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
    assert!(Endpoint::new("//foreign/path").is_err());
    assert!(
        ChatViaClaude::prepare(
            serde_json::from_value(input("h")).unwrap(),
            "selected",
            endpoint(),
            ids(Dialect::Claude, Dialect::OpenAiChat)
        )
        .is_err()
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
fn duplicate_names_have_distinct_saved_ids_and_truncated_results_recover() {
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
    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
    let replay = ready(state.recover_tools(&ids, &Default::default())).unwrap();
    assert_eq!(replay.names[&ids[0]], "same");
    assert_eq!(replay.names[&ids[1]], "same");
    assert!(replay.original_call_ids.is_empty());
}

#[test]
fn state_failure_blocks_exposure_then_recovers_without_post() {
    let host = gemini_tools_host();
    let store = Store {
        fail_at: Some(2),
        ..Default::default()
    };
    let state = state(&store, Dialect::Gemini);
    let mut p = chat_gemini();
    let mut progress = GenerationProgress::default();
    let error = ready(p.invoke(
        &host,
        &(),
        codec_limits(),
        &state,
        &mut progress,
        gemini_facts,
    ))
    .unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::Conflict);
    assert_eq!(store.entries.lock().unwrap().len(), 1);
    let result = ready(p.recover(codec_limits(), &state, &mut progress, gemini_facts)).unwrap();
    assert!(matches!(result, GenerationOutcome::Success { .. }));
    assert_eq!(host.sent.lock().unwrap().len(), 1);
    assert_eq!(store.entries.lock().unwrap().len(), 3);
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
    let ids = vec!["exact_client_id".into()];
    assert_eq!(
        ready(state.recover_tools(&ids, &Default::default()))
            .unwrap_err()
            .kind(),
        TransformErrorKind::MissingState
    );
    let replay = ready(state.recover_tools(
        &ids,
        &std::collections::BTreeMap::from([("exact_client_id".into(), "lookup".into())]),
    ))
    .unwrap();
    assert_eq!(replay.names["exact_client_id"], "lookup");
}

#[test]
fn truncated_chat_tool_result_uses_saved_name_in_actual_next_request() {
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
    let call_id = response["choices"][0]["message"]["tool_calls"][0]["id"]
        .as_str()
        .unwrap();
    let mut next = input("h");
    next["messages"] = json!([{"role":"tool","tool_call_id":call_id,"content":"actual result"}]);
    let mut p = ready(ChatViaGemini::prepare_with_state(
        serde_json::from_value(next).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::OpenAiChat, Dialect::Gemini),
        &state,
        &Default::default(),
    ))
    .unwrap();
    let target = serde_json::to_value(p.target_request()).unwrap();
    assert_eq!(
        target["contents"][0]["parts"][0]["functionResponse"]["name"],
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
}

#[test]
fn claude_chat_policy_rewrites_collision_prone_ids_and_saves_exact_originals() {
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
    let calls = value["choices"][0]["message"]["tool_calls"]
        .as_array()
        .unwrap();
    let first = calls[0]["id"].as_str().unwrap();
    let second = calls[1]["id"].as_str().unwrap();
    assert_ne!(first, second);
    assert_eq!(second, "a_b");
    let replay =
        ready(state.recover_tools(&[first.into(), second.into()], &Default::default())).unwrap();
    assert_eq!(replay.original_call_ids[first], "a.b");
    assert_eq!(replay.original_call_ids[second], "a_b");
}

#[test]
fn state_expiry_and_record_budget_are_enforced_before_exposure() {
    let host = gemini_tools_host();
    let store = Store::default();
    let mut state = state(&store, Dialect::Gemini);
    state.max_records = 1;
    let mut p = chat_gemini();
    let mut progress = GenerationProgress::default();
    let error = ready(p.invoke(
        &host,
        &(),
        codec_limits(),
        &state,
        &mut progress,
        gemini_facts,
    ))
    .unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::Limit);
    assert!(store.entries.lock().unwrap().is_empty());
    assert!(progress.native_response.is_some());
    state.max_records = 64;
    state.now = state.expires_at;
    let error = ready(p.recover(codec_limits(), &state, &mut progress, gemini_facts)).unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::InvalidInput);
}

#[test]
fn previously_applied_identity_disappearing_blocks_recovery() {
    let host = gemini_tools_host();
    let store = Store {
        fail_at: Some(2),
        ..Default::default()
    };
    let state = state(&store, Dialect::Gemini);
    let mut p = chat_gemini();
    let mut progress = GenerationProgress::default();
    ready(p.invoke(
        &host,
        &(),
        codec_limits(),
        &state,
        &mut progress,
        gemini_facts,
    ))
    .unwrap_err();
    store.entries.lock().unwrap().clear();
    let error = ready(p.recover(codec_limits(), &state, &mut progress, gemini_facts)).unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::MissingState);
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}
