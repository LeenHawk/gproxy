#[path = "support/client_tools.rs"]
mod client_tools_base;
#[path = "support/client_tools_backends.rs"]
mod fixtures;
use gproxy_protocol::{
    Dialect,
    transform::{
        count_tokens::request as count,
        generate::{claude_responses as c, gemini_responses as g},
        identity::*,
    },
    wire::openai::responses as r,
};
use serde_json::{Value, json};
fn flow() -> IdentityFlow {
    IdentityFlow::new(IdNamespace([97; 16]))
}
fn policy() -> TargetIdPolicy {
    TargetIdPolicy::new(Dialect::OpenAi).with_syntax(IdSyntax::AsciiIdentifier)
}
fn request(backend: Dialect, input: Value) -> Value {
    match backend {
        Dialect::Claude => serde_json::to_value(
            c::responses_to_claude_request(
                serde_json::from_value(input).unwrap(),
                "selected",
                Default::default(),
            )
            .unwrap()
            .value,
        )
        .unwrap(),
        Dialect::Gemini => serde_json::to_value(
            g::responses_to_gemini_request(
                serde_json::from_value(input).unwrap(),
                "selected",
                Default::default(),
            )
            .unwrap()
            .value,
        )
        .unwrap(),
        _ => unreachable!(),
    }
}
fn names(backend: Dialect, value: &Value) -> Vec<String> {
    let tools = if backend == Dialect::Claude {
        &value["tools"]
    } else {
        &value["tools"][0]["functionDeclarations"]
    };
    tools
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn freeform_client_patch_round_trips_through_both_function_only_backends() {
    let patch = "*** Begin Patch\n*** Add File: probe.txt\n+passed\n*** End Patch";
    for backend in [Dialect::Claude, Dialect::Gemini] {
        let original = json!({
            "model":"client", "input":"edit", "max_output_tokens":128,
            "tools":[{"type":"custom","name":"apply_patch","description":"Edit files with a patch","format":{"type":"text"}}]
        });
        let target = request(backend, original.clone());
        let aliases = names(backend, &target);
        assert_eq!(aliases.len(), 1);
        let context_request = serde_json::from_value(original.clone()).unwrap();
        let output = if backend == Dialect::Claude {
            let mut native = fixtures::claude(&aliases);
            native["content"][0]["input"] = json!({"input":patch});
            c::claude_to_responses_response(
                serde_json::from_value(native).unwrap(),
                c::ClaudeResponseContext {
                    request: context_request,
                    created_at: 7,
                    effective_parallel_tool_calls: true,
                    effective_tool_choice: r::ToolChoice::Mode(r::ToolChoiceMode::Auto),
                    usage: Default::default(),
                    effective_prompt_cache_options: None,
                },
                &mut flow(),
                &policy(),
            )
            .unwrap()
            .value
        } else {
            let mut native = fixtures::gemini(&aliases, false);
            native["candidates"][0]["content"]["parts"][0]["functionCall"]["args"] =
                json!({"input":patch});
            g::gemini_to_responses_response(
                serde_json::from_value(native).unwrap(),
                g::GeminiResponseContext {
                    request: context_request,
                    created_at: 7,
                    effective_parallel_tool_calls: true,
                    effective_tool_choice: r::ToolChoice::Mode(r::ToolChoiceMode::Auto),
                    usage: g::GeminiUsageFacts {
                        cache_write_tokens: Some(0),
                        ..Default::default()
                    },
                    effective_prompt_cache_options: None,
                },
                &mut flow(),
                &policy(),
            )
            .unwrap()
            .value
        };
        let wire = serde_json::to_value(output).unwrap();
        assert_eq!(wire["output"][0]["type"], "custom_tool_call");
        assert_eq!(wire["output"][0]["name"], "apply_patch");
        assert_eq!(wire["output"][0]["input"], patch);
        let mut followup = original;
        followup["input"] = json!([
            wire["output"][0],
            {"type":"custom_tool_call_output","call_id":wire["output"][0]["call_id"],"output":"patch applied"}
        ]);
        let replay = request(backend, followup);
        let args = if backend == Dialect::Claude {
            &replay["messages"][0]["content"][0]["input"]
        } else {
            &replay["contents"][0]["parts"][0]["functionCall"]["args"]
        };
        assert_eq!(args["input"], patch);
        assert!(replay.to_string().contains("patch applied"));
    }
}

#[test]
fn both_backends_restore_native_client_tool_calls_and_count_the_same_history() {
    for backend in [Dialect::Claude, Dialect::Gemini] {
        let original = fixtures::request(false);
        let target = request(backend, original.clone());
        let aliases = names(backend, &target);
        assert_eq!(aliases.len(), 4);
        let context_request: r::GenerateContentRequestBody =
            serde_json::from_value(original).unwrap();
        let output = if backend == Dialect::Claude {
            c::claude_to_responses_response(
                serde_json::from_value(fixtures::claude(&aliases)).unwrap(),
                c::ClaudeResponseContext {
                    request: context_request,
                    created_at: 7,
                    effective_parallel_tool_calls: true,
                    effective_tool_choice: r::ToolChoice::Mode(r::ToolChoiceMode::Auto),
                    usage: Default::default(),
                    effective_prompt_cache_options: None,
                },
                &mut flow(),
                &policy(),
            )
            .unwrap()
            .value
        } else {
            g::gemini_to_responses_response(
                serde_json::from_value(fixtures::gemini(&aliases, false)).unwrap(),
                g::GeminiResponseContext {
                    request: context_request,
                    created_at: 7,
                    effective_parallel_tool_calls: true,
                    effective_tool_choice: r::ToolChoice::Mode(r::ToolChoiceMode::Auto),
                    usage: g::GeminiUsageFacts {
                        cache_write_tokens: Some(0),
                        ..Default::default()
                    },
                    effective_prompt_cache_options: None,
                },
                &mut flow(),
                &policy(),
            )
            .unwrap()
            .value
        };
        let wire = serde_json::to_value(output).unwrap();
        assert_eq!(
            wire["output"]
                .as_array()
                .unwrap()
                .iter()
                .map(|i| i["type"].as_str().unwrap())
                .collect::<Vec<_>>(),
            [
                "shell_call",
                "apply_patch_call",
                "function_call",
                "tool_search_call"
            ]
        );
        assert_eq!(wire["output"][2]["namespace"], "repo");
        assert_eq!(wire["output"][2]["name"], "lookup");
        for item in wire["output"].as_array().unwrap() {
            assert_ne!(item["id"], item["call_id"]);
        }
        let followup = fixtures::followup(&wire);
        let generated = request(backend, followup.clone());
        assert_eq!(names(backend, &generated).len(), 5);
        let counted = if backend == Dialect::Claude {
            serde_json::to_value(
                count::openai_to_claude(
                    serde_json::from_value(followup).unwrap(),
                    "selected",
                    Default::default(),
                )
                .unwrap()
                .value,
            )
            .unwrap()
        } else {
            serde_json::to_value(
                count::openai_to_gemini(
                    serde_json::from_value(followup).unwrap(),
                    "selected",
                    Default::default(),
                )
                .unwrap()
                .value
                .generate_content_request
                .unwrap(),
            )
            .unwrap()
        };
        assert_eq!(generated["tools"], counted["tools"]);
        let history = if backend == Dialect::Claude {
            "messages"
        } else {
            "contents"
        };
        assert_eq!(generated[history], counted[history]);
        assert!(counted.get("max_tokens").is_none());
    }
}

#[test]
fn native_tool_selection_and_existing_claude_deferred_fields_keep_their_semantics() {
    let mut input = fixtures::request(false);
    input["tool_choice"] = json!({"type":"allowed_tools","mode":"required","tools":[{"type":"shell"},{"type":"function","name":"lookup","namespace":"repo"}]});
    let claude = request(Dialect::Claude, input.clone());
    assert_eq!(names(Dialect::Claude, &claude).len(), 2);
    assert_eq!(claude["tool_choice"]["type"], "any");
    let gemini = request(Dialect::Gemini, input);
    assert_eq!(gemini["toolConfig"]["functionCallingConfig"]["mode"], "ANY");
    assert_eq!(
        gemini["toolConfig"]["functionCallingConfig"]["allowedFunctionNames"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let original = json!({"model":"m","max_output_tokens":32,"input":"hi","tools":[{"type":"function","name":"lookup","parameters":{"type":"object","properties":{}},"strict":false,"defer_loading":true}]});
    assert_eq!(
        request(Dialect::Claude, original)["tools"][0]["defer_loading"],
        true
    );
}

#[test]
fn server_execution_is_rejected_for_both_new_backends() {
    for backend in [Dialect::Claude, Dialect::Gemini] {
        for tool in [
            json!({"type":"shell","environment":{"type":"container_auto"}}),
            json!({"type":"tool_search","execution":"server"}),
            json!({"type":"apply_patch","allowed_callers":["programmatic"]}),
        ] {
            let input = json!({"model":"m","max_output_tokens":32,"input":"hi","tools":[tool]});
            let converted = if backend == Dialect::Claude {
                c::responses_to_claude_request(
                    serde_json::from_value(input).unwrap(),
                    "selected",
                    Default::default(),
                )
                .is_ok()
            } else {
                g::responses_to_gemini_request(
                    serde_json::from_value(input).unwrap(),
                    "selected",
                    Default::default(),
                )
                .is_ok()
            };
            assert!(converted);
        }
    }
}

#[test]
fn claude_deferred_action_queue_is_bounded_and_cannot_finish_prematurely() {
    use gproxy_protocol::transform::generate::{
        claude_responses::stream::*, stream::claude::synthesize_claude_stream,
    };
    let input = fixtures::request(false);
    let target = request(Dialect::Claude, input.clone());
    let aliases = names(Dialect::Claude, &target);
    let events = synthesize_claude_stream(
        serde_json::from_value(fixtures::claude(&aliases)).unwrap(),
        Default::default(),
    )
    .unwrap()
    .value;
    let context = || c::ClaudeResponseContext {
        request: serde_json::from_value(input.clone()).unwrap(),
        created_at: 7,
        effective_parallel_tool_calls: true,
        effective_tool_choice: r::ToolChoice::Mode(r::ToolChoiceMode::Auto),
        usage: Default::default(),
        effective_prompt_cache_options: None,
    };
    let mut stream = ClaudeToResponsesStream::new(
        context(),
        flow(),
        StreamLimits {
            max_pending: 256,
            ..Default::default()
        },
    )
    .unwrap();
    for event in events.iter().take(2).cloned() {
        stream.push(event).unwrap();
    }
    assert!(stream.push(serde_json::from_value(json!({"type":"content_block_start","index":1,"content_block":{"type":"text","text":""}})).unwrap()).unwrap().value.is_empty());
    assert!(stream.push(serde_json::from_value(json!({"type":"content_block_delta","index":1,"delta":{"type":"text_delta","text":"x".repeat(512)}})).unwrap()).is_err());
    assert!(stream.finish().is_err());
    let mut truncated =
        ClaudeToResponsesStream::new(context(), flow(), Default::default()).unwrap();
    for event in events.into_iter().take(2) {
        truncated.push(event).unwrap();
    }
    assert!(truncated.finish().is_err());
}

#[test]
fn malformed_custom_calls_are_omitted_without_losing_text() {
    for args in [json!({}), json!({"input":null}), json!({"input":42})] {
        for backend in [Dialect::Claude, Dialect::Gemini] {
            let original = json!({
                "model":"client", "input":"edit", "max_output_tokens":128,
                "tools":[{"type":"custom","name":"apply_patch","description":"Edit files with a patch","format":{"type":"text"}}]
            });
            let target = request(backend, original.clone());
            let aliases = names(backend, &target);
            assert_eq!(aliases.len(), 1);
            let context_request = serde_json::from_value(original.clone()).unwrap();
            let output = if backend == Dialect::Claude {
                let mut native = fixtures::claude(&aliases);
                native["content"][0]["input"] = args.clone();
                native["content"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"type":"text","text":"kept"}));
                c::claude_to_responses_response(
                    serde_json::from_value(native).unwrap(),
                    c::ClaudeResponseContext {
                        request: context_request,
                        created_at: 7,
                        effective_parallel_tool_calls: true,
                        effective_tool_choice: r::ToolChoice::Mode(r::ToolChoiceMode::Auto),
                        usage: Default::default(),
                        effective_prompt_cache_options: None,
                    },
                    &mut flow(),
                    &policy(),
                )
                .unwrap()
                .value
            } else {
                let mut native = fixtures::gemini(&aliases, false);
                native["candidates"][0]["content"]["parts"][0]["functionCall"]["args"] =
                    args.clone();
                native["candidates"][0]["content"]["parts"]
                    .as_array_mut()
                    .unwrap()
                    .push(json!({"text":"kept"}));
                g::gemini_to_responses_response(
                    serde_json::from_value(native).unwrap(),
                    g::GeminiResponseContext {
                        request: context_request,
                        created_at: 7,
                        effective_parallel_tool_calls: true,
                        effective_tool_choice: r::ToolChoice::Mode(r::ToolChoiceMode::Auto),
                        usage: g::GeminiUsageFacts {
                            cache_write_tokens: Some(0),
                            ..Default::default()
                        },
                        effective_prompt_cache_options: None,
                    },
                    &mut flow(),
                    &policy(),
                )
                .unwrap()
                .value
            };
            let wire = serde_json::to_value(output).unwrap();
            assert_eq!(wire["output"].as_array().unwrap().len(), 1);
            assert_eq!(wire["output"][0]["type"], "message");
            assert_eq!(wire["output"][0]["content"][0]["text"], "kept");
        }
    }
}
