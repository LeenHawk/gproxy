use gproxy_protocol::openai::responses::response::*;
use gproxy_protocol::openai::responses::{generate, input};
use http::{HeaderMap, HeaderValue, StatusCode};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

fn minimal() -> Value {
    json!({"id":"r","created_at":123,"error":null,"incomplete_details":null,"instructions":null,"metadata":null,"model":"future-model","object":"response","output":[],"parallel_tool_calls":true,"temperature":null,"tool_choice":"auto","tools":[],"top_p":null})
}

// Source output branches: Create a model response.md:10119-13237.
fn output_fixtures() -> Vec<Value> {
    vec![
        json!({"type": "file_search_call", "id": "f", "queries": ["x"], "status": "searching", "results": [{"attributes": {"tag": "a", "rank": 1.5, "active": true}, "file_id": "f", "filename": "a.txt", "score": 0.5, "text": "x"}]}),
        json!({"type": "computer_call", "id": "c", "call_id": "c", "status": "in_progress", "pending_safety_checks": [{"id": "s", "code": null, "message": null}], "action": {"type": "wait"}, "actions": [{"type": "screenshot"}]}),
        json!({"type": "computer_call_output", "call_id": "c", "id": "c", "output": {"type": "computer_screenshot", "file_id": "f", "image_url": "https://x"}, "acknowledged_safety_checks": [{"id": "s", "code": null, "message": null}], "status": "failed", "created_by": "actor"}),
        json!({"type": "web_search_call", "id": "w", "status": "failed", "action": {"type": "search", "query": "x", "queries": ["x"], "sources": [{"type": "url", "url": "https://x"}]}}),
        json!({"type": "function_call", "id": "f", "call_id": "c", "name": "f", "namespace": "ns", "arguments": "{}", "caller": {"type": "program", "caller_id": "p"}, "status": "incomplete"}),
        json!({"type": "function_call_output", "id": "f", "call_id": "c", "output": [{"type": "input_image", "detail": "auto", "file_id": null}], "name": "f", "namespace": "n", "caller": null, "status": "completed", "created_by": "actor"}),
        json!({"type": "tool_search_call", "arguments": {"arbitrary": [true, 1]}, "id": "t", "call_id": null, "execution": "client", "status": "completed", "created_by": "actor"}),
        json!({"type": "tool_search_output", "tools": [], "id": "t", "call_id": null, "execution": "server", "status": "completed", "created_by": "actor"}),
        json!({"type": "additional_tools", "role": "critic", "tools": [], "id": "a"}),
        json!({"type": "reasoning", "id": "r", "summary": [{"type": "summary_text", "text": "summary"}], "content": [{"type": "reasoning_text", "text": "reason"}], "encrypted_content": null, "status": "completed"}),
        json!({"type": "compaction", "encrypted_content": "data", "id": "c", "created_by": "actor"}),
        json!({"type": "image_generation_call", "id": "i", "result": null, "status": "generating"}),
        json!({"type": "code_interpreter_call", "id": "i", "container_id": "c", "code": null, "outputs": null, "status": "interpreting"}),
        json!({"type": "local_shell_call", "id": "l", "call_id": "c", "status": "completed", "action": {"type": "exec", "command": ["pwd"], "env": {"LANG": "C"}, "timeout_ms": null, "user": null, "working_directory": null}}),
        json!({"type": "local_shell_call_output", "id": "l", "output": "x", "status": null}),
        json!({"type": "shell_call", "call_id": "c", "action": {"commands": ["pwd"], "max_output_length": null, "timeout_ms": null}, "id": "s", "caller": null, "environment": {"type": "local"}, "status": "completed", "created_by": "actor"}),
        json!({"type": "shell_call_output", "call_id": "c", "output": [{"stdout": "", "stderr": "", "outcome": {"type": "exit", "exit_code": 0}, "created_by": "actor"}], "id": "s", "caller": null, "max_output_length": null, "status": "completed", "created_by": "actor"}),
        json!({"type": "apply_patch_call", "call_id": "c", "operation": {"type": "delete_file", "path": "a"}, "status": "in_progress", "id": "p", "caller": null, "created_by": "actor"}),
        json!({"type": "apply_patch_call_output", "call_id": "c", "status": "failed", "output": null, "id": "p", "caller": null, "created_by": "actor"}),
        json!({"type": "mcp_list_tools", "id": "m", "server_label": "s", "tools": [{"name": "f", "input_schema": {"type": "object"}, "annotations": null, "description": null}], "error": null}),
        json!({"type": "mcp_approval_request", "id": "m", "arguments": "{}", "name": "f", "server_label": "s"}),
        json!({"type": "mcp_approval_response", "approval_request_id": "m", "approve": false, "id": "m", "reason": null}),
        json!({"type": "mcp_call", "id": "m", "arguments": "{}", "name": "f", "server_label": "s", "approval_request_id": null, "error": null, "output": null, "status": "calling"}),
        json!({"type": "custom_tool_call_output", "call_id": "c", "id": "c", "caller": null, "output": [{"type": "input_image", "detail": "original", "file_id": null, "image_url": null}], "status": "completed", "created_by": "actor"}),
        json!({"type": "custom_tool_call", "call_id": "c", "input": "raw", "name": "f", "id": "i", "caller": null, "namespace": "n"}),
        json!({"type": "program", "id": "p", "call_id": "c", "code": "1", "fingerprint": "fp"}),
        json!({"type": "program_output", "id": "p", "call_id": "c", "result": "1", "status": "incomplete"}),
        json!({"type": "message", "id": "m", "role": "assistant", "status": "completed", "phase": "final_answer", "content": [{"type": "output_text", "text": "answer", "annotations": [], "logprobs": []}]}),
    ]
}

#[test]
fn all_output_variants_preserve_known_fields_and_extensions() {
    let mut fixtures = output_fixtures();
    fixtures.push(json!({"type":"configuration_update","id":"cfg1","reasoning":{"effort":"high"}}));
    assert_eq!(fixtures.len(), 29);
    for wire in fixtures {
        let value: ResponseOutputItem = serde_json::from_value(wire.clone()).unwrap();
        match &value {
            ResponseOutputItem::ConfigurationUpdate(x) => assert!(x.rest.is_empty()),
            ResponseOutputItem::FileSearchCall(x) => {
                assert!(x.rest.is_empty(), "FileSearchCall: {:?}", x.rest)
            }
            ResponseOutputItem::ComputerCall(x) => {
                assert!(x.rest.is_empty(), "ComputerCall: {:?}", x.rest)
            }
            ResponseOutputItem::ComputerCallOutput(x) => {
                assert!(x.rest.is_empty(), "ComputerCallOutput: {:?}", x.rest)
            }
            ResponseOutputItem::WebSearchCall(x) => {
                assert!(x.rest.is_empty(), "WebSearchCall: {:?}", x.rest)
            }
            ResponseOutputItem::FunctionCall(x) => {
                assert!(x.rest.is_empty(), "FunctionCall: {:?}", x.rest)
            }
            ResponseOutputItem::FunctionCallOutput(x) => {
                assert!(x.rest.is_empty(), "FunctionCallOutput: {:?}", x.rest)
            }
            ResponseOutputItem::ToolSearchCall(x) => {
                assert!(x.rest.is_empty(), "ToolSearchCall: {:?}", x.rest)
            }
            ResponseOutputItem::ToolSearchOutput(x) => {
                assert!(x.rest.is_empty(), "ToolSearchOutput: {:?}", x.rest)
            }
            ResponseOutputItem::AdditionalTools(x) => {
                assert!(x.rest.is_empty(), "AdditionalTools: {:?}", x.rest)
            }
            ResponseOutputItem::Reasoning(x) => {
                assert!(x.rest.is_empty(), "Reasoning: {:?}", x.rest)
            }
            ResponseOutputItem::Compaction(x) => {
                assert!(x.rest.is_empty(), "Compaction: {:?}", x.rest)
            }
            ResponseOutputItem::ImageGenerationCall(x) => {
                assert!(x.rest.is_empty(), "ImageGenerationCall: {:?}", x.rest)
            }
            ResponseOutputItem::CodeInterpreterCall(x) => {
                assert!(x.rest.is_empty(), "CodeInterpreterCall: {:?}", x.rest)
            }
            ResponseOutputItem::LocalShellCall(x) => {
                assert!(x.rest.is_empty(), "LocalShellCall: {:?}", x.rest)
            }
            ResponseOutputItem::LocalShellCallOutput(x) => {
                assert!(x.rest.is_empty(), "LocalShellCallOutput: {:?}", x.rest)
            }
            ResponseOutputItem::ShellCall(x) => {
                assert!(x.rest.is_empty(), "ShellCall: {:?}", x.rest)
            }
            ResponseOutputItem::ShellCallOutput(x) => {
                assert!(x.rest.is_empty(), "ShellCallOutput: {:?}", x.rest)
            }
            ResponseOutputItem::ApplyPatchCall(x) => {
                assert!(x.rest.is_empty(), "ApplyPatchCall: {:?}", x.rest)
            }
            ResponseOutputItem::ApplyPatchCallOutput(x) => {
                assert!(x.rest.is_empty(), "ApplyPatchCallOutput: {:?}", x.rest)
            }
            ResponseOutputItem::McpListTools(x) => {
                assert!(x.rest.is_empty(), "McpListTools: {:?}", x.rest)
            }
            ResponseOutputItem::McpApprovalRequest(x) => {
                assert!(x.rest.is_empty(), "McpApprovalRequest: {:?}", x.rest)
            }
            ResponseOutputItem::McpApprovalResponse(x) => {
                assert!(x.rest.is_empty(), "McpApprovalResponse: {:?}", x.rest)
            }
            ResponseOutputItem::McpCall(x) => assert!(x.rest.is_empty(), "McpCall: {:?}", x.rest),
            ResponseOutputItem::CustomToolCallOutput(x) => {
                assert!(x.rest.is_empty(), "CustomToolCallOutput: {:?}", x.rest)
            }
            ResponseOutputItem::CustomToolCall(x) => {
                assert!(x.rest.is_empty(), "CustomToolCall: {:?}", x.rest)
            }
            ResponseOutputItem::Program(x) => assert!(x.rest.is_empty(), "Program: {:?}", x.rest),
            ResponseOutputItem::ProgramOutput(x) => {
                assert!(x.rest.is_empty(), "ProgramOutput: {:?}", x.rest)
            }
            ResponseOutputItem::Message(x) => assert!(x.rest.is_empty(), "Message: {:?}", x.rest),

            #[cfg(not(feature = "exhaustive"))]
            _ => panic!(),
        }
        assert_eq!(serde_json::to_value(&value).unwrap(), wire);
        let mut extended = wire;
        extended["future"] = json!({"x":1});
        let value: ResponseOutputItem = serde_json::from_value(extended.clone()).unwrap();
        assert_eq!(serde_json::to_value(value).unwrap(), extended);
    }
    for invalid in [
        json!({"role":"user","content":"x"}),
        json!({"id":"ref"}),
        json!({"type":"item_reference","id":"ref"}),
        json!({"type":"compaction_trigger"}),
    ] {
        assert!(serde_json::from_value::<ResponseOutputItem>(invalid).is_err());
    }
}

fn full() -> Value {
    let mut v = minimal();
    let extra = json!({
 "background":true,"completed_at":124,"conversation":{"id":"conv"},"max_output_tokens":1000,"max_tool_calls":2,
 "moderation":{"input":{"type":"moderation_result","categories":{"custom":false},"category_applied_input_types":{"custom":["text","image"]},"category_scores":{"custom":0.2},"flagged":false,"model":"m"},"output":{"type":"error","code":"arbitrary","message":"error"}},
 "output_text":"answer","previous_response_id":"previous","prompt":{"id":"p","variables":{"x":{"type":"input_image","detail":"auto","file_id":null}},"version":null},
 "prompt_cache_key":"key","prompt_cache_options":{"mode":"implicit","ttl":"30m"},"prompt_cache_retention":"24h",
 "reasoning":{"effort":"max","summary":null},"safety_identifier":"safe","service_tier":"priority","status":"completed",
 "text":{"format":{"type":"json_schema","name":"s","schema":{"type":"object"},"strict":null},"verbosity":"low"},
 "top_logprobs":2,"truncation":"disabled","usage":{"input_tokens":5,"input_tokens_details":{"cache_write_tokens":1,"cached_tokens":2},"output_tokens":3,"output_tokens_details":{"reasoning_tokens":1},"total_tokens":8},"user":"u"});
    v.as_object_mut()
        .unwrap()
        .extend(extra.as_object().unwrap().clone());
    v["output"] = json!(output_fixtures());
    v["error"] = json!({"code":"image_file_not_found","message":"missing"});
    v["incomplete_details"] = json!({"reason":"content_filter"});
    v["instructions"] = json!([{"role":"developer","content":"x"}]);
    v["metadata"] = json!({"purpose":"test"});
    v["temperature"] = json!(0.5);
    v["top_p"] = json!(0.9);
    v
}
#[test]
fn root_35_fields_and_http_three_elements_are_complete() {
    let wire = full();
    assert_eq!(wire.as_object().unwrap().len(), 35);
    let body: GenerateContentResponseBody = serde_json::from_value(wire.clone()).unwrap();
    assert!(body.rest.is_empty());
    assert!(body.error.as_ref().unwrap().rest.is_empty());
    assert!(body.incomplete_details.as_ref().unwrap().rest.is_empty());
    let moderation = body.moderation.as_ref().unwrap().as_ref().unwrap();
    assert!(moderation.rest.is_empty());
    match &moderation.input {
        ResponseModerationOutcome::Result(v) => {
            assert!(v.rest.is_empty());
            assert_eq!(v.category_scores["custom"].as_f64(), Some(0.2));
        }
        _ => panic!(),
    }
    match &moderation.output {
        ResponseModerationOutcome::Error(v) => assert!(v.rest.is_empty()),
        _ => panic!(),
    }
    let usage = body.usage.as_ref().unwrap().as_ref().unwrap();
    assert!(usage.rest.is_empty());
    assert!(usage.input_tokens_details.rest.is_empty());
    assert!(usage.output_tokens_details.rest.is_empty());
    assert!(body.prompt_cache_options.as_ref().unwrap().rest.is_empty());
    assert!(
        body.prompt
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .rest
            .is_empty()
    );
    let mut headers = HeaderMap::new();
    headers.insert("x-request-id", HeaderValue::from_static("r"));
    let response: GenerateContentResponse = GenerateContentResponse {
        status: StatusCode::OK,
        headers,
        body,
    };
    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(response.headers["x-request-id"], "r");
    assert_eq!(serde_json::to_value(response.body).unwrap(), wire);
}
#[test]
fn root_required_nullable_and_optional_contracts_match_source() {
    let wire = full();
    let required = [
        "id",
        "created_at",
        "error",
        "incomplete_details",
        "instructions",
        "metadata",
        "model",
        "object",
        "output",
        "parallel_tool_calls",
        "temperature",
        "tool_choice",
        "tools",
        "top_p",
    ];
    let required_nullable = [
        "error",
        "incomplete_details",
        "instructions",
        "metadata",
        "temperature",
        "top_p",
    ];
    let nullable = [
        "background",
        "completed_at",
        "conversation",
        "max_output_tokens",
        "max_tool_calls",
        "moderation",
        "output_text",
        "previous_response_id",
        "prompt",
        "prompt_cache_key",
        "prompt_cache_retention",
        "reasoning",
        "safety_identifier",
        "service_tier",
        "top_logprobs",
        "truncation",
        // Native created/in_progress examples explicitly carry these as null.
        "usage",
        "user",
    ];
    let optional = ["prompt_cache_options", "status", "text"];
    for f in required {
        let mut x = wire.clone();
        x.as_object_mut().unwrap().remove(f);
        assert!(
            serde_json::from_value::<GenerateContentResponseBody>(x).is_err(),
            "missing {f}"
        );
    }
    for f in required_nullable.into_iter().chain(nullable) {
        let mut x = wire.clone();
        x[f] = Value::Null;
        let p: GenerateContentResponseBody = serde_json::from_value(x.clone()).unwrap();
        assert_eq!(serde_json::to_value(p).unwrap(), x, "null {f}");
    }
    for f in nullable.into_iter().chain(optional) {
        let mut x = wire.clone();
        x.as_object_mut().unwrap().remove(f);
        let p: GenerateContentResponseBody = serde_json::from_value(x.clone()).unwrap();
        assert_eq!(serde_json::to_value(p).unwrap(), x, "absent {f}");
    }
    for f in optional {
        let mut x = wire.clone();
        x[f] = Value::Null;
        assert!(
            serde_json::from_value::<GenerateContentResponseBody>(x).is_err(),
            "null {f}"
        );
    }
    for instructions in [
        json!("text"),
        json!([{"type":"item_reference","id":"i"}]),
        Value::Null,
    ] {
        let mut x = minimal();
        x["instructions"] = instructions;
        let p: GenerateContentResponseBody = serde_json::from_value(x.clone()).unwrap();
        assert_eq!(serde_json::to_value(p).unwrap(), x);
    }
    let mut x = minimal();
    x["metadata"] = json!({"k":3});
    assert!(serde_json::from_value::<GenerateContentResponseBody>(x).is_err());
}
#[test]
fn response_specific_required_fields_cannot_fall_back_to_input_shapes() {
    let checks = [
        ("function_call_output", vec!["id", "status"]),
        ("computer_call_output", vec!["id", "status"]),
        (
            "tool_search_call",
            vec!["id", "call_id", "execution", "status"],
        ),
        (
            "tool_search_output",
            vec!["id", "call_id", "execution", "status"],
        ),
        ("additional_tools", vec!["id", "role"]),
        ("compaction", vec!["id"]),
        ("shell_call", vec!["id", "environment", "status"]),
        (
            "shell_call_output",
            vec!["id", "max_output_length", "status"],
        ),
        ("apply_patch_call", vec!["id", "status"]),
        ("apply_patch_call_output", vec!["id", "status"]),
        ("mcp_approval_response", vec!["id"]),
        ("custom_tool_call_output", vec!["id", "status"]),
    ];
    for (kind, fields) in checks {
        let fixture = output_fixtures()
            .into_iter()
            .find(|v| v["type"] == kind)
            .unwrap();
        for field in fields {
            let mut wire = fixture.clone();
            wire.as_object_mut().unwrap().remove(field);
            assert!(
                serde_json::from_value::<ResponseOutputItem>(wire).is_err(),
                "{kind} missing {field}"
            );
        }
    }
    let mut wire = output_fixtures()
        .into_iter()
        .find(|v| v["type"] == "shell_call")
        .unwrap();
    for field in ["max_output_length", "timeout_ms"] {
        let mut x = wire.clone();
        x["action"].as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<ResponseOutputItem>(x).is_err());
    }
    wire["environment"] = Value::Null;
    let item: ResponseOutputItem = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(item).unwrap(), wire);
    let mut function = output_fixtures()
        .into_iter()
        .find(|v| v["type"] == "function_call_output")
        .unwrap();
    function["output"] = json!([{"type":"input_image"}]);
    assert!(serde_json::from_value::<ResponseOutputItem>(function).is_err());
    assert!(
        serde_json::from_value::<ResponsePromptCacheOptions>(json!({"mode":"implicit"})).is_err()
    );
    assert!(serde_json::from_value::<ResponsePromptCacheOptions>(json!({"ttl":"30m"})).is_err());
}
#[test]
fn nested_response_only_fields_are_typed_and_preserve_nullable_semantics() {
    for wire in output_fixtures() {
        let item: ResponseOutputItem = serde_json::from_value(wire).unwrap();
        match item {
            ResponseOutputItem::ShellCall(v) => {
                assert!(v.action.rest.is_empty());
                assert_eq!(v.action.max_output_length, None);
                match v.environment.unwrap() {
                    ResponseShellEnvironment::Local(v) => assert!(v.rest.is_empty()),
                    _ => panic!(),
                }
            }
            ResponseOutputItem::ShellCallOutput(v) => {
                assert!(v.output[0].rest.is_empty());
                assert_eq!(v.output[0].created_by.as_deref(), Some("actor"));
            }
            ResponseOutputItem::ComputerCallOutput(v) => {
                assert!(v.output.rest.is_empty());
                assert!(v.acknowledged_safety_checks.unwrap()[0].rest.is_empty());
            }
            _ => {}
        }
    }
    let wire = json!({"type":"container_reference","container_id":"c"});
    let p: ResponseShellEnvironment = serde_json::from_value(wire.clone()).unwrap();
    assert!(matches!(p, ResponseShellEnvironment::Reference(_)));
    assert_eq!(serde_json::to_value(p).unwrap(), wire);
    let wire = json!({"type":"local","skills":[{"extension":true}]});
    let p: ResponseShellEnvironment = serde_json::from_value(wire.clone()).unwrap();
    match &p {
        ResponseShellEnvironment::Local(v) => assert!(v.rest.contains_key("skills")),
        _ => panic!(),
    };
    assert_eq!(serde_json::to_value(p).unwrap(), wire);
    for k in ["tool_search_call", "tool_search_output"] {
        let wire = output_fixtures()
            .into_iter()
            .find(|v| v["type"] == k)
            .unwrap();
        assert!(wire["call_id"].is_null());
        let p: ResponseOutputItem = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(p).unwrap(), wire);
    }
    let wire = json!({"id":"c","call_id":"c","output":{"type":"computer_screenshot"},"status":"failed","type":"computer_call_output","acknowledged_safety_checks":null});
    assert!(serde_json::from_value::<ResponseComputerCallOutput>(wire).is_err());
}
fn closed<T: DeserializeOwned + Serialize>(values: &[&str]) {
    for v in values {
        let p: T = serde_json::from_value(json!(v)).unwrap();
        assert_eq!(serde_json::to_value(p).unwrap(), json!(v));
    }
    assert!(serde_json::from_value::<T>(json!("unknown-value")).is_err());
}
#[test]
fn response_enums_use_exact_native_values_and_never_lossy_other() {
    closed::<ResponseErrorCode>(&[
        "server_error",
        "rate_limit_exceeded",
        "invalid_prompt",
        "data_residency_mismatch",
        "bio_policy",
        "vector_store_timeout",
        "invalid_image",
        "invalid_image_format",
        "invalid_base64_image",
        "invalid_image_url",
        "image_too_large",
        "image_too_small",
        "image_parse_error",
        "image_content_policy_violation",
        "invalid_image_mode",
        "image_file_too_large",
        "unsupported_image_media_type",
        "empty_image_file",
        "failed_to_download_image",
        "image_file_not_found",
    ]);
    closed::<ResponseStatus>(&[
        "completed",
        "failed",
        "in_progress",
        "cancelled",
        "queued",
        "incomplete",
    ]);
    closed::<ResponseAdditionalToolsRole>(&[
        "unknown",
        "user",
        "assistant",
        "system",
        "critic",
        "discriminator",
        "developer",
        "tool",
    ]);
    closed::<ResponseComputerOutputStatus>(&["completed", "incomplete", "failed", "in_progress"]);
    closed::<ResponseIncompleteReason>(&["max_output_tokens", "content_filter"]);
    closed::<ResponseModerationInputType>(&["text", "image"]);
    assert!(serde_json::from_value::<ResponseStatus>(json!("InProgress")).is_err());
    let error = json!({"type":"error","code":"future-code","message":"x","categories":{},"category_scores":{},"category_applied_input_types":{},"flagged":false,"model":"m"});
    let p: ResponseModerationOutcome = serde_json::from_value(error.clone()).unwrap();
    assert!(matches!(p, ResponseModerationOutcome::Error(_)));
    assert_eq!(serde_json::to_value(p).unwrap(), error);
}
#[test]
fn response_builders_encode_required_nullable_and_native_tags() {
    let body = GenerateContentResponseBody::builder(
        "r".into(),
        123,
        None,
        None,
        None,
        None,
        "model".into(),
        ResponseObject::Response,
        vec![],
        true,
        None,
        input::ToolChoice::Mode(input::ToolChoiceMode::Auto),
        vec![],
        None,
    )
    .build();
    assert_eq!(serde_json::to_value(body).unwrap(), {
        let mut v = minimal();
        v["model"] = json!("model");
        v
    });
    let item = ResponseToolSearchCall::builder(
        "t".into(),
        json!({"query":"x"}),
        None,
        gproxy_protocol::openai::responses::tools::ToolExecution::Server,
        input::ItemStatus::Completed,
        input::ToolSearchCallType::ToolSearchCall,
    )
    .created_by("actor")
    .build();
    let v = serde_json::to_value(item).unwrap();
    assert!(v.get("call_id").unwrap().is_null());
    assert_eq!(v["type"], "tool_search_call");
    assert_eq!(v["created_by"], "actor");
    let p = ResponsePromptCacheOptions::builder(
        generate::PromptCachingMode::Implicit,
        generate::PromptCacheTtl::ThirtyMinutes,
    )
    .build();
    assert_eq!(
        serde_json::to_value(p).unwrap(),
        json!({"mode":"implicit","ttl":"30m"})
    );
    let x =
        ResponseCompaction::builder("c".into(), "data".into(), input::CompactionType::Compaction)
            .created_by("actor")
            .build();
    assert_eq!(serde_json::to_value(x).unwrap()["created_by"], "actor");
}
#[test]
fn response_unknown_fields_and_snake_case_names_round_trip() {
    let mut wire = minimal();
    wire["completedAt"] = json!(100);
    wire["future"] = json!({"x":true});
    let p: GenerateContentResponseBody = serde_json::from_value(wire.clone()).unwrap();
    assert!(p.completed_at.is_none());
    assert_eq!(p.rest.len(), 2);
    assert_eq!(serde_json::to_value(p).unwrap(), wire);
}

// Each tuple is transcribed from the output item's own field declarations,
// not inferred from the implementation or the similar input-item contracts.
#[test]
fn every_output_field_has_its_documented_required_optional_and_null_contract() {
    type Contract = (
        &'static str,
        &'static [&'static str],
        &'static [&'static str],
        &'static [&'static str],
        &'static [&'static str],
    );
    let contracts: &[Contract] = &[
        (
            "message",
            &["id", "content", "role", "status", "type", "phase"],
            &["id", "content", "role", "status", "type"],
            &["phase"],
            &["phase"],
        ),
        (
            "file_search_call",
            &["id", "queries", "status", "type", "results"],
            &["id", "queries", "status", "type"],
            &["results"],
            &["results"],
        ),
        (
            "function_call",
            &[
                "arguments",
                "call_id",
                "name",
                "type",
                "id",
                "caller",
                "namespace",
                "status",
            ],
            &["arguments", "call_id", "name", "type"],
            &["id", "caller", "namespace", "status"],
            &["caller"],
        ),
        (
            "function_call_output",
            &[
                "id",
                "call_id",
                "output",
                "status",
                "type",
                "caller",
                "created_by",
                "name",
                "namespace",
            ],
            &["id", "call_id", "output", "status", "type"],
            &["caller", "created_by", "name", "namespace"],
            &["caller"],
        ),
        (
            "web_search_call",
            &["id", "action", "status", "type"],
            &["id", "action", "status", "type"],
            &[],
            &[],
        ),
        (
            "computer_call",
            &[
                "id",
                "call_id",
                "pending_safety_checks",
                "status",
                "type",
                "action",
                "actions",
            ],
            &["id", "call_id", "pending_safety_checks", "status", "type"],
            &["action", "actions"],
            &[],
        ),
        (
            "computer_call_output",
            &[
                "id",
                "call_id",
                "output",
                "status",
                "type",
                "acknowledged_safety_checks",
                "created_by",
            ],
            &["id", "call_id", "output", "status", "type"],
            &["acknowledged_safety_checks", "created_by"],
            &[],
        ),
        (
            "reasoning",
            &[
                "id",
                "summary",
                "type",
                "content",
                "encrypted_content",
                "status",
            ],
            &["id", "summary", "type"],
            &["content", "encrypted_content", "status"],
            &["encrypted_content"],
        ),
        (
            "program",
            &["id", "call_id", "code", "fingerprint", "type"],
            &["id", "call_id", "code", "fingerprint", "type"],
            &[],
            &[],
        ),
        (
            "program_output",
            &["id", "call_id", "result", "status", "type"],
            &["id", "call_id", "result", "status", "type"],
            &[],
            &[],
        ),
        (
            "tool_search_call",
            &[
                "id",
                "arguments",
                "call_id",
                "execution",
                "status",
                "type",
                "created_by",
            ],
            &["id", "arguments", "call_id", "execution", "status", "type"],
            &["created_by"],
            &["arguments", "call_id"],
        ),
        (
            "tool_search_output",
            &[
                "id",
                "call_id",
                "execution",
                "status",
                "tools",
                "type",
                "created_by",
            ],
            &["id", "call_id", "execution", "status", "tools", "type"],
            &["created_by"],
            &["call_id"],
        ),
        (
            "additional_tools",
            &["id", "role", "tools", "type"],
            &["id", "role", "tools", "type"],
            &[],
            &[],
        ),
        (
            "compaction",
            &["id", "encrypted_content", "type", "created_by"],
            &["id", "encrypted_content", "type"],
            &["created_by"],
            &[],
        ),
        (
            "image_generation_call",
            &["id", "result", "status", "type"],
            &["id", "result", "status", "type"],
            &[],
            &["result"],
        ),
        (
            "code_interpreter_call",
            &["id", "code", "container_id", "outputs", "status", "type"],
            &["id", "code", "container_id", "outputs", "status", "type"],
            &[],
            &["code", "outputs"],
        ),
        (
            "local_shell_call",
            &["id", "action", "call_id", "status", "type"],
            &["id", "action", "call_id", "status", "type"],
            &[],
            &[],
        ),
        (
            "local_shell_call_output",
            &["id", "output", "type", "status"],
            &["id", "output", "type"],
            &["status"],
            &["status"],
        ),
        (
            "shell_call",
            &[
                "id",
                "action",
                "call_id",
                "environment",
                "status",
                "type",
                "caller",
                "created_by",
            ],
            &["id", "action", "call_id", "environment", "status", "type"],
            &["caller", "created_by"],
            &["environment", "caller"],
        ),
        (
            "shell_call_output",
            &[
                "id",
                "call_id",
                "max_output_length",
                "output",
                "status",
                "type",
                "caller",
                "created_by",
            ],
            &[
                "id",
                "call_id",
                "max_output_length",
                "output",
                "status",
                "type",
            ],
            &["caller", "created_by"],
            &["max_output_length", "caller"],
        ),
        (
            "apply_patch_call",
            &[
                "id",
                "call_id",
                "operation",
                "status",
                "type",
                "caller",
                "created_by",
            ],
            &["id", "call_id", "operation", "status", "type"],
            &["caller", "created_by"],
            &["caller"],
        ),
        (
            "apply_patch_call_output",
            &[
                "id",
                "call_id",
                "status",
                "type",
                "caller",
                "created_by",
                "output",
            ],
            &["id", "call_id", "status", "type"],
            &["caller", "created_by", "output"],
            &["caller", "output"],
        ),
        (
            "mcp_call",
            &[
                "id",
                "arguments",
                "name",
                "server_label",
                "type",
                "approval_request_id",
                "error",
                "output",
                "status",
            ],
            &["id", "arguments", "name", "server_label", "type"],
            &["approval_request_id", "error", "output", "status"],
            &["approval_request_id", "error", "output"],
        ),
        (
            "mcp_list_tools",
            &["id", "server_label", "tools", "type", "error"],
            &["id", "server_label", "tools", "type"],
            &["error"],
            &["error"],
        ),
        (
            "mcp_approval_request",
            &["id", "arguments", "name", "server_label", "type"],
            &["id", "arguments", "name", "server_label", "type"],
            &[],
            &[],
        ),
        (
            "mcp_approval_response",
            &["id", "approval_request_id", "approve", "type", "reason"],
            &["id", "approval_request_id", "approve", "type"],
            &["reason"],
            &["reason"],
        ),
        (
            "custom_tool_call",
            &[
                "call_id",
                "input",
                "name",
                "type",
                "id",
                "caller",
                "namespace",
            ],
            &["call_id", "input", "name", "type"],
            &["id", "caller", "namespace"],
            &["caller"],
        ),
        (
            "custom_tool_call_output",
            &[
                "id",
                "call_id",
                "output",
                "status",
                "type",
                "caller",
                "created_by",
            ],
            &["id", "call_id", "output", "status", "type"],
            &["caller", "created_by"],
            &["caller"],
        ),
    ];
    for (tag, fields, required, optional, nullable) in contracts {
        let base = output_fixtures()
            .into_iter()
            .find(|v| v["type"] == *tag)
            .unwrap();
        for field in *required {
            let mut v = base.clone();
            v.as_object_mut().unwrap().remove(*field);
            assert!(
                serde_json::from_value::<ResponseOutputItem>(v).is_err(),
                "{tag} missing {field}"
            );
        }
        for field in *optional {
            let mut v = base.clone();
            v.as_object_mut().unwrap().remove(*field);
            let item: ResponseOutputItem = serde_json::from_value(v.clone()).unwrap();
            assert_eq!(serde_json::to_value(item).unwrap(), v, "{tag} omit {field}");
        }
        for field in *fields {
            let mut v = base.clone();
            v[*field] = Value::Null;
            let item = serde_json::from_value::<ResponseOutputItem>(v.clone());
            if nullable.contains(field) {
                assert_eq!(
                    serde_json::to_value(item.unwrap()).unwrap(),
                    v,
                    "{tag} null {field}"
                );
            } else {
                assert!(item.is_err(), "{tag}.{field} is not nullable");
            }
        }
    }
}
