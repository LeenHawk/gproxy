use super::*;
use gproxy_protocol::{
    adapt::generate::{
        claude_responses::ResponsesViaClaude, gemini_responses::ResponsesViaGemini,
        stream::bridge::StreamBridge,
    },
    transform::generate::stream as native,
    wire::openai::responses::stream as rs,
};
#[path = "../support/client_tools_backends.rs"]
mod fixtures;

fn run<B: StreamBridge<ClientEvent = rs::StreamEvent>>(
    mut call: StreamInvocation<B>,
    source: Vec<String>,
    backend: Dialect,
    store: &Arc<Store>,
    expected_calls: usize,
) -> Value {
    let access = all_pairs::access(store, backend);
    let feed = Feed::default();
    for event in source {
        feed.push(format!("data: {event}\n\n"));
    }
    feed.close();
    let host = Host::stream(store.clone(), feed);
    ready(call.start(&host, &(), &access)).unwrap();
    let mut observed = Vec::new();
    while let Some(chunk) = ready(call.next(&access)).unwrap() {
        for line in std::str::from_utf8(&chunk.bytes).unwrap().lines() {
            let Some(data) = line.strip_prefix("data: ") else {
                continue;
            };
            let event: Value = serde_json::from_str(data).unwrap();
            if let Some(id) = event["item"]["call_id"].as_str() {
                observed.push(id.to_owned());
            }
        }
    }
    assert_eq!(observed.len(), expected_calls * 2);
    // The dotted native IDs were rewritten into reversible aliases, which
    // name the native ID themselves, so nothing is persisted for a call.
    assert!(
        store
            .entries
            .lock()
            .unwrap()
            .keys()
            .all(|key| !key.starts_with("stream:") && !key.starts_with("generate:"))
    );
    for id in &observed {
        assert!(id.starts_with("call_gpe_native_2e"), "{id}");
    }
    assert_eq!(host.sent.lock().unwrap().len(), 1);
    serde_json::to_value(call.client_result().unwrap()).unwrap()
}

#[test]
fn claude_client_tools_stream_persist_and_resume_from_previous_response() {
    let store = Arc::new(Store::default());
    let access = all_pairs::access(&store, Dialect::Claude);
    let call = ready(ResponsesViaClaude::prepare_stream(
        serde_json::from_value(fixtures::request(true)).unwrap(),
        all_pairs::target(Dialect::OpenAi, Dialect::Claude),
        ResponsesViaClaudeStreamFacts {
            request: Default::default(),
            response: all_pairs::claude_response_context(),
        },
        settings(),
        &access,
    ))
    .unwrap();
    let request = serde_json::to_value(call.target_request()).unwrap();
    let names: Vec<_> = request["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect();
    let mut events = native::claude::synthesize_claude_stream(
        serde_json::from_value(fixtures::claude(&names)).unwrap(),
        Default::default(),
    )
    .unwrap()
    .value;
    // Start every call before finishing any: later blocks must wait behind an
    // unfinished native action without losing the Responses output order.
    use gproxy_protocol::wire::claude::stream::StreamEvent as C;
    events.sort_by_key(|event| match event {
        C::MessageStart(_) => (0, 0),
        C::ContentBlockStart(v) => (1, v.index),
        C::ContentBlockDelta(v) => (2, -v.index),
        C::ContentBlockStop(v) => (3, -v.index),
        C::MessageDelta(_) => (4, 0),
        C::MessageStop(_) => (5, 0),
        _ => (6, 0),
    });
    let events = events
        .into_iter()
        .map(|e| serde_json::to_string(&e).unwrap())
        .collect();
    let output = run(call, events, Dialect::Claude, &store, 4);
    let mut followup = fixtures::followup(&output);
    followup["stream"] = json!(true);
    followup["previous_response_id"] = output["id"].clone();
    let answered = output["output"].as_array().unwrap().len();
    followup["input"].as_array_mut().unwrap().drain(..answered);
    let mut target = all_pairs::target(Dialect::OpenAi, Dialect::Claude);
    target.identities = GenerationIdentity::new(
        IdNamespace([101; 16]),
        IdNamespace([102; 16]),
        Dialect::OpenAi,
        Dialect::Claude,
    )
    .unwrap();
    let next = ready(ResponsesViaClaude::prepare_stream(
        serde_json::from_value(followup).unwrap(),
        target,
        ResponsesViaClaudeStreamFacts {
            request: Default::default(),
            response: all_pairs::claude_response_context(),
        },
        settings(),
        &access,
    ))
    .unwrap();
    let next = serde_json::to_value(next.target_request()).unwrap();
    assert_eq!(next["tools"].as_array().unwrap().len(), 5);
    assert!(next["messages"].to_string().contains("native.0"));
    assert!(next["messages"].to_string().contains("context mismatch"));
}

#[test]
fn gemini_client_tools_carry_signed_parts_and_resume_discovery_history() {
    let store = Arc::new(Store::default());
    let access = all_pairs::access(&store, Dialect::Gemini);
    let call = ready(ResponsesViaGemini::prepare_stream(
        serde_json::from_value(fixtures::request(true)).unwrap(),
        all_pairs::target(Dialect::OpenAi, Dialect::Gemini),
        ResponsesViaGeminiStreamFacts {
            request: Default::default(),
            response: all_pairs::gemini_response_context(),
        },
        settings(),
        &access,
    ))
    .unwrap();
    let request = serde_json::to_value(call.target_request()).unwrap();
    let names: Vec<_> = request["tools"][0]["functionDeclarations"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect();
    let source = fixtures::gemini(&names, true);
    let events = native::gemini::synthesize_gemini_stream(
        serde_json::from_value(source.clone()).unwrap(),
        Default::default(),
    )
    .unwrap()
    .value
    .into_iter()
    .map(|e| serde_json::to_string(&e).unwrap())
    .collect();
    let output = run(call, events, Dialect::Gemini, &store, 4);
    let followup = fixtures::followup(&output);
    let next = ready(ResponsesViaGemini::prepare_with_state(
        serde_json::from_value(followup.clone()).unwrap(),
        Endpoint::new("/generate").unwrap(),
        GenerationIdentity::new(
            IdNamespace([103; 16]),
            IdNamespace([104; 16]),
            Dialect::OpenAi,
            Dialect::Gemini,
        )
        .unwrap(),
        &access,
        Default::default(),
    ))
    .unwrap();
    let next = serde_json::to_value(next.target_request()).unwrap();
    let calls: Vec<_> = next["contents"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|c| c["parts"].as_array().into_iter().flatten())
        .filter(|p| p.get("functionCall").is_some())
        .cloned()
        .collect();
    assert_eq!(
        calls,
        source["candidates"][0]["content"]["parts"]
            .as_array()
            .unwrap()
            .clone()
    );
    let mut tampered = followup.clone();
    let shell = tampered["input"]
        .as_array()
        .unwrap()
        .iter()
        .position(|item| item["type"] == "shell_call")
        .unwrap();
    tampered["input"][shell]["action"]["commands"] = json!(["changed"]);
    // The call is rebuilt from what the client sends, with its carried
    // signature: a changed call goes to the upstream, which decides.
    let tampered = ready(ResponsesViaGemini::prepare_with_state(
        serde_json::from_value(tampered).unwrap(),
        Endpoint::new("/generate").unwrap(),
        GenerationIdentity::new(
            IdNamespace([105; 16]),
            IdNamespace([106; 16]),
            Dialect::OpenAi,
            Dialect::Gemini,
        )
        .unwrap(),
        &access,
        Default::default(),
    ))
    .unwrap();
    let tampered = serde_json::to_value(tampered.target_request()).unwrap();
    assert!(tampered.to_string().contains("changed"));
    assert!(tampered.to_string().contains("c2lnbmF0dXJl"));
    let mut followup = followup;
    followup["stream"] = json!(true);
    followup["previous_response_id"] = output["id"].clone();
    let answered = output["output"].as_array().unwrap().len();
    followup["input"].as_array_mut().unwrap().drain(..answered);
    let mut target = all_pairs::target(Dialect::OpenAi, Dialect::Gemini);
    target.identities = GenerationIdentity::new(
        IdNamespace([107; 16]),
        IdNamespace([108; 16]),
        Dialect::OpenAi,
        Dialect::Gemini,
    )
    .unwrap();
    let resumed = ready(ResponsesViaGemini::prepare_stream(
        serde_json::from_value(followup).unwrap(),
        target,
        ResponsesViaGeminiStreamFacts {
            request: Default::default(),
            response: all_pairs::gemini_response_context(),
        },
        settings(),
        &access,
    ))
    .unwrap();
    let resumed = serde_json::to_value(resumed.target_request()).unwrap();
    assert_eq!(
        resumed["tools"][0]["functionDeclarations"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert!(resumed["contents"].to_string().contains("c2lnbmF0dXJl"));
}

#[test]
fn freeform_patch_streams_finish_with_the_original_client_input() {
    let patch = "*** Begin Patch\n*** Add File: probe.txt\n+passed\n*** End Patch";
    let input = json!({"model":"client","stream":true,"max_output_tokens":128,"input":"edit",
        "tools":[{"type":"custom","name":"apply_patch","format":{"type":"text"}}]});
    for backend in [Dialect::Claude, Dialect::Gemini] {
        let store = Arc::new(Store::default());
        let access = all_pairs::access(&store, backend);
        let output = if backend == Dialect::Claude {
            let call = ready(ResponsesViaClaude::prepare_stream(
                serde_json::from_value(input.clone()).unwrap(),
                all_pairs::target(Dialect::OpenAi, backend),
                ResponsesViaClaudeStreamFacts {
                    request: Default::default(),
                    response: all_pairs::claude_response_context(),
                },
                settings(),
                &access,
            ))
            .unwrap();
            let request = serde_json::to_value(call.target_request()).unwrap();
            let names = vec![request["tools"][0]["name"].as_str().unwrap().to_owned()];
            let mut source = fixtures::claude(&names);
            source["content"][0]["input"] = json!({"input":patch});
            let events = native::claude::synthesize_claude_stream(
                serde_json::from_value(source).unwrap(),
                Default::default(),
            )
            .unwrap()
            .value
            .into_iter()
            .map(|event| serde_json::to_string(&event).unwrap())
            .collect();
            run(call, events, backend, &store, 1)
        } else {
            let call = ready(ResponsesViaGemini::prepare_stream(
                serde_json::from_value(input.clone()).unwrap(),
                all_pairs::target(Dialect::OpenAi, backend),
                ResponsesViaGeminiStreamFacts {
                    request: Default::default(),
                    response: all_pairs::gemini_response_context(),
                },
                settings(),
                &access,
            ))
            .unwrap();
            let request = serde_json::to_value(call.target_request()).unwrap();
            let names = vec![
                request["tools"][0]["functionDeclarations"][0]["name"]
                    .as_str()
                    .unwrap()
                    .to_owned(),
            ];
            let mut source = fixtures::gemini(&names, false);
            source["candidates"][0]["content"]["parts"][0]["functionCall"]["args"] =
                json!({"input":patch});
            let events = native::gemini::synthesize_gemini_stream(
                serde_json::from_value(source).unwrap(),
                Default::default(),
            )
            .unwrap()
            .value
            .into_iter()
            .map(|event| serde_json::to_string(&event).unwrap())
            .collect();
            run(call, events, backend, &store, 1)
        };
        assert_eq!(output["output"][0]["type"], "custom_tool_call");
        assert_eq!(output["output"][0]["name"], "apply_patch");
        assert_eq!(output["output"][0]["input"], patch);
    }
}

#[test]
fn malformed_custom_streams_keep_following_text() {
    for keep_valid in [false, true] {
        let expected_calls = usize::from(keep_valid);
        for args in [json!({}), json!({"input":null}), json!({"input":42})] {
            let input = json!({"model":"client","stream":true,"max_output_tokens":128,"input":"edit",
        "tools":[{"type":"custom","name":"apply_patch","format":{"type":"text"}}]});
            for backend in [Dialect::Claude, Dialect::Gemini] {
                let store = Arc::new(Store::default());
                let access = all_pairs::access(&store, backend);
                let output = if backend == Dialect::Claude {
                    let call = ready(ResponsesViaClaude::prepare_stream(
                        serde_json::from_value(input.clone()).unwrap(),
                        all_pairs::target(Dialect::OpenAi, backend),
                        ResponsesViaClaudeStreamFacts {
                            request: Default::default(),
                            response: all_pairs::claude_response_context(),
                        },
                        settings(),
                        &access,
                    ))
                    .unwrap();
                    let request = serde_json::to_value(call.target_request()).unwrap();
                    let names = vec![request["tools"][0]["name"].as_str().unwrap().to_owned()];
                    let mut source = fixtures::claude(&names);
                    source["content"][0]["input"] = args.clone();
                    if keep_valid {
                        source["content"].as_array_mut().unwrap().push(json!({
                        "type":"tool_use", "id":"native.1", "name":names[0], "input":{"input":"valid"}
                    }));
                    }
                    source["content"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({"type":"text","text":"kept"}));
                    let events = native::claude::synthesize_claude_stream(
                        serde_json::from_value(source).unwrap(),
                        Default::default(),
                    )
                    .unwrap()
                    .value
                    .into_iter()
                    .map(|event| serde_json::to_string(&event).unwrap())
                    .collect();
                    run(call, events, backend, &store, expected_calls)
                } else {
                    let call = ready(ResponsesViaGemini::prepare_stream(
                        serde_json::from_value(input.clone()).unwrap(),
                        all_pairs::target(Dialect::OpenAi, backend),
                        ResponsesViaGeminiStreamFacts {
                            request: Default::default(),
                            response: all_pairs::gemini_response_context(),
                        },
                        settings(),
                        &access,
                    ))
                    .unwrap();
                    let request = serde_json::to_value(call.target_request()).unwrap();
                    let names = vec![
                        request["tools"][0]["functionDeclarations"][0]["name"]
                            .as_str()
                            .unwrap()
                            .to_owned(),
                    ];
                    let mut source = fixtures::gemini(&names, true);
                    source["candidates"][0]["content"]["parts"][0]["functionCall"]["args"] =
                        args.clone();
                    if keep_valid {
                        source["candidates"][0]["content"]["parts"].as_array_mut().unwrap().push(json!({
                        "functionCall":{"id":"native.1", "name":names[0], "args":{"input":"valid"}},
                        "thoughtSignature":"dmFsaWQ="
                    }));
                    }
                    source["candidates"][0]["content"]["parts"]
                        .as_array_mut()
                        .unwrap()
                        .push(json!({"text":"kept"}));
                    let events = native::gemini::synthesize_gemini_stream(
                        serde_json::from_value(source).unwrap(),
                        Default::default(),
                    )
                    .unwrap()
                    .value
                    .into_iter()
                    .map(|event| serde_json::to_string(&event).unwrap())
                    .collect();
                    run(call, events, backend, &store, expected_calls)
                };
                // A signed Gemini call carries its signature on a reasoning
                // item right before it; the omitted call carries none.
                let carriers = usize::from(backend == Dialect::Gemini) * expected_calls;
                if keep_valid {
                    let call = &output["output"][carriers];
                    assert_eq!(call["input"], "valid");
                    // The alias names the dotted native ID.
                    assert_eq!(call["call_id"], "call_gpe_native_2e1");
                    if carriers > 0 {
                        assert_eq!(
                            output["output"][0]["encrypted_content"],
                            "gemini-next:dmFsaWQ="
                        );
                    }
                }
                let last = carriers + expected_calls;
                assert_eq!(output["output"].as_array().unwrap().len(), 1 + last);
                assert_eq!(output["output"][last]["type"], "message");
                assert_eq!(output["output"][last]["content"][0]["text"], "kept");
            }
        }
    }
}
