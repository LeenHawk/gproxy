use gproxy_protocol::{
    transform::generate::stream::claude::*,
    wire::{
        DeclaredFields,
        claude::{generate_content::GenerateContentResponseBody, stream::StreamEvent},
    },
};
use serde_json::{Value, json};
fn event(v: Value) -> StreamEvent {
    serde_json::from_value(v).unwrap()
}
fn collector() -> ClaudeStreamCollector {
    ClaudeStreamCollector::new(ClaudeStreamLimits::default())
}
fn start() -> Value {
    json!({"type":"message_start","message":{"type":"message","id":"msg_native","model":"c","role":"assistant","content":[],"usage":{"input_tokens":1,"output_tokens":0}}})
}
fn terminal() -> Value {
    json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":3}})
}
fn stop() -> Value {
    json!({"type":"message_stop"})
}
fn block(v: Value) -> Value {
    json!({"type":"content_block_start","index":0,"content_block":v})
}
fn delta(v: Value) -> Value {
    json!({"type":"content_block_delta","index":0,"delta":v})
}
fn close() -> Value {
    json!({"type":"content_block_stop","index":0})
}
fn run(values: Vec<Value>) -> GenerateContentResponseBody {
    let mut c = collector();
    for v in values {
        c.push(event(v)).unwrap();
    }
    c.finish().unwrap().value
}
fn body(block: Value) -> GenerateContentResponseBody {
    serde_json::from_value(json!({"type":"message","id":"msg_native","model":"n","role":"assistant","content":[block],"stop_reason":"end_turn","usage":{"input_tokens":7,"output_tokens":9}})).unwrap()
}
fn roundtrip(input: GenerateContentResponseBody) -> GenerateContentResponseBody {
    let events = synthesize_claude_stream(input, ClaudeStreamLimits::default())
        .unwrap()
        .value;
    let mut c = collector();
    for e in events {
        c.push(e).unwrap();
    }
    c.finish().unwrap().value
}
#[test]
fn requires_every_lifecycle_boundary_and_poisons_failures() {
    for values in [
        vec![stop()],
        vec![start(), stop()],
        vec![start(), block(json!({"type":"text","text":""})), terminal()],
        vec![
            start(),
            block(json!({"type":"text","text":""})),
            close(),
            close(),
        ],
        vec![
            start(),
            terminal(),
            block(json!({"type":"text","text":"late"})),
        ],
    ] {
        let mut c = collector();
        let mut failed = false;
        for v in values {
            if c.push(event(v)).is_err() {
                failed = true;
                break;
            }
        }
        assert!(failed);
        assert!(c.push(event(json!({"type":"ping"}))).is_err());
        assert!(c.finish().is_err());
    }
    let mut c = collector();
    c.push(event(start())).unwrap();
    c.push(event(terminal())).unwrap();
    assert!(c.finish().is_err());
    let mut c = collector();
    for v in [start(), terminal(), stop()] {
        c.push(event(v)).unwrap();
    }
    assert!(c.push(event(stop())).is_err());
    assert!(c.finish().is_err());
}
#[test]
fn split_json_supports_native_client_server_and_mcp_tools() {
    for b in [
        json!({"type":"tool_use","id":"toolu_1","name":"same","input":{}}),
        json!({"type":"server_tool_use","id":"srvtoolu_1","name":"web_search","input":{}}),
        json!({"type":"mcp_tool_use","id":"mcp_1","name":"same","server_name":"mcp","input":{}}),
    ] {
        let out = run(vec![
            start(),
            block(b),
            delta(json!({"type":"input_json_delta","partial_json":"{\"nested\":"})),
            delta(
                json!({"type":"input_json_delta","partial_json":"[1,null,{\"rest\":\"formal\"}]}"}),
            ),
            close(),
            terminal(),
            stop(),
        ]);
        assert_eq!(
            serde_json::to_value(out).unwrap()["content"][0]["input"],
            json!({"nested":[1,null,{"rest":"formal"}]})
        );
    }
    let mut c = collector();
    for v in [
        start(),
        block(json!({"type":"tool_use","id":"t","name":"x","input":{}})),
        delta(json!({"type":"input_json_delta","partial_json":"{\"x\":"})),
    ] {
        c.push(event(v)).unwrap();
    }
    assert!(c.push(event(close())).is_err());
    assert!(c.finish().is_err());
}
#[test]
fn citations_and_signature_snapshots_survive() {
    let citation = json!({"type":"char_location","cited_text":"ok","document_index":0,"end_char_index":2,"start_char_index":0});
    let out = run(vec![
        start(),
        block(json!({"type":"text","text":"","citations":null})),
        delta(json!({"type":"text_delta","text":"ok"})),
        delta(json!({"type":"citations_delta","citation":citation})),
        close(),
        terminal(),
        stop(),
    ]);
    assert_eq!(
        serde_json::to_value(&out).unwrap()["content"][0]["citations"],
        json!([citation])
    );
    assert_eq!(roundtrip(out.clone()), out);
    let out = run(vec![
        start(),
        block(json!({"type":"thinking","thinking":"","signature":""})),
        delta(json!({"type":"thinking_delta","thinking":"thought"})),
        delta(json!({"type":"signature_delta","signature":"first"})),
        delta(json!({"type":"signature_delta","signature":"final"})),
        close(),
        terminal(),
        stop(),
    ]);
    assert_eq!(
        serde_json::to_value(&out).unwrap()["content"][0]["signature"],
        "final"
    );
    assert_eq!(roundtrip(out.clone()), out);
}
#[test]
fn compaction_snapshots_and_fallback_model_follow_native_contract() {
    let out = run(vec![
        start(),
        block(json!({"type":"compaction","content":null})),
        delta(json!({"type":"compaction_delta","content":"one","encrypted_content":"old"})),
        delta(json!({"type":"compaction_delta","content":"two","encrypted_content":"new"})),
        close(),
        terminal(),
        stop(),
    ]);
    assert_eq!(
        serde_json::to_value(&out).unwrap()["content"][0]["content"],
        "two"
    );
    assert_eq!(roundtrip(out.clone()), out);
    let out = run(vec![
        start(),
        block(
            json!({"type":"fallback","from":{"model":"c"},"to":{"model":"fallback"},"trigger":{"type":"refusal","category":null}}),
        ),
        close(),
        terminal(),
        stop(),
    ]);
    assert_eq!(out.model, "fallback");
}
#[test]
fn full_usage_and_late_metadata_merge_without_resetting_omitted_fields() {
    let mut begin = start();
    begin["message"]["usage"] = json!({"cache_creation":{"ephemeral_1h_input_tokens":1,"ephemeral_5m_input_tokens":2},"cache_creation_input_tokens":3,"cache_read_input_tokens":4,"fallback_credit":null,"inference_geo":"us","input_tokens":5,"iterations":null,"output_tokens":0,"server_tool_use":{"web_fetch_requests":1,"web_search_requests":2},"service_tier":"priority","speed":"fast"});
    let d = json!({"type":"message_delta","delta":{"stop_reason":"end_turn","container":{"id":"container_1","expires_at":"2030-01-01T00:00:00Z"}},"usage":{"input_tokens":9,"output_tokens":6,"fallback_credit":{"status":{"type":"redeemed"}},"iterations":[{"type":"compaction","cache_creation":null,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"input_tokens":3,"output_tokens":2}],"output_tokens_details":{"thinking_tokens":2}}});
    let out = run(vec![
        begin,
        d,
        json!({"type":"message_delta","delta":{},"usage":{"output_tokens":8}}),
        stop(),
    ]);
    let v = serde_json::to_value(&out).unwrap();
    assert_eq!(v["usage"]["input_tokens"], 9);
    assert_eq!(v["usage"]["cache_read_input_tokens"], 4);
    assert_eq!(v["usage"]["output_tokens"], 8);
    assert_eq!(v["usage"]["fallback_credit"]["status"]["type"], "redeemed");
    assert_eq!(v["container"]["id"], "container_1");
    assert_eq!(roundtrip(out.clone()), out);
}
#[test]
fn limits_cover_starts_deltas_synthesis_and_poison() {
    for limits in [
        ClaudeStreamLimits {
            max_text_bytes: 1,
            ..Default::default()
        },
        ClaudeStreamLimits {
            max_json_bytes: 1,
            ..Default::default()
        },
    ] {
        let mut c = ClaudeStreamCollector::new(limits);
        let a = c.push(event(start()));
        if a.is_ok() {
            assert!(
                c.push(event(block(json!({"type":"text","text":"large"}))))
                    .is_err()
            );
        }
        assert!(c.finish().is_err());
        assert!(
            synthesize_claude_stream(body(json!({"type":"text","text":"large"})), limits).is_err()
        );
    }
}
#[test]
fn nested_rest_is_isolated_but_formal_tool_json_remains() {
    let clean = body(
        json!({"type":"tool_use","id":"t","name":"x","caller":{"type":"direct"},"input":{"rest":{"unknown":42}}}),
    );
    let mut dirty = serde_json::to_value(&clean).unwrap();
    dirty["unknown"] = json!("sentinel");
    dirty["usage"]["unknown"] = json!("sentinel");
    dirty["content"][0]["unknown"] = json!("sentinel");
    dirty["content"][0]["caller"]["unknown"] = json!("sentinel");
    let dirty: GenerateContentResponseBody = serde_json::from_value(dirty).unwrap();
    assert_eq!(roundtrip(dirty), clean.into_declared());
    let mut begin = start();
    begin["unknown"] = json!("sentinel");
    begin["message"]["unknown"] = json!("sentinel");
    begin["message"]["usage"]["unknown"] = json!("sentinel");
    let out = run(vec![begin, terminal(), stop()]);
    assert!(!serde_json::to_string(&out).unwrap().contains("sentinel"));
}
#[test]
fn invalid_usage_tool_ids_and_error_events_never_succeed() {
    let mut begin = start();
    begin["message"]["usage"]["input_tokens"] = json!(-1);
    let mut c = collector();
    assert!(c.push(event(begin)).is_ok());
    assert!(c.finish().is_err());
    let mut c = collector();
    assert!(
        c.push(event(
            json!({"type":"error","error":{"type":"overloaded_error","message":"retry later"}})
        ))
        .is_err()
    );
    assert!(c.finish().is_err());
    let mut c = collector();
    for v in [
        start(),
        block(json!({"type":"tool_use","id":"same","name":"x","input":{}})),
        close(),
    ] {
        c.push(event(v)).unwrap();
    }
    assert!(c.push(event(json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"same","name":"x","input":{}}}))).is_err());
    assert!(c.finish().is_err());
}

#[test]
fn all_seventeen_block_kinds_and_32_variants_roundtrip() {
    let text = json!({"type":"text","text":"answer","citations":null});
    let mut blocks = vec![
        text.clone(),
        json!({"type":"thinking","signature":"sig","thinking":"thought"}),
        json!({"type":"redacted_thinking","data":"cipher"}),
        json!({"type":"tool_use","id":"id","input":{"arbitrary":[1,null]},"name":"x","caller":{"type":"direct"}}),
        json!({"type":"server_tool_use","id":"id","input":{},"name":"web_fetch","caller":{"type":"code_execution_20260120","tool_id":"code"}}),
        json!({"type":"mcp_tool_use","id":"id","input":{},"name":"x","server_name":"mcp"}),
        json!({"type":"mcp_tool_result","tool_use_id":"id","content":[text],"is_error":false}),
        json!({"type":"mcp_tool_result","tool_use_id":"id","content":"done","is_error":false}),
        json!({"type":"container_upload","file_id":"file"}),
        json!({"type":"compaction","content":null,"encrypted_content":"cipher"}),
        json!({"type":"compaction","content":"summary","encrypted_content":null}),
        json!({"type":"fallback","from":{"model":"m"},"to":{"model":"n"},"trigger":{"type":"refusal","category":"cyber"}}),
        json!({"type":"fallback","from":{"model":"m"},"to":{"model":"n"},"trigger":{"type":"refusal","category":null}}),
    ];
    let nested = [
        (
            "web_search_tool_result",
            vec![
                json!([{"type":"web_search_result","encrypted_content":"cipher","page_age":null,"title":"title","url":"url"}]),
                json!({"type":"web_search_tool_result_error","error_code":"unavailable"}),
            ],
        ),
        (
            "web_fetch_tool_result",
            vec![
                json!({"type":"web_fetch_result","retrieved_at":null,"url":"url","content":{"type":"document","citations":{"enabled":true},"title":null,"source":{"type":"text","data":"doc","media_type":"text/plain"}}}),
                json!({"type":"web_fetch_result","retrieved_at":"today","url":"url","content":{"type":"document","citations":null,"title":"pdf","source":{"type":"base64","data":"bytes","media_type":"application/pdf"}}}),
                json!({"type":"web_fetch_tool_result_error","error_code":"url_not_allowed"}),
            ],
        ),
        (
            "advisor_tool_result",
            vec![
                json!({"type":"advisor_result","text":"advice","stop_reason":null}),
                json!({"type":"advisor_redacted_result","encrypted_content":"cipher","stop_reason":"max_tokens"}),
                json!({"type":"advisor_tool_result_error","error_code":"model_not_found"}),
            ],
        ),
        (
            "code_execution_tool_result",
            vec![
                json!({"type":"code_execution_result","content":[{"type":"code_execution_output","file_id":"file"}],"return_code":0,"stderr":"","stdout":"ok"}),
                json!({"type":"encrypted_code_execution_result","content":[],"return_code":0,"stderr":"","encrypted_stdout":"cipher"}),
                json!({"type":"code_execution_tool_result_error","error_code":"unavailable"}),
            ],
        ),
        (
            "bash_code_execution_tool_result",
            vec![
                json!({"type":"bash_code_execution_result","content":[{"type":"bash_code_execution_output","file_id":"file"}],"return_code":0,"stderr":"","stdout":"ok"}),
                json!({"type":"bash_code_execution_tool_result_error","error_code":"output_file_too_large"}),
            ],
        ),
        (
            "text_editor_code_execution_tool_result",
            vec![
                json!({"type":"text_editor_code_execution_view_result","content":"text","file_type":"text","num_lines":null,"start_line":1,"total_lines":1}),
                json!({"type":"text_editor_code_execution_create_result","is_file_update":true}),
                json!({"type":"text_editor_code_execution_str_replace_result","lines":["line"],"new_lines":1,"new_start":1,"old_lines":1,"old_start":1}),
                json!({"type":"text_editor_code_execution_tool_result_error","error_code":"file_not_found","error_message":null}),
            ],
        ),
        (
            "tool_search_tool_result",
            vec![
                json!({"type":"tool_search_tool_search_result","tool_references":[{"type":"tool_reference","tool_name":"x"}]}),
                json!({"type":"tool_search_tool_result_error","error_code":"unavailable","error_message":"why"}),
            ],
        ),
    ];
    for (kind, contents) in nested {
        for content in contents {
            blocks.push(json!({"type":kind,"content":content,"tool_use_id":"id"}));
        }
    }
    assert_eq!(blocks.len(), 32);
    for block in blocks {
        let input = body(block);
        assert_eq!(roundtrip(input.clone()), input);
    }
}

#[test]
fn synthesis_requires_consistent_final_fallback_model_and_preserves_start_origin() {
    let mut input = body(
        json!({"type":"fallback","from":{"model":"original"},"to":{"model":"final"},"trigger":{"type":"refusal","category":null}}),
    );
    input.model = "wrong-final".into();
    assert!(synthesize_claude_stream(input.clone(), Default::default()).is_err());
    input.model = "final".into();
    let events = synthesize_claude_stream(input, Default::default())
        .unwrap()
        .value;
    let StreamEvent::MessageStart(start) = &events[0] else {
        panic!()
    };
    assert_eq!(start.message.model, "original");
    let mut collected = collector();
    for event in events {
        collected.push(event).unwrap();
    }
    assert_eq!(collected.finish().unwrap().value.model, "final");
}

#[test]
fn nested_usage_counts_and_cumulative_output_cannot_be_invalid() {
    for usage in [
        json!({"input_tokens":1,"output_tokens":0,"cache_creation":{"ephemeral_1h_input_tokens":-1,"ephemeral_5m_input_tokens":2}}),
        json!({"input_tokens":1,"output_tokens":0,"cache_creation":{"ephemeral_1h_input_tokens":i64::MAX,"ephemeral_5m_input_tokens":1}}),
        json!({"input_tokens":1,"output_tokens":0,"iterations":[{"type":"message","model":"m","input_tokens":-1,"output_tokens":0,"cache_creation_input_tokens":0,"cache_read_input_tokens":0}]}),
    ] {
        let mut value = start();
        value["message"]["usage"] = usage;
        let mut collected = collector();
        assert!(collected.push(event(value)).is_ok());
        assert!(collected.finish().is_err());
    }
    let mut collected = collector();
    collected.push(event(start())).unwrap();
    collected.push(event(terminal())).unwrap();
    let mut decreased = terminal();
    decreased["usage"]["output_tokens"] = json!(2);
    assert!(collected.push(event(decreased)).is_ok());
    let mut collected = collector();
    collected.push(event(start())).unwrap();
    collected.push(event(terminal())).unwrap();
    let mut cleared = terminal();
    cleared["delta"]["stop_reason"] = Value::Null;
    assert!(collected.push(event(cleared)).is_err());
}
