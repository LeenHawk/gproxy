use gproxy_protocol::claude::generate_content::*;
use gproxy_protocol::{WireRequest, WireResponse};
use http::{HeaderMap, HeaderValue, Method, StatusCode};

#[test]
fn generate_request_full_fixture_round_trips_and_keeps_http_metadata_outside_body() {
    let json = serde_json::json!({
        "max_tokens": 128,
        "messages": [{"role":"user","content":[{"type":"text","text":"hello"}]}],
        "model":"claude-sonnet-4-6",
        "cache_control":{"type":"ephemeral","ttl":"5m"},
        "container":{"id":"cont_1","skills":[{"skill_id":"s1","type":"custom","version":"latest"}]},
        "metadata":{"user_id":"u1"},
        "context_management":{"edits":[{"type":"compact_20260112","instructions":"summarize","pause_after_compaction":false,"trigger":{"type":"input_tokens","value":100}}]},
        "diagnostics":{"previous_message_id":"msg_previous"},
        "inference_geo":"global",
        "mcp_servers":[{"name":"server","type":"url","url":"https://example.com/mcp","authorization_token":"token","tool_configuration":{"allowed_tools":["search"],"enabled":true}}],
        "output_format":{"type":"json_schema","schema":{"type":"object"}},
        "thinking":{"type":"adaptive","display":"summarized"},
        "tool_choice":{"type":"auto","disable_parallel_tool_use":true},
        "tools":[{"name":"search","input_schema":{"type":"object","properties":{"query":{"type":"string"}}},"description":"Search"}],
        "output_config":{"effort":"high","task_budget":{"type":"tokens","total":100}},
        "service_tier":"auto","speed":"fast","stop_sequences":["END"],"stream":false,
        "system":"be helpful","temperature":0.2,"top_k":20,"top_p":0.9,
        "future": {"x": true}
    });
    let body: GenerateContentRequestBody = serde_json::from_value(json.clone()).unwrap();
    let encoded = serde_json::to_value(&body).unwrap();
    assert_eq!(encoded, json);
    assert!(body.rest.contains_key("future"));
    let mut known_body = body.clone();
    known_body.rest.clear();
    assert!(!format!("{known_body:?}").contains("rest: {\""));
    let mut headers = HeaderMap::new();
    headers.insert(
        "anthropic-beta",
        HeaderValue::from_static("token-counting-2024-11-01"),
    );
    let request = WireRequest {
        method: Method::POST,
        path: "/v1/messages".into(),
        query: None,
        headers,
        body,
    };
    assert_eq!(request.path, "/v1/messages");
    assert!(
        !serde_json::to_value(request.body)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("anthropic-beta")
    );
}

#[test]
fn generate_response_all_primary_fields_and_unknown_rest_round_trip() {
    let json = serde_json::json!({
        "id":"msg_1","type":"message","role":"assistant","model":"claude-sonnet-4-6",
        "content":[{"type":"text","text":"hello"},{"type":"thinking","thinking":"reason","signature":"sig"}],
        "container":{"id":"cont_1","expires_at":"2026-01-01T00:00:00Z","skills":[{"skill_id":"s1","type":"anthropic","version":"latest"}]},
        "stop_reason":"end_turn","stop_sequence":null,
        "usage":{"input_tokens":10,"output_tokens":4,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"cache_creation":{"ephemeral_1h_input_tokens":0,"ephemeral_5m_input_tokens":0},"inference_geo":"us-east","future_usage":7},
        "future_response":true
    });
    let body: GenerateContentResponseBody = serde_json::from_value(json.clone()).unwrap();
    assert_eq!(serde_json::to_value(&body).unwrap(), json);
    assert!(body.rest.contains_key("future_response"));
    let response = WireResponse {
        status: StatusCode::OK,
        headers: HeaderMap::new(),
        body,
    };
    assert_eq!(response.status, StatusCode::OK);
}

fn round_trip_known<T>(value: serde_json::Value) -> T
where
    T: serde::de::DeserializeOwned + serde::Serialize + std::fmt::Debug,
{
    let body: T = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(serde_json::to_value(&body).unwrap(), value);
    // A successful JSON round trip alone can hide documented fields in Rest.
    assert!(!format!("{body:?}").contains("rest: {\""), "{body:?}");
    body
}

#[test]
fn request_fallback_unions_and_diagnostics_null_round_trip() {
    use serde_json::json;
    for token in [
        json!("opaque"),
        json!({"token":"opaque"}),
        json!({"token":"opaque","mode":"best_effort"}),
    ] {
        let request = json!({"max_tokens":64,"model":"m","messages":[],"fallback_credit_token":token,"diagnostics":{"previous_message_id":null}});
        round_trip_known::<GenerateContentRequestBody>(request);
    }
    for fallbacks in [
        json!("default"),
        json!([{"model":"m","max_tokens":64,"output_config":{"effort":"high"},"speed":"standard","thinking":{"type":"disabled"}}]),
    ] {
        round_trip_known::<GenerateContentRequestBody>(
            json!({"max_tokens":64,"model":"m","messages":[],"fallbacks":fallbacks,"container":"container_1"}),
        );
    }
    assert!(serde_json::from_value::<FallbacksParam>(json!("anything")).is_err());
    assert!(
        serde_json::from_value::<FallbackCreditTokenParam>(json!({"token":"x","mode":"anything"}))
            .is_err()
    );
}

#[test]
fn output_content_variants_have_complete_typed_nested_fields() {
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
        round_trip_known::<ResponseContentBlock>(block);
    }
}

#[test]
fn all_output_citations_preserve_nullable_titles_and_file_ids() {
    use serde_json::json;
    for citation in [
        json!({"type":"char_location","cited_text":"t","document_index":0,"document_title":null,"file_id":null,"start_char_index":0,"end_char_index":1}),
        json!({"type":"page_location","cited_text":"t","document_index":0,"document_title":"doc","file_id":"file","start_page_number":1,"end_page_number":2}),
        json!({"type":"content_block_location","cited_text":"t","document_index":0,"document_title":null,"file_id":null,"start_block_index":0,"end_block_index":1}),
        json!({"type":"web_search_result_location","cited_text":"t","encrypted_index":"cipher","title":null,"url":"url"}),
        json!({"type":"search_result_location","cited_text":"t","end_block_index":1,"search_result_index":0,"source":"source","start_block_index":0,"title":null}),
    ] {
        round_trip_known::<ResponseTextCitation>(citation);
    }
}

#[test]
fn diagnostics_iterations_and_credit_status_are_typed_unions() {
    use serde_json::json;
    for kind in [
        "model_changed",
        "system_changed",
        "tools_changed",
        "messages_changed",
        "previous_message_not_found",
        "unavailable",
    ] {
        let mut value = json!({"type":kind});
        if kind.ends_with("changed") {
            value["cache_missed_input_tokens"] = json!(12);
        }
        round_trip_known::<DiagnosticsResponse>(json!({"cache_miss_reason":value}));
    }
    round_trip_known::<DiagnosticsResponse>(json!({"cache_miss_reason":null}));
    let mut iterations = vec![];
    for kind in [
        "message",
        "compaction",
        "advisor_message",
        "fallback_message",
    ] {
        let mut value = json!({"type":kind,"cache_creation":null,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"input_tokens":5,"output_tokens":2});
        if kind != "compaction" {
            value["model"] = json!("m");
        }
        round_trip_known::<IterationUsage>(value.clone());
        iterations.push(value);
    }
    for reason in [
        "body_mismatch",
        "continuation_excluded",
        "continuation_only",
        "expired",
        "invalid_target_model",
        "not_enabled",
        "reprice_unavailable",
        "temporarily_unavailable",
        "variant_fields_present",
        "wrong_organization",
        "wrong_platform",
        "wrong_workspace",
    ] {
        let mut status = json!({"type":"not_applied","reason":reason});
        if reason == "variant_fields_present" {
            status["remove_to_redeem"] = json!(["speed"]);
        }
        round_trip_known::<Usage>(
            json!({"input_tokens":5,"output_tokens":2,"iterations":iterations,"fallback_credit":{"status":status},"cache_creation_input_tokens":null,"cache_read_input_tokens":null,"inference_geo":null,"output_tokens_details":{"thinking_tokens":1},"server_tool_use":{"web_fetch_requests":1,"web_search_requests":0},"service_tier":"priority","speed":"fast"}),
        );
    }
    for category in [
        "cyber",
        "bio",
        "frontier_llm",
        "reasoning_extraction",
        "general_harms",
    ] {
        round_trip_known::<RefusalStopDetails>(
            json!({"type":"refusal","category":category,"explanation":null,"fallback_credit_token":null,"fallback_has_prefill_claim":null,"recommended_model":null}),
        );
    }
    round_trip_known::<ContextEditResponse>(
        json!({"type":"clear_thinking_20251015","cleared_input_tokens":1,"cleared_thinking_turns":1}),
    );
}

#[test]
fn response_contracts_reject_request_only_shapes_and_missing_required_fields() {
    use serde_json::json;
    for block in [
        json!({"type":"image","source":{"type":"url","url":"url"}}),
        json!({"type":"tool_result","tool_use_id":"id"}),
        json!({"type":"mid_conv_system","content":[]}),
        json!({"type":"mcp_tool_result","tool_use_id":"id"}),
        json!({"type":"fallback","from":{"model":"m"},"to":{"model":"n"}}),
    ] {
        assert!(serde_json::from_value::<ResponseContentBlock>(block).is_err());
    }
    for source in [
        json!({"type":"url","url":"url"}),
        json!({"type":"file","file_id":"file"}),
        json!({"type":"content","content":"text"}),
    ] {
        assert!(serde_json::from_value::<ResponseDocumentSource>(source).is_err());
    }
    assert!(serde_json::from_value::<ResponseCitationConfig>(json!({})).is_err());
    assert!(
        serde_json::from_value::<ResponseTextBlock>(json!({"type":"image","text":"text"})).is_err()
    );
    assert!(
        serde_json::from_value::<ResponseTextCitation>(
            json!({"type":"char_location","cited_text":"t","document_index":0,"end_char_index":1})
        )
        .is_err()
    );
    let mut message = json!({"id":"id","type":"message","role":"assistant","model":"m","content":[],"usage":{"input_tokens":1,"output_tokens":1},"stop_reason":"end_turn"});
    round_trip_known::<GenerateContentResponseBody>(message.clone());
    message["stop_reason"] = json!(null);
    assert!(serde_json::from_value::<GenerateContentResponseBody>(message.clone()).is_err());
    message.as_object_mut().unwrap().remove("stop_reason");
    assert!(serde_json::from_value::<GenerateContentResponseBody>(message).is_err());
}

#[test]
fn unknown_fields_cannot_redirect_discriminated_output_variants() {
    use serde_json::json;
    let value = json!({"type":"text_editor_code_execution_str_replace_result","is_file_update":true,"future":{"x":null}});
    let result: ResponseTextEditorResultContent = serde_json::from_value(value.clone()).unwrap();
    let ResponseTextEditorResultContent::Replace(ref replace) = result else {
        panic!("wrong variant");
    };
    assert_eq!(replace.rest["is_file_update"], true);
    assert_eq!(serde_json::to_value(result).unwrap(), value);
    let value = json!({"type":"not_applied","reason":"expired","future":[1,null]});
    let status: FallbackCreditStatus = serde_json::from_value(value.clone()).unwrap();
    assert!(matches!(&status, FallbackCreditStatus::NotApplied(_)));
    assert_eq!(serde_json::to_value(status).unwrap(), value);
    let value = json!({"type":"unavailable","cache_missed_input_tokens":42,"future":[1,null]});
    let reason: CacheMissReason = serde_json::from_value(value.clone()).unwrap();
    assert!(matches!(&reason, CacheMissReason::Unavailable(_)));
    assert_eq!(serde_json::to_value(reason).unwrap(), value);
}

#[test]
fn typed_builders_emit_tags_and_preserve_explicit_null_without_rest() {
    use serde_json::json;
    let reference =
        ResponseToolReferenceBlock::builder(ResponseToolReferenceBlockType::Tag, "search".into())
            .build();
    let result =
        ResponseToolSearchResults::builder(ResponseToolSearchResultsType::Tag, vec![reference])
            .build();
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        json!({"type":"tool_search_tool_search_result","tool_references":[{"type":"tool_reference","tool_name":"search"}]})
    );
    let failure = ResponseCompactionBlock::builder(ResponseCompactionBlockType::Tag)
        .content(None::<String>)
        .build();
    assert_eq!(
        serde_json::to_value(failure).unwrap(),
        json!({"type":"compaction","content":null})
    );
    let omitted = ResponseCompactionBlock::builder(ResponseCompactionBlockType::Tag)
        .encrypted_content(Some("cipher".into()))
        .build();
    assert_eq!(
        serde_json::to_value(omitted).unwrap(),
        json!({"type":"compaction","encrypted_content":"cipher"})
    );
    let pending = DiagnosticsResponse::builder()
        .cache_miss_reason(None::<CacheMissReason>)
        .build();
    assert_eq!(
        serde_json::to_value(pending).unwrap(),
        json!({"cache_miss_reason":null})
    );
}
