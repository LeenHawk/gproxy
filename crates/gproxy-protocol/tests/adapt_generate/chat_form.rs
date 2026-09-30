use super::*;
use gproxy_protocol::transform::identity::IdSyntax;
fn prime_legacy(store: &Store) -> String {
    let access = state(store, Dialect::OpenAiChat);
    let mut body = output("h");
    body["choices"][0]["message"]["function_call"] =
        json!({"name":"lookup","arguments":"{\"x\":1}"});
    body["choices"][0]["finish_reason"] = json!("function_call");
    let host = Host::new(body);
    let mut p = GeminiViaChat::prepare(
        serde_json::from_value(input("g")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::Gemini, Dialect::OpenAiChat),
    )
    .unwrap();
    let GenerationOutcome::Success { response, .. } = ready(p.invoke(
        &host,
        &(),
        codec_limits(),
        &access,
        &mut GenerationProgress::default(),
        |_| Ok(()),
    ))
    .unwrap() else {
        panic!("rejected")
    };
    let id = response.body.candidates.unwrap()[0]
        .content
        .as_ref()
        .unwrap()
        .parts
        .as_ref()
        .unwrap()
        .iter()
        .find_map(|p| p.function_call.as_ref())
        .unwrap()
        .id
        .clone()
        .unwrap();
    // The alias itself says the call was a legacy function_call without an
    // ID, so nothing is saved for it.
    assert_eq!(id, "call_gpl_0202020202020202_0");
    assert!(store.entries.lock().unwrap().is_empty());
    assert_eq!(host.sent.lock().unwrap().len(), 1);
    id
}
fn next_body() -> Value {
    let mut body = output("h");
    body["id"] = json!("next-native-response");
    body["usage"]["prompt_tokens_details"]["cache_write_tokens"] = json!(0);
    body
}
fn strict_legacy_ids(client: Dialect) -> GenerationIdentity {
    let mut value = ids(client, Dialect::OpenAiChat);
    value.request_policy = value
        .request_policy
        .with_syntax(IdSyntax::AsciiIdentifier)
        .with_max_len(1);
    value
}
fn assert_legacy_request(value: &Value, full: bool) {
    let messages = value["messages"].as_array().unwrap();
    assert_eq!(messages.len(), if full { 2 } else { 1 });
    let result = messages.last().unwrap();
    assert_eq!(result["role"], "function");
    assert_eq!(result["name"], "lookup");
    assert!(result.get("tool_call_id").is_none());
    assert!(result["content"].is_string());
    if full {
        assert_eq!(messages[0]["role"], "assistant");
        assert_eq!(messages[0]["function_call"]["name"], "lookup");
        let args: Value =
            serde_json::from_str(messages[0]["function_call"]["arguments"].as_str().unwrap())
                .unwrap();
        assert_eq!(args, json!({"x":1}));
        assert!(messages[0].get("tool_calls").is_none());
    }
}
fn assert_sent(host: &Host, full: bool) {
    let sent = host.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    let HttpBody::Bytes(bytes) = &sent[0].body else {
        panic!("not bytes")
    };
    let value: Value = serde_json::from_slice(bytes).unwrap();
    assert_legacy_request(&value, full);
}
#[test]
fn actual_legacy_gemini_roundtrip_restores_full_or_truncated_native_chat_form() {
    for full in [false, true] {
        let store = Store::default();
        let call = prime_legacy(&store);
        let access = state(&store, Dialect::OpenAiChat);
        let mut next = input("g");
        let mut contents = Vec::new();
        if full {
            contents.push(json!({"role":"model","parts":[{"functionCall":{"id":call,"name":"lookup","args":{"x":1}}}]}));
        }
        contents.push(json!({"role":"user","parts":[{"functionResponse":{"id":call,"name":"lookup","response":{"actual":"result"}}}]}));
        next["contents"] = json!(contents);
        let mut p = ready(GeminiViaChat::prepare_with_state(
            serde_json::from_value(next).unwrap(),
            endpoint(),
            strict_legacy_ids(Dialect::Gemini),
            &access,
        ))
        .unwrap();
        assert_legacy_request(&serde_json::to_value(p.target_request()).unwrap(), full);
        let host = Host::new(next_body());
        ready(p.invoke(
            &host,
            &(),
            codec_limits(),
            &access,
            &mut GenerationProgress::default(),
            |_| Ok(()),
        ))
        .unwrap();
        assert_sent(&host, full);
    }
}
#[test]
fn claude_legacy_call_restores_before_modern_id_policy_and_a_nameless_result_is_refused() {
    for full in [false, true] {
        let store = Store::default();
        let call = prime_legacy(&store);
        let access = state(&store, Dialect::OpenAiChat);
        let mut next = input("c");
        let mut messages = Vec::new();
        if full {
            messages.push(json!({"role":"assistant","content":[{"type":"tool_use","id":call,"name":"lookup","input":{"x":1}}]}));
        }
        messages.push(json!({"role":"user","content":[{"type":"tool_result","tool_use_id":call,"content":"actual result"}]}));
        next["messages"] = json!(messages);
        let prepared = ready(ClaudeViaChat::prepare_with_state(
            serde_json::from_value(next).unwrap(),
            endpoint(),
            strict_legacy_ids(Dialect::Claude),
            &access,
        ));
        if !full {
            // A Claude result carries no name, and a legacy function message
            // needs one: only the declaring tool_use in history supplies it.
            assert_eq!(
                prepared.unwrap_err().kind(),
                TransformErrorKind::MissingState
            );
            continue;
        }
        let mut p = prepared.unwrap();
        assert_legacy_request(&serde_json::to_value(p.target_request()).unwrap(), full);
        let host = Host::new(next_body());
        ready(p.invoke(
            &host,
            &(),
            codec_limits(),
            &access,
            &mut GenerationProgress::default(),
            |_| Ok(claude_chat::ResponseSupplement::default()),
        ))
        .unwrap();
        assert_sent(&host, full);
    }
}
#[test]
fn responses_legacy_result_and_declared_call_use_native_legacy_fields() {
    for full in [false, true] {
        let store = Store::default();
        let call = prime_legacy(&store);
        let access = state(&store, Dialect::OpenAiChat);
        let mut next = input("r");
        let mut items = Vec::new();
        if full {
            items.push(json!({"type":"function_call","call_id":call,"name":"lookup","arguments":"{\"x\":1}"}));
        }
        items.push(json!({"type":"function_call_output","call_id":call,"output":"actual result"}));
        next["input"] = json!(items);
        let prepared = ready(ResponsesViaChat::prepare_with_state(
            serde_json::from_value(next).unwrap(),
            endpoint(),
            strict_legacy_ids(Dialect::OpenAi),
            &access,
        ));
        if !full {
            // This result names no function, and nothing is saved to name it.
            assert_eq!(
                prepared.unwrap_err().kind(),
                TransformErrorKind::MissingState
            );
            continue;
        }
        let mut p = prepared.unwrap();
        assert_legacy_request(&serde_json::to_value(p.target_request()).unwrap(), full);
        let host = Host::new(next_body());
        ready(p.invoke(
            &host,
            &(),
            codec_limits(),
            &access,
            &mut GenerationProgress::default(),
            |_| {
                Ok(ChatReturnFacts {
                    parallel_tool_calls: true,
                    tool_choice: auto(),
                    prompt_cache_options: None,
                    usage: Default::default(),
                })
            },
        ))
        .unwrap();
        assert_sent(&host, full);
    }
}
#[test]
fn legacy_binding_never_accepts_a_custom_tool_shape() {
    let store = Store::default();
    let call = prime_legacy(&store);
    let access = state(&store, Dialect::OpenAiChat);
    let mut next = input("r");
    next["input"] =
        json!([{"type":"custom_tool_call","call_id":call,"name":"lookup","input":"actual"}]);
    assert!(
        ready(ResponsesViaChat::prepare_with_state(
            serde_json::from_value(next).unwrap(),
            endpoint(),
            ids(Dialect::OpenAi, Dialect::OpenAiChat),
            &access
        ))
        .is_err()
    );
}
#[test]
fn original_modern_chat_ids_restore_exactly() {
    let store = Store::default();
    let access = state(&store, Dialect::OpenAiChat);
    let mut body = output("h");
    body["choices"][0]["message"]["tool_calls"] = json!([{"type":"function","id":"native.actual","function":{"name":"lookup","arguments":"{}"}}]);
    body["choices"][0]["finish_reason"] = json!("tool_calls");
    let host = Host::new(body);
    let mut identity = ids(Dialect::Gemini, Dialect::OpenAiChat);
    identity.response_policy = identity
        .response_policy
        .with_syntax(IdSyntax::AsciiIdentifier);
    let mut p = GeminiViaChat::prepare(
        serde_json::from_value(input("g")).unwrap(),
        "selected",
        endpoint(),
        identity,
    )
    .unwrap();
    let GenerationOutcome::Success { response, .. } = ready(p.invoke(
        &host,
        &(),
        codec_limits(),
        &access,
        &mut GenerationProgress::default(),
        |_| Ok(()),
    ))
    .unwrap() else {
        panic!("rejected")
    };
    let call = response.body.candidates.unwrap()[0]
        .content
        .as_ref()
        .unwrap()
        .parts
        .as_ref()
        .unwrap()
        .iter()
        .find_map(|p| p.function_call.as_ref())
        .unwrap()
        .id
        .clone()
        .unwrap();
    assert_eq!(call, "call_gpe_native_2eactual");
    assert!(store.entries.lock().unwrap().is_empty());
    let mut next = input("g");
    next["contents"] = json!([
        {"role":"model","parts":[{"functionCall":{"id":call,"name":"lookup","args":{}}}]},
        {"role":"user","parts":[{"functionResponse":{"id":call,"name":"lookup","response":{"actual":"result"}}}]}
    ]);
    let p = ready(GeminiViaChat::prepare_with_state(
        serde_json::from_value(next).unwrap(),
        endpoint(),
        ids(Dialect::Gemini, Dialect::OpenAiChat),
        &access,
    ))
    .unwrap();
    let target = serde_json::to_value(p.target_request()).unwrap();
    assert_eq!(
        target["messages"][0]["tool_calls"][0]["id"],
        "native.actual"
    );
    assert_eq!(target["messages"][1]["tool_call_id"], "native.actual");
}

#[test]
fn actual_chat_stream_aliases_tell_modern_missing_id_from_legacy() {
    use gproxy_protocol::{
        adapt::generate::stream::{
            StreamSettings, StreamStart, StreamTarget, event::EventLimits, reader::SourceFraming,
        },
        transform::generate::claude_chat::stream::ChatToClaudeContext,
        wire::claude::{generate_content as c, stream as cs},
    };
    for (legacy, native_id) in [(false, Some("native.actual")), (false, None), (true, None)] {
        let store = Store::default();
        let access = state(&store, Dialect::OpenAiChat);
        let mut delta = if legacy {
            json!({"role":"assistant","function_call":{"name":"lookup","arguments":"{\"x\":1}"}})
        } else {
            json!({"role":"assistant","tool_calls":[{"index":0,"type":"function","function":{"name":"lookup","arguments":"{\"x\":1}"}}]})
        };
        if let Some(id) = native_id {
            delta["tool_calls"][0]["id"] = json!(id);
        }
        let chunk = json!({"id":"native-stream-response","object":"chat.completion.chunk","created":123,"model":"selected","choices":[{"index":0,"delta":delta,"finish_reason":if legacy{"function_call"}else{"tool_calls"}}],"usage":{"prompt_tokens":4,"completion_tokens":2,"total_tokens":6,"prompt_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},"completion_tokens_details":{"reasoning_tokens":0}}});
        let host = Host::raw(200, format!("data: {chunk}\n\ndata: [DONE]\n\n"));
        host.response
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .headers
            .insert(
                http::header::CONTENT_TYPE,
                http::HeaderValue::from_static("text/event-stream"),
            );
        let mut source = input("c");
        source["stream"] = json!(true);
        let mut identity = ids(Dialect::Claude, Dialect::OpenAiChat);
        identity.response_policy = identity
            .response_policy
            .with_syntax(IdSyntax::AsciiIdentifier);
        let settings = StreamSettings {
            codec: codec_limits(),
            events: EventLimits {
                max_bytes: 65536,
                max_pending_bytes: 65536,
            },
            source_framing: SourceFraming::Sse,
            client_framing: SourceFraming::Sse,
        };
        let context=ChatToClaudeContext{start_usage:Some(serde_json::from_value(json!({"input_tokens":4,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens_details":{"thinking_tokens":0}})).unwrap())};
        let mut call = ready(ClaudeViaChat::prepare_stream(
            serde_json::from_value(source).unwrap(),
            StreamTarget {
                endpoint: endpoint(),
                identities: identity,
            },
            context,
            settings,
            &access,
        ))
        .unwrap();
        assert!(matches!(
            ready(call.start(&host, &(), &access)).unwrap(),
            StreamStart::Streaming(_)
        ));
        let mut alias = None;
        while let Some(chunk) = ready(call.next(&access)).unwrap() {
            if let Some(cs::StreamEvent::ContentBlockStart(event)) = chunk.event
                && let c::ResponseContentBlock::ToolUse(tool) = event.content_block
            {
                alias = Some(tool.id);
            }
        }
        let alias = alias.unwrap();
        // The alias alone says what the call was: the escaped upstream ID, a
        // modern call without one, or a legacy function_call.
        assert_eq!(
            alias,
            match (legacy, native_id) {
                (false, Some(_)) => "toolu_gpe_native_2eactual",
                (false, None) => "toolu_gpn_0202020202020202_0",
                (true, _) => "toolu_gpl_0202020202020202_0",
            }
        );
        assert!(store.entries.lock().unwrap().is_empty());
        assert_eq!(host.sent.lock().unwrap().len(), 1);
        let mut next = input("c");
        next["messages"] = json!([
            {"role":"assistant","content":[{"type":"tool_use","id":alias,"name":"lookup","input":{"x":1}}]},
            {"role":"user","content":[{"type":"tool_result","tool_use_id":alias,"content":"actual result"}]}
        ]);
        let prepared = ready(ClaudeViaChat::prepare_with_state(
            serde_json::from_value(next).unwrap(),
            endpoint(),
            ids(Dialect::Claude, Dialect::OpenAiChat),
            &access,
        ))
        .unwrap();
        let value = serde_json::to_value(prepared.target_request()).unwrap();
        if legacy {
            assert_legacy_request(&value, true);
        } else {
            // A modern call goes back under its upstream ID, or under the
            // alias when the upstream sent none.
            let id = native_id.unwrap_or(&alias);
            assert_eq!(value["messages"][0]["tool_calls"][0]["id"], id);
            assert_eq!(value["messages"][1]["role"], "tool");
            assert_eq!(value["messages"][1]["tool_call_id"], id);
        }
        assert!(store.entries.lock().unwrap().is_empty());
        assert_eq!(host.sent.lock().unwrap().len(), 1);
    }
}

#[test]
fn forwarded_modern_gemini_call_is_replayed_from_history_not_state() {
    let store = Store::default();
    let access = state(&store, Dialect::OpenAiChat);
    let mut body = output("h");
    body["choices"][0]["message"]["tool_calls"] = json!([{"type":"function","id":"actual-modern","function":{"name":"lookup","arguments":"{}"}}]);
    body["choices"][0]["finish_reason"] = json!("tool_calls");
    let host = Host::new(body);
    let mut p = GeminiViaChat::prepare(
        serde_json::from_value(input("g")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::Gemini, Dialect::OpenAiChat),
    )
    .unwrap();
    ready(p.invoke(
        &host,
        &(),
        codec_limits(),
        &access,
        &mut GenerationProgress::default(),
        |_| Ok(()),
    ))
    .unwrap();
    // The client saw Chat's own ID, so there is nothing to remember.
    assert!(store.entries.lock().unwrap().is_empty());
    let mut next = input("g");
    next["contents"] = json!([
        {"role":"model","parts":[{"functionCall":{"id":"actual-modern","name":"lookup","args":{}}}]},
        {"role":"user","parts":[{"functionResponse":{"id":"actual-modern","name":"lookup","response":{"actual":"result"}}}]}
    ]);
    let mut p = ready(GeminiViaChat::prepare_with_state(
        serde_json::from_value(next).unwrap(),
        endpoint(),
        ids(Dialect::Gemini, Dialect::OpenAiChat),
        &access,
    ))
    .unwrap();
    let target = serde_json::to_value(p.target_request()).unwrap();
    assert_eq!(
        target["messages"][0]["tool_calls"][0]["id"],
        "actual-modern"
    );
    assert_eq!(target["messages"][1]["role"], "tool");
    assert_eq!(target["messages"][1]["tool_call_id"], "actual-modern");
    let host = Host::new(next_body());
    ready(p.invoke(
        &host,
        &(),
        codec_limits(),
        &access,
        &mut GenerationProgress::default(),
        |_| Ok(()),
    ))
    .unwrap();
    assert_eq!(host.sent.lock().unwrap().len(), 1);
    // Without its call, a result has no binding: Chat needs the declaring
    // assistant message, and nothing was stored to stand in for it.
    let mut truncated = input("g");
    truncated["contents"] = json!([{"role":"user","parts":[{"functionResponse":{"id":"actual-modern","name":"lookup","response":{"actual":"result"}}}]}]);
    assert_eq!(
        ready(GeminiViaChat::prepare_with_state(
            serde_json::from_value(truncated).unwrap(),
            endpoint(),
            ids(Dialect::Gemini, Dialect::OpenAiChat),
            &access,
        ))
        .unwrap_err()
        .kind(),
        TransformErrorKind::MissingMetadata
    );
}
#[test]
fn native_modern_id_matching_another_legacy_client_alias_does_not_change_its_form() {
    use gproxy_protocol::transform::identity::IdNamespace;
    for gemini in [false, true] {
        let store = Store::default();
        let legacy = prime_legacy(&store);
        let access = state(&store, Dialect::OpenAiChat);
        let mut body = output("h");
        body["id"] = json!("modern-response");
        body["choices"][0]["message"]["tool_calls"] = json!([{"type":"function","id":legacy,"function":{"name":"modern_name","arguments":"{\"y\":2}"}}]);
        body["choices"][0]["finish_reason"] = json!("tool_calls");
        let host = Host::new(body);
        let mut identity = GenerationIdentity::new(
            IdNamespace([7; 16]),
            IdNamespace([8; 16]),
            Dialect::Gemini,
            Dialect::OpenAiChat,
        )
        .unwrap();
        identity.response_policy.preserve_source_ids = false;
        let mut p = GeminiViaChat::prepare(
            serde_json::from_value(input("g")).unwrap(),
            "selected",
            endpoint(),
            identity,
        )
        .unwrap();
        let GenerationOutcome::Success { response, .. } = ready(p.invoke(
            &host,
            &(),
            codec_limits(),
            &access,
            &mut GenerationProgress::default(),
            |_| Ok(()),
        ))
        .unwrap() else {
            panic!("rejected")
        };
        let modern = response.body.candidates.unwrap()[0]
            .content
            .as_ref()
            .unwrap()
            .parts
            .as_ref()
            .unwrap()
            .iter()
            .find_map(|p| p.function_call.as_ref())
            .unwrap()
            .id
            .clone()
            .unwrap();
        assert_ne!(modern, legacy);
        let target = if gemini {
            let mut request = input("g");
            request["contents"] = json!([{"role":"model","parts":[{"functionCall":{"id":legacy,"name":"lookup","args":{"x":1}}},{"functionCall":{"id":modern,"name":"modern_name","args":{"y":2}}}]},{"role":"user","parts":[{"functionResponse":{"id":legacy,"name":"lookup","response":{}}},{"functionResponse":{"id":modern,"name":"modern_name","response":{}}}]}]);
            serde_json::to_value(
                ready(GeminiViaChat::prepare_with_state(
                    serde_json::from_value(request).unwrap(),
                    endpoint(),
                    ids(Dialect::Gemini, Dialect::OpenAiChat),
                    &access,
                ))
                .unwrap()
                .target_request(),
            )
            .unwrap()
        } else {
            let mut request = input("c");
            request["messages"] = json!([{"role":"assistant","content":[{"type":"tool_use","id":legacy,"name":"lookup","input":{"x":1}},{"type":"tool_use","id":modern,"name":"modern_name","input":{"y":2}}]},{"role":"user","content":[{"type":"tool_result","tool_use_id":legacy,"content":"one"},{"type":"tool_result","tool_use_id":modern,"content":"two"}]}]);
            serde_json::to_value(
                ready(ClaudeViaChat::prepare_with_state(
                    serde_json::from_value(request).unwrap(),
                    endpoint(),
                    ids(Dialect::Claude, Dialect::OpenAiChat),
                    &access,
                ))
                .unwrap()
                .target_request(),
            )
            .unwrap()
        };
        let messages = target["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 4);
        assert_eq!(messages[0]["function_call"]["name"], "lookup");
        assert_eq!(
            messages[1]["tool_calls"][0]["function"]["name"],
            "modern_name"
        );
        assert_eq!(messages[1]["tool_calls"][0]["id"], legacy);
        assert_eq!(messages[2]["role"], "function");
        assert_eq!(messages[3]["tool_call_id"], legacy);
    }
}
