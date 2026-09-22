use gproxy_protocol::claude::generate_content as response;
use gproxy_protocol::claude::stream::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
fn round<T: Serialize + DeserializeOwned>(wire: Value) -> T {
    let p: T = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(&p).unwrap(), wire);
    p
}
fn event_rest(v: &StreamEvent) -> &gproxy_protocol::Rest {
    match v {
        StreamEvent::MessageStart(v) => &v.rest,
        StreamEvent::ContentBlockStart(v) => &v.rest,
        StreamEvent::ContentBlockDelta(v) => &v.rest,
        StreamEvent::ContentBlockStop(v) => &v.rest,
        StreamEvent::MessageDelta(v) => &v.rest,
        StreamEvent::MessageStop(v) => &v.rest,
        StreamEvent::Ping(v) => &v.rest,
        StreamEvent::Error(v) => &v.rest,
        #[cfg(not(feature = "exhaustive"))]
        _ => panic!(),
    }
}
#[test]
fn eight_events_six_deltas_and_unknown_extensions_preserve_typed_fields() {
    for wire in [
        json!({"type":"message_start","message":{"type":"message","id":"m","role":"assistant","content":[],"model":"m","stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":1,"output_tokens":1}}}),
        json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"x"}}),
        json!({"type":"content_block_stop","index":0}),
        json!({"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"output_tokens":2}}),
        json!({"type":"message_stop"}),
        json!({"type":"ping"}),
        json!({"type":"error","error":{"type":"overloaded_error","message":"Overloaded"},"request_id":"r"}),
    ] {
        let p = round::<StreamEvent>(wire.clone());
        assert!(event_rest(&p).is_empty());
        let mut wire = wire;
        wire["future"] = json!({"v":1});
        let p = round::<StreamEvent>(wire);
        assert_eq!(event_rest(&p).len(), 1);
    }
    for delta in [
        json!({"type":"text_delta","text":"x"}),
        json!({"type":"input_json_delta","partial_json":"{\"x\":"}),
        json!({"type":"thinking_delta","thinking":"x"}),
        json!({"type":"signature_delta","signature":"sig"}),
        json!({"type":"citations_delta","citation":{"type":"web_search_result_location","cited_text":"x","encrypted_index":"cipher","title":null,"url":"u"}}),
        json!({"type":"compaction_delta","content":null,"encrypted_content":"cipher"}),
    ] {
        let p = round::<StreamEvent>(json!({"type":"content_block_delta","index":0,"delta":delta}));
        if let StreamEvent::ContentBlockDelta(e) = p {
            assert!(e.rest.is_empty());
            match e.delta {
                ContentBlockDelta::Text(v) => assert!(v.rest.is_empty()),
                ContentBlockDelta::InputJson(v) => assert!(v.rest.is_empty()),
                ContentBlockDelta::Thinking(v) => assert!(v.rest.is_empty()),
                ContentBlockDelta::Signature(v) => assert!(v.rest.is_empty()),
                ContentBlockDelta::Citations(v) => assert!(v.rest.is_empty()),
                ContentBlockDelta::Compaction(v) => assert!(v.rest.is_empty()),
                #[cfg(not(feature = "exhaustive"))]
                _ => panic!(),
            }
        }
    }
    assert!(
        serde_json::from_value::<StreamEvent>(json!({"type":"future_event","payload":true}))
            .is_err()
    );
    assert!(
        serde_json::from_value::<ContentBlockDelta>(json!({"type":"future_delta","text":"x"}))
            .is_err()
    );
}
#[test]
fn delta_usage_is_cumulative_and_input_counts_are_optional() {
    let mut outputs = vec![];
    for count in [5, 9] {
        let p = round::<StreamEvent>(
            json!({"type":"message_delta","delta":{},"usage":{"output_tokens":count}}),
        );
        if let StreamEvent::MessageDelta(v) = p {
            assert!(v.usage.input_tokens.is_none());
            outputs.push(v.usage.output_tokens);
        }
    }
    assert_eq!(outputs, vec![5, 9]); // Wire values are cumulative, never implicitly summed.
    let usage = json!({"cache_creation_input_tokens":1,"cache_read_input_tokens":2,"fallback_credit":{"status":{"type":"redeemed"}},"input_tokens":3,"iterations":[{"type":"compaction","cache_creation":null,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"input_tokens":3,"output_tokens":2}],"output_tokens":9,"output_tokens_details":{"thinking_tokens":2},"server_tool_use":{"web_search_requests":1,"web_fetch_requests":0}});
    let p = round::<MessageDeltaUsage>(usage);
    assert!(p.rest.is_empty());
    assert!(p.output_tokens_details.unwrap().unwrap().rest.is_empty());
    for field in [
        "cache_creation_input_tokens",
        "cache_read_input_tokens",
        "fallback_credit",
        "input_tokens",
        "iterations",
        "output_tokens_details",
        "server_tool_use",
    ] {
        let mut v = json!({"output_tokens":1});
        v[field] = Value::Null;
        let p = round::<MessageDeltaUsage>(v);
        assert!(p.rest.is_empty());
    }
    assert!(serde_json::from_value::<MessageDeltaUsage>(json!({"input_tokens":1})).is_err());
    assert!(serde_json::from_value::<MessageDeltaUsage>(json!({"output_tokens":null})).is_err());
}
#[test]
fn message_context_container_stop_details_and_start_usage_are_complete() {
    let details = json!({"type":"refusal","category":"cyber","explanation":null,"fallback_credit_token":null,"fallback_has_prefill_claim":null});
    let context = json!({"applied_edits":[{"type":"clear_tool_uses_20250919","cleared_input_tokens":1,"cleared_tool_uses":1}]});
    let container = json!({"id":"c","expires_at":"2026-09-13T00:00:00Z","skills":[{"skill_id":"s","type":"custom","version":"1"}]});
    let wire = json!({"type":"message_delta","context_management":context,"delta":{"container":container,"stop_details":details,"stop_reason":"refusal","stop_sequence":null},"usage":{"output_tokens":3}});
    let p = round::<StreamEvent>(wire);
    if let StreamEvent::MessageDelta(e) = p {
        assert!(e.rest.is_empty());
        assert!(e.context_management.unwrap().unwrap().rest.is_empty());
        assert!(e.delta.rest.is_empty());
        assert!(e.delta.container.unwrap().unwrap().rest.is_empty());
        assert!(e.delta.stop_details.unwrap().unwrap().rest.is_empty());
    }
    let usage = json!({"cache_creation":{"ephemeral_1h_input_tokens":1,"ephemeral_5m_input_tokens":2},"cache_creation_input_tokens":3,"cache_read_input_tokens":4,"fallback_credit":null,"inference_geo":"us","input_tokens":5,"iterations":null,"output_tokens":1,"output_tokens_details":{"thinking_tokens":1},"server_tool_use":{"web_fetch_requests":1,"web_search_requests":2},"service_tier":"priority","speed":"fast"});
    let p = round::<StreamMessage>(
        json!({"type":"message","id":"m","container":null,"content":[],"context_management":null,"diagnostics":null,"model":"m","role":"assistant","stop_details":null,"stop_reason":null,"stop_sequence":null,"usage":usage}),
    );
    assert!(p.rest.is_empty());
    assert!(p.usage.rest.is_empty());
    assert_eq!(p.stop_reason, Some(None));
    for field in ["container", "stop_details", "stop_reason", "stop_sequence"] {
        let mut v = json!({});
        v[field] = Value::Null;
        let p = round::<MessageDelta>(v);
        assert!(p.rest.is_empty());
    }
    for wire in [
        json!({}),
        json!({"content":"summary"}),
        json!({"encrypted_content":"cipher"}),
        json!({"content":null,"encrypted_content":null}),
    ] {
        let p = round::<CompactionDelta>(wire);
        assert!(p.rest.is_empty());
    }
}
#[test]
fn typed_builders_and_http_stream_body_are_independent_of_event_models() {
    let v = StreamEvent::ContentBlockDelta(
        ContentBlockDeltaEvent::builder(
            0,
            ContentBlockDelta::Compaction(
                CompactionDelta::builder()
                    .encrypted_content(Some("cipher".into()))
                    .build(),
            ),
        )
        .build(),
    );
    assert_eq!(
        serde_json::to_value(v).unwrap(),
        json!({"type":"content_block_delta","index":0,"delta":{"type":"compaction_delta","encrypted_content":"cipher"}})
    );
    let p = StreamEvent::MessageDelta(
        MessageDeltaEvent::builder(
            MessageDelta::builder()
                .stop_reason(Some(StreamStopReason::EndTurn))
                .build(),
            MessageDeltaUsage::builder(9).build(),
        )
        .build()
        .into(),
    );
    assert_eq!(
        serde_json::to_value(p).unwrap(),
        json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":9}})
    );
    struct Empty;
    impl futures_core::Stream for Empty {
        type Item = Result<bytes::Bytes, gproxy_protocol::connection::TransportError>;
        fn poll_next(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            std::task::Poll::Ready(None)
        }
    }
    let stream: StreamResponse = StreamResponse {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: Box::pin(Empty),
    };
    assert_eq!(stream.status, http::StatusCode::OK);
}

#[test]
fn all_seventeen_response_block_kinds_are_valid_stream_starts() {
    use serde_json::json;
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
        let p = round::<StreamEvent>(
            json!({"type":"content_block_start","index":0,"content_block":block}),
        );
        assert!(event_rest(&p).is_empty());
        if let StreamEvent::ContentBlockStart(v) = p {
            match v.content_block {
                response::ResponseContentBlock::Text(x) => assert!(x.rest.is_empty()),
                response::ResponseContentBlock::Thinking(x) => assert!(x.rest.is_empty()),
                response::ResponseContentBlock::RedactedThinking(x) => assert!(x.rest.is_empty()),
                response::ResponseContentBlock::ToolUse(x) => assert!(x.rest.is_empty()),
                response::ResponseContentBlock::ServerToolUse(x) => assert!(x.rest.is_empty()),
                response::ResponseContentBlock::WebSearchToolResult(x) => {
                    assert!(x.rest.is_empty())
                }
                response::ResponseContentBlock::WebFetchToolResult(x) => assert!(x.rest.is_empty()),
                response::ResponseContentBlock::AdvisorToolResult(x) => assert!(x.rest.is_empty()),
                response::ResponseContentBlock::CodeExecutionToolResult(x) => {
                    assert!(x.rest.is_empty())
                }
                response::ResponseContentBlock::BashCodeExecutionToolResult(x) => {
                    assert!(x.rest.is_empty())
                }
                response::ResponseContentBlock::TextEditorCodeExecutionToolResult(x) => {
                    assert!(x.rest.is_empty())
                }
                response::ResponseContentBlock::ToolSearchToolResult(x) => {
                    assert!(x.rest.is_empty())
                }
                response::ResponseContentBlock::McpToolListing(x) => assert!(x.rest.is_empty()),
                response::ResponseContentBlock::McpToolUse(x) => assert!(x.rest.is_empty()),
                response::ResponseContentBlock::McpToolResult(x) => assert!(x.rest.is_empty()),
                response::ResponseContentBlock::ContainerUpload(x) => assert!(x.rest.is_empty()),
                response::ResponseContentBlock::Compaction(x) => assert!(x.rest.is_empty()),
                response::ResponseContentBlock::Fallback(x) => assert!(x.rest.is_empty()),
                #[cfg(not(feature = "exhaustive"))]
                _ => panic!(),
            }
        }
    }
}

#[test]
fn all_five_citation_delta_variants_are_typed() {
    use serde_json::json;
    for citation in [
        json!({"type":"char_location","cited_text":"t","document_index":0,"document_title":null,"file_id":null,"start_char_index":0,"end_char_index":1}),
        json!({"type":"page_location","cited_text":"t","document_index":0,"document_title":"doc","file_id":"file","start_page_number":1,"end_page_number":2}),
        json!({"type":"content_block_location","cited_text":"t","document_index":0,"document_title":null,"file_id":null,"start_block_index":0,"end_block_index":1}),
        json!({"type":"web_search_result_location","cited_text":"t","encrypted_index":"cipher","title":null,"url":"url"}),
        json!({"type":"search_result_location","cited_text":"t","end_block_index":1,"search_result_index":0,"source":"source","start_block_index":0,"title":null}),
    ] {
        let p = round::<CitationsDelta>(json!({"citation":citation}));
        assert!(p.rest.is_empty());
        match p.citation {
            response::ResponseTextCitation::Char(v) => assert!(v.rest.is_empty()),
            response::ResponseTextCitation::Page(v) => assert!(v.rest.is_empty()),
            response::ResponseTextCitation::ContentBlock(v) => assert!(v.rest.is_empty()),
            response::ResponseTextCitation::Web(v) => assert!(v.rest.is_empty()),
            response::ResponseTextCitation::Search(v) => assert!(v.rest.is_empty()),
            #[cfg(not(feature = "exhaustive"))]
            _ => panic!(),
        }
    }
}

#[test]
fn required_fields_closed_errors_and_snake_case_are_enforced() {
    for value in [
        "api_error",
        "authentication_error",
        "billing_error",
        "invalid_request_error",
        "not_found_error",
        "overloaded_error",
        "permission_error",
        "rate_limit_error",
        "timeout_error",
        "conflict_error",
        "request_too_large",
    ] {
        round::<StreamErrorType>(json!(value));
    }
    assert!(serde_json::from_value::<StreamErrorType>(json!("future_error")).is_err());
    for wire in [
        json!({"type":"message_start","message":{}}),
        json!({"type":"content_block_start","content_block":{"type":"text","text":"x"}}),
        json!({"type":"content_block_delta","index":0}),
        json!({"type":"content_block_stop"}),
        json!({"type":"message_delta","delta":{}}),
        json!({"type":"error","error":{"type":"api_error"}}),
    ] {
        assert!(serde_json::from_value::<StreamEvent>(wire).is_err());
    }
    let wire = json!({"type":"message_delta","contextManagement":{"future":true},"delta":{"stopReason":"end_turn"},"usage":{"output_tokens":1}});
    let p = round::<StreamEvent>(wire);
    if let StreamEvent::MessageDelta(v) = p {
        assert!(v.context_management.is_none());
        assert!(v.rest.contains_key("contextManagement"));
        assert!(v.delta.stop_reason.is_none());
        assert!(v.delta.rest.contains_key("stopReason"));
    }
    let p = round::<MessageDeltaEvent>(
        json!({"context_management":null,"delta":{},"usage":{"output_tokens":1}}),
    );
    assert_eq!(p.context_management, Some(None));
}
