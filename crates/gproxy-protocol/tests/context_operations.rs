use gproxy_protocol::openai::{compact::*, conversation::*, memory::*, web_search::*};

#[test]
fn compact_and_conversation_payloads_round_trip_nullable_fields() {
    let compact = serde_json::json!({"model":null,"input":"hello","instructions":null,"previous_response_id":null,"prompt_cache_key":"k","service_tier":"fast","future":true});
    let body: CompactRequestBody = serde_json::from_value(compact.clone()).unwrap();
    assert_eq!(serde_json::to_value(body).unwrap(), compact);
    let conversation = serde_json::json!({"items":null,"metadata":{"k":"v"},"future":1});
    let body: CreateConversationRequestBody = serde_json::from_value(conversation.clone()).unwrap();
    assert_eq!(serde_json::to_value(body).unwrap(), conversation);
}

#[test]
fn memory_and_web_search_known_fields_and_unknown_rest_round_trip() {
    let memory = serde_json::json!({"model":"gpt","traces":[{"id":"t","metadata":{"source_path":"/tmp/a"},"items":[]}],"future":1});
    let body: MemorySummarizeRequestBody = serde_json::from_value(memory.clone()).unwrap();
    assert_eq!(serde_json::to_value(body).unwrap(), memory);
    let search = serde_json::json!({"id":"s","model":"gpt","input":"find","commands":{"search_query":[{"q":"openai","recency":7,"domains":["openai.com"]}]},"settings":{"search_context_size":"high","user_location":{"type":"approximate","country":"US"}},"future":true});
    let body: WebSearchRequestBody = serde_json::from_value(search.clone()).unwrap();
    assert_eq!(serde_json::to_value(body).unwrap(), search);
}

fn exact<T: serde::Serialize + serde::de::DeserializeOwned + std::fmt::Debug>(
    value: serde_json::Value,
) {
    let parsed: T = serde_json::from_value(value.clone()).unwrap();
    assert!(
        format!("{parsed:?}")
            .split("rest: ")
            .skip(1)
            .all(|rest| rest.starts_with("{}")),
        "known fields in rest: {parsed:?}"
    );
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
}
#[test]
fn compact_conversation_and_memory_follow_their_actual_contracts() {
    use serde_json::json;
    exact::<CompactRequestBody>(json!({"model":null}));
    assert!(serde_json::from_value::<CompactRequestBody>(json!({})).is_err());
    let value = json!({"model":"m","input":"x","instructions":"i","previous_response_id":"r","prompt_cache_key":"p","prompt_cache_options":{"mode":"explicit","ttl":"30m"},"prompt_cache_retention":"24h","service_tier":"priority"});
    for key in [
        "input",
        "instructions",
        "previous_response_id",
        "prompt_cache_key",
        "prompt_cache_options",
        "prompt_cache_retention",
        "service_tier",
    ] {
        let mut null = value.clone();
        null[key] = json!(null);
        exact::<CompactRequestBody>(null);
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(key);
        exact::<CompactRequestBody>(missing);
    }
    exact::<CompactResponseBody>(
        json!({"id":"c","created_at":1,"object":"response.compaction","output":[{"type":"compaction","id":"i","encrypted_content":"e"}],"usage":{"input_tokens":1,"input_tokens_details":{"cache_write_tokens":0,"cached_tokens":0},"output_tokens":1,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":2}}),
    );
    for metadata in [json!(null), json!([1]), json!({"k":"v"})] {
        exact::<ConversationResponseBody>(
            json!({"id":"c","created_at":1,"object":"conversation","metadata":metadata}),
        );
    }
    exact::<MemorySummarizeRequestBody>(
        json!({"model":"m","traces":[{"id":"i","metadata":{"source_path":"/p"},"items":[null,1,"opaque",{"new_type":true}]}],"reasoning":{"effort":"model_defined","summary":"none","context":"all_turns"}}),
    );
    let output: MemorySummarizeOutput =
        serde_json::from_value(json!({"raw_memory":"raw","memory_summary":"sum"})).unwrap();
    assert_eq!(
        serde_json::to_value(output).unwrap(),
        json!({"trace_summary":"raw","memory_summary":"sum"})
    );
    exact::<MemorySummarizeResponseBody>(
        json!({"output":[{"trace_summary":"t","memory_summary":"m"}]}),
    );
}
#[test]
fn native_search_all_commands_and_settings_are_typed() {
    use serde_json::json;
    exact::<WebSearchRequestBody>(
        json!({"id":"s","model":"m","reasoning":{"effort":"high","summary":"concise","context":"current_turn"},"input":"q","max_output_tokens":10,"commands":{"search_query":[{"q":"q","recency":1,"domains":["a"]}],"image_query":[{"q":"i"}],"open":[{"ref_id":"r","lineno":2}],"click":[{"ref_id":"r","id":1}],"find":[{"ref_id":"r","pattern":"x"}],"screenshot":[{"ref_id":"r","pageno":0}],"finance":[{"ticker":"SPX","type":"index","market":"USA"}],"weather":[{"location":"FR, Paris","start":"2026-01-01","duration":2}],"sports":[{"tool":"sports","fn":"schedule","league":"nba","team":"GSW","opponent":"BOS","date_from":"2026-01-01","date_to":"2026-01-02","num_games":1,"locale":"en"}],"time":[{"utc_offset":"+01:00"}],"response_length":"long"},"settings":{"user_location":{"type":"approximate","country":"US","region":"CA","city":"SF","timezone":"UTC"},"search_context_size":"high","filters":{"allowed_domains":["a"],"blocked_domains":["b"]},"image_settings":{"max_results":2,"caption":true},"allowed_callers":["direct","shell","code_interpreter"],"external_web_access":"indexed"}}),
    );
    exact::<WebSearchRequestBody>(
        json!({"id":"s","model":"m","settings":{"external_web_access":false},"commands":{"weather":[{"location":"US"}],"sports":[{"fn":"standings","league":"ipl"}],"open":[{"ref_id":"r"}]}}),
    );
    for value in [
        json!({"output":"text"}),
        json!({"output":"text","encrypted_output":null,"results":null}),
        json!({"output":"text","encrypted_output":"e","results":[1,null,{"future":"opaque"}]}),
    ] {
        exact::<WebSearchResponseBody>(value);
    }
    assert!(serde_json::from_value::<FinanceAssetType>(json!("stock")).is_err());
    assert!(serde_json::from_value::<SportsOperation>(json!({"fn":"schedule"})).is_err());
}

fn client_items() -> Vec<serde_json::Value> {
    use serde_json::json;
    vec![
        json!({"type":"additional_tools","role":"system","tools":[{"future":true}]}),
        json!({"type":"message","role":"assistant","content":[{"type":"input_text","text":"a"},{"type":"input_image","image_url":"u","detail":"original"},{"type":"input_audio","audio_url":"a"},{"type":"output_text","text":"b"}],"phase":"final_answer","internal_chat_message_metadata_passthrough":{"turn_id":"t","create_time":1.5,"content_item_kinds":["future"],"cell_id":"c","tool_calls_complete":true,"executed_tool_calls":[{"name":"f","arguments":{"x":1},"tool_result_sources":[{"type":"file","id":"i"}]}]}}),
        json!({"type":"agent_message","author":"a","recipient":"b","content":[{"type":"input_text","text":"a"},{"type":"encrypted_content","encrypted_content":"e"}]}),
        json!({"type":"reasoning","summary":[{"type":"summary_text","text":"s"}],"encrypted_content":null,"content":[{"type":"reasoning_text","text":"r"}]}),
        json!({"type":"local_shell_call","call_id":null,"status":"completed","action":{"type":"exec","command":["pwd"],"timeout_ms":null,"working_directory":null,"env":null,"user":null}}),
        json!({"type":"function_call","name":"f","arguments":"{}","call_id":"c"}),
        json!({"type":"tool_search_call","call_id":null,"execution":"server","arguments":null}),
        json!({"type":"function_call_output","output":"text"}),
        json!({"type":"custom_tool_call","call_id":"c","name":"c","input":"raw"}),
        json!({"type":"custom_tool_call_output","call_id":"c","output":[{"type":"encrypted_content","encrypted_content":"e"}]}),
        json!({"type":"tool_search_output","call_id":null,"status":"completed","execution":"server","tools":[null]}),
        json!({"type":"web_search_call","action":{"type":"search","queries":["q"]}}),
        json!({"type":"image_generation_call","status":"completed","result":"b64"}),
        json!({"type":"compaction","encrypted_content":"e"}),
        json!({"type":"configuration_update","reasoning":{"effort":"model_defined"}}),
        json!({"type":"compaction_trigger"}),
        json!({"type":"context_compaction"}),
        json!({"type":"other"}),
    ]
}
#[test]
fn guardian_native_client_items_and_root_extensions_do_not_use_public_ir() {
    use gproxy_protocol::openai::guardian::*;
    use serde_json::json;
    for item in client_items() {
        exact::<ClientResponseItem>(item);
    }
    let value = json!({"model":"m","instructions":"i","input":client_items(),"tools":[{"custom_native":true}],"tool_choice":"auto","parallel_tool_calls":true,"reasoning":null,"store":false,"stream":true,"stream_options":{"reasoning_summary_delivery":"sequential_cutoff"},"include":["reasoning.encrypted_content"],"service_tier":"future-tier","prompt_cache_key":"k","text":{"verbosity":"low","format":{"type":"json_schema","strict":true,"schema":true,"name":"n"}},"client_metadata":{"source":"guardian"},"access_programs":{"cyber":"daybreak_blue"}});
    exact::<GuardianRequestBody>(value.clone());
    let mut missing = value;
    missing.as_object_mut().unwrap().remove("reasoning");
    assert!(serde_json::from_value::<GuardianRequestBody>(missing).is_err());
    exact::<WebSearchRequestBody>(json!({"id":"s","model":"m","input":client_items()}));
    let mut headers = http::HeaderMap::new();
    headers.insert("x-openai-subagent", "guardian".parse().unwrap());
    let body = GuardianRequestBody::builder(
        "m".into(),
        "".into(),
        vec![],
        "auto".into(),
        true,
        None,
        false,
        true,
        vec![],
    )
    .build();
    let request: GuardianClassifierRequest = gproxy_protocol::WireRequest {
        method: http::Method::POST,
        path: "/guardian-classifier".into(),
        query: None,
        headers: headers.clone(),
        body,
    };
    assert_eq!(request.method, http::Method::POST);
    assert_eq!(request.path, "/guardian-classifier");
    assert_eq!(request.headers, headers);
    assert!(request.query.is_none());
    assert_eq!(
        serde_json::to_value(request.body).unwrap(),
        json!({"model":"m","input":[],"tool_choice":"auto","parallel_tool_calls":true,"reasoning":null,"store":false,"stream":true,"include":[]})
    );
}

#[test]
fn context_http_envelopes_and_native_optional_payloads_are_separate() {
    use gproxy_protocol::{WireRequest, WireResponse};
    use serde_json::json;
    let request: CompactRequest = WireRequest {
        method: http::Method::POST,
        path: "/responses/compact".into(),
        query: Some("trace=1".into()),
        headers: http::HeaderMap::new(),
        body: CompactRequestBody::builder(None).build(),
    };
    assert_eq!(request.path, "/responses/compact");
    assert_eq!(request.query.as_deref(), Some("trace=1"));
    assert_eq!(
        serde_json::to_value(request.body).unwrap(),
        json!({"model":null})
    );
    let response: CreateConversationResponse = WireResponse {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: ConversationResponseBody::builder(
            "c".into(),
            1,
            json!(null),
            ConversationObject::Conversation,
        )
        .build(),
    };
    assert_eq!(response.status, http::StatusCode::OK);
    assert!(response.headers.is_empty());
    assert_eq!(
        serde_json::to_value(response.body).unwrap(),
        json!({"id":"c","created_at":1,"metadata":null,"object":"conversation"})
    );
    let body: gproxy_protocol::openai::guardian::GuardianRequestBody=serde_json::from_value(json!({"model":"m","input":[],"tools":null,"tool_choice":"auto","parallel_tool_calls":true,"reasoning":null,"store":false,"stream":true,"include":[]})).unwrap();
    assert_eq!(serde_json::to_value(body).unwrap()["tools"], json!(null));
    for asset in ["equity", "fund", "crypto", "index"] {
        exact::<FinanceAssetType>(json!(asset));
    }
    for league in [
        "nba", "wnba", "nfl", "nhl", "mlb", "epl", "ncaamb", "ncaawb", "ipl",
    ] {
        exact::<SportsLeague>(json!(league));
    }
    for mode in [
        json!(true),
        json!(false),
        json!("cached"),
        json!("indexed"),
        json!("live"),
    ] {
        exact::<ExternalWebAccess>(mode);
    }
}
