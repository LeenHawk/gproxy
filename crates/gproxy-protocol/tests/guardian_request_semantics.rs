use gproxy_protocol::{
    codec::CodecLimits,
    openai::guardian::GuardianRequestBody,
    transform::{
        TransformErrorKind,
        guardian::{self, GuardianOperation, GuardianRequestContext},
    },
};
use serde_json::{Value, json};

fn limits(cap: u64) -> CodecLimits {
    CodecLimits {
        max_buffer_bytes: cap,
        max_value_bytes: cap,
        max_body_bytes: cap,
        max_line_bytes: cap,
        max_part_bytes: cap,
        max_parts: 16,
    }
}
fn context(operation: GuardianOperation) -> GuardianRequestContext {
    GuardianRequestContext {
        target_model: "m".into(),
        max_tokens: 64,
        operation,
    }
}
fn source(extra: Value) -> GuardianRequestBody {
    let mut value = json!({"model":"guardian","instructions":"base policy","input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"act"}]}],"tools":null,"tool_choice":"auto","parallel_tool_calls":false,"reasoning":null,"store":false,"stream":true,"include":[]});
    if let (Value::Object(base), Value::Object(extra)) = (&mut value, extra) {
        base.extend(extra);
    }
    serde_json::from_value(value).unwrap()
}
fn encoded(request: guardian::GuardianPreparedRequest) -> Value {
    let request = request.into_request();
    match request {
        guardian::GuardianDialectRequest::Claude(v) => serde_json::to_value(v.body).unwrap(),
        guardian::GuardianDialectRequest::Gemini(v) => serde_json::to_value(v.body).unwrap(),
        guardian::GuardianDialectRequest::OpenAiChat(v) => serde_json::to_value(v.body).unwrap(),
        guardian::GuardianDialectRequest::OpenAiResponses(v) => {
            serde_json::to_value(v.body).unwrap()
        }
    }
}

#[test]
fn encrypted_and_unreadable_media_history_is_rejected() {
    let encrypted =
        source(json!({"input":[{"type":"compaction","id":"c","encrypted_content":"ciphertext"}]}));
    let error = guardian::prepare_openai_responses_with_limits(
        encrypted,
        context(GuardianOperation::Review),
        limits(64 * 1024),
    )
    .unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::Unsupported);
    let image = source(
        json!({"input":[{"type":"message","role":"user","content":[{"type":"input_image","image_url":"https://example.test/a.png","detail":"high"}]}]}),
    );
    let value = encoded(
        guardian::prepare_openai_chat_with_limits(
            image,
            context(GuardianOperation::Review),
            limits(64 * 1024),
        )
        .unwrap()
        .value,
    );
    assert!(value.to_string().contains("https://example.test/a.png"));
}

#[test]
fn typed_media_uses_native_target_shapes_and_gemini_requires_data_uri() {
    let chat = source(
        json!({"input":[{"type":"message","role":"user","content":[{"type":"input_image","image_url":"https://example.test/a.png","detail":"high"},{"type":"input_audio","audio_url":"data:audio/wav;base64,UklGRiYAAABXQVZFZm10IBAAAAABAAEAQB8AAIA+AAACABAAZGF0YQIAAAAAAA=="}]}]}),
    );
    let value = encoded(
        guardian::prepare_openai_chat_with_limits(
            chat,
            context(GuardianOperation::Review),
            limits(64 * 1024),
        )
        .unwrap()
        .value,
    );
    assert!(value["messages"].to_string().contains("image_url"));
    assert!(value["messages"].to_string().contains("input_audio"));
    let tool_image = source(
        json!({"input":[{"type":"function_call_output","call_id":"c","output":[{"type":"input_image","image_url":"https://example.test/tool.png","detail":"auto"}]}]}),
    );
    let value = encoded(
        guardian::prepare_openai_chat_with_limits(
            tool_image,
            context(GuardianOperation::Review),
            limits(64 * 1024),
        )
        .unwrap()
        .value,
    );
    assert!(value["messages"].to_string().contains("tool.png"));
    let claude = source(
        json!({"input":[{"type":"message","role":"user","content":[{"type":"input_image","image_url":"https://example.test/a.png","detail":"auto"}]}]}),
    );
    let value = encoded(
        guardian::prepare_claude_with_limits(
            claude,
            context(GuardianOperation::Review),
            limits(64 * 1024),
        )
        .unwrap()
        .value,
    );
    assert!(
        value["messages"]
            .to_string()
            .contains("https://example.test/a.png")
    );
    let gemini = source(
        json!({"input":[{"type":"message","role":"user","content":[{"type":"input_image","image_url":"data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC","detail":"auto"}]}]}),
    );
    let value = encoded(
        guardian::prepare_gemini_with_limits(
            gemini,
            context(GuardianOperation::Review),
            limits(64 * 1024),
        )
        .unwrap()
        .value,
    );
    assert!(value["contents"].to_string().contains("inlineData"));
    let gemini_url = source(
        json!({"input":[{"type":"message","role":"user","content":[{"type":"input_image","image_url":"https://example.test/a.png","detail":"auto"}]}]}),
    );
    let error = guardian::prepare_gemini_with_limits(
        gemini_url,
        context(GuardianOperation::Review),
        limits(64 * 1024),
    )
    .unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::MissingMetadata);
}

#[test]
fn source_schema_name_strict_and_developer_policy_are_retained() {
    let request = source(json!({
        "instructions":"base",
        "input":[{"type":"message","role":"developer","content":[{"type":"input_text","text":"developer policy"}]}],
        "text":{"format":{"type":"json_schema","strict":false,"name":"source_name","schema":{"type":"object","properties":{"decision":{"type":"string"}},"required":["decision"]}}}
    }));
    let value = encoded(
        guardian::prepare_openai_chat_with_limits(
            request,
            context(GuardianOperation::Review),
            limits(64 * 1024),
        )
        .unwrap()
        .value,
    );
    assert_eq!(
        value["response_format"]["json_schema"]["name"],
        "source_name"
    );
    assert_eq!(value["response_format"]["json_schema"]["strict"], false);
    assert_eq!(
        value["response_format"]["json_schema"]["schema"]["required"],
        json!(["decision"])
    );
    assert!(value["messages"].to_string().contains("developer policy"));
}

#[test]
fn source_reasoning_and_access_programs_are_not_silently_dropped() {
    let request = source(json!({"reasoning":{"effort":"high","summary":null,"context":null}}));
    let value = encoded(
        guardian::prepare_openai_chat_with_limits(
            request,
            context(GuardianOperation::Classify),
            limits(64 * 1024),
        )
        .unwrap()
        .value,
    );
    assert_eq!(value["reasoning_effort"], "high");
    let request = source(json!({"access_programs":{"cyber":"standard"}}));
    let error = guardian::prepare_openai_responses_with_limits(
        request,
        context(GuardianOperation::Review),
        limits(64 * 1024),
    )
    .unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::Unsupported);
}

#[test]
fn declared_rest_is_removed_and_history_cap_precedes_large_task_encoding() {
    let request = source(
        json!({"x-rest":{"poison":true},"input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"x"}],"x-rest":{"poison":true}}]}),
    );
    let value = encoded(
        guardian::prepare_openai_responses_with_limits(
            request,
            context(GuardianOperation::Review),
            limits(64 * 1024),
        )
        .unwrap()
        .value,
    );
    assert!(!value.to_string().contains("poison"));
    let request = source(
        json!({"input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"a very long task"}]}]}),
    );
    let error = guardian::prepare_openai_responses_with_limits(
        request,
        context(GuardianOperation::Review),
        limits(8),
    )
    .unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::Limit);
}

fn prepare_all(input: GuardianRequestBody, operation: GuardianOperation) -> Vec<Value> {
    [
        guardian::prepare_claude_with_limits,
        guardian::prepare_gemini_with_limits,
        guardian::prepare_openai_chat_with_limits,
        guardian::prepare_openai_responses_with_limits,
    ]
    .into_iter()
    .map(|f| {
        encoded(
            f(input.clone(), context(operation), limits(65536))
                .unwrap()
                .value,
        )
    })
    .collect()
}
#[test]
fn common_classifier_all_turns_policy_and_native_effort_store_are_supported() {
    // Matches Codex's ordinary sampler: policy is a developer message,
    // instructions is empty, store is false and reasoning context is all_turns.
    let input = source(
        json!({"instructions":"", "tool_choice":"none", "reasoning":{"effort":"high","context":"all_turns"}, "input":[
            {"type":"message","role":"developer","content":[{"type":"input_text","text":"Classify execution risk."}]},
            {"type":"message","role":"user","content":[{"type":"input_text","text":"first turn"}]},
            {"type":"message","role":"assistant","content":[{"type":"output_text","text":"second turn"}]}
        ]}),
    );
    let values = prepare_all(input, GuardianOperation::Classify);
    for value in &values {
        assert!(value.to_string().contains("Classify execution risk."));
        assert!(value.to_string().contains("first turn"));
        assert!(value.to_string().contains("second turn"));
    }
    assert_eq!(values[0]["thinking"]["type"], "adaptive");
    assert_eq!(values[0]["output_config"]["effort"], "high");
    assert_eq!(
        values[1]["generationConfig"]["thinkingConfig"]["thinkingLevel"],
        "HIGH"
    );
    assert_eq!(values[1]["store"], false);
    assert_eq!(values[2]["reasoning_effort"], "high");
    assert_eq!(values[2]["store"], false);
    assert_eq!(values[3]["reasoning"]["context"], "all_turns");
    assert_eq!(values[3]["store"], false);
}
#[test]
fn missing_policy_and_active_execution_dependencies_fail_explicitly() {
    let input = source(json!({"instructions":" \n "}));
    let error =
        guardian::prepare_openai_responses(input, context(GuardianOperation::Review)).unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::MissingMetadata);
    for extra in [
        json!({"tools":[{"type":"function","name":"exec_command"}]}),
        json!({"tool_choice":"required"}),
        json!({"input":[{"type":"additional_tools","role":"developer","tools":[{"type":"function","name":"exec_command"}]}]}),
    ] {
        let input = source(extra);
        assert_eq!(
            guardian::prepare_openai_chat(input, context(GuardianOperation::Review))
                .unwrap_err()
                .kind(),
            TransformErrorKind::Unsupported
        );
    }
    let input =
        source(json!({"tools":[{"type":"function","name":"exec_command"}],"tool_choice":"none"}));
    assert!(guardian::prepare_openai_chat(input, context(GuardianOperation::Review)).is_ok());
}
#[test]
fn execution_metadata_is_evidence_and_readable_reasoning_does_not_leak_ciphertext() {
    let input = source(json!({"input":[
        {"type":"message","role":"assistant","content":[{"type":"output_text","text":"ran check"}], "internal_chat_message_metadata_passthrough":{"turn_id":"turn-7","tool_calls_complete":true,"executed_tool_calls":[{"name":"exec_command","arguments":{"cmd":"echo hi"},"tool_result_sources":[{"type":"tool_result","id":"result-7"}]}]}},
        {"type":"reasoning","summary":[{"type":"summary_text","text":"readable rationale"}],"encrypted_content":"DO-NOT-PROMPT-CIPHERTEXT"},
        {"type":"function_call","name":"exec_command","arguments":"{}","call_id":"call-8","encrypted_function_args":[]}
    ]}));
    let prepared =
        guardian::prepare_openai_responses(input, context(GuardianOperation::Review)).unwrap();
    assert!(
        prepared
            .report
            .diagnostics
            .iter()
            .any(|v| v.field == "guardian.reasoning.encrypted_content")
    );
    let value = encoded(prepared.value).to_string();
    for expected in [
        "turn-7",
        "tool_calls_complete",
        "result-7",
        "echo hi",
        "readable rationale",
        "call-8",
    ] {
        assert!(value.contains(expected));
    }
    assert!(!value.contains("DO-NOT-PROMPT-CIPHERTEXT"));
}
#[test]
fn media_evidence_references_ordered_attachments_without_duplicating_bytes() {
    const PNG: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";
    let input = source(json!({"input":[
        {"type":"message","role":"user","content":[{"type":"input_image","image_url":format!("data:image/png;base64,{PNG}"),"detail":"auto"}]},
        {"type":"image_generation_call","status":"completed","result":PNG}
    ]}));
    for value in prepare_all(input, GuardianOperation::Review) {
        let raw = value.to_string();
        assert!(raw.contains("attachment:0"));
        assert!(raw.contains("attachment:1"));
        assert_eq!(
            raw.matches(PNG).count(),
            2,
            "one copy per native attachment; no base64 in quoted evidence"
        );
    }
}
#[test]
fn response_controls_use_native_fields_and_incompatible_schemas_fail() {
    let input = source(
        json!({"store":true,"service_tier":"priority","prompt_cache_key":"cache-key","reasoning":{"effort":"model_defined","summary":"concise","context":"current_turn"},"include":["reasoning.encrypted_content"],"text":{"verbosity":"high"}}),
    );
    let value = encoded(
        guardian::prepare_openai_responses(input, context(GuardianOperation::Review))
            .unwrap()
            .value,
    );
    assert_eq!(value["store"], true);
    assert_eq!(value["service_tier"], "priority");
    assert_eq!(value["prompt_cache_key"], "cache-key");
    assert!(value["reasoning"].get("effort").is_none());
    assert_eq!(value["reasoning"]["summary"], "concise");
    assert_eq!(value["reasoning"]["context"], "current_turn");
    assert_eq!(value["text"]["verbosity"], "high");
    assert_eq!(value["text"]["format"]["strict"], false);
    assert_eq!(value["include"], json!(["reasoning.encrypted_content"]));
    for schema in [
        json!(false),
        json!({"type":"string"}),
        json!({"type":"object","properties":{"decision":{"type":"string"}},"additionalProperties":false}),
    ] {
        let input = source(
            json!({"text":{"format":{"type":"json_schema","name":"bad","schema":schema,"strict":false}}}),
        );
        assert!(
            guardian::prepare_openai_responses(input, context(GuardianOperation::Review)).is_err()
        );
    }
    let input = source(
        json!({"text":{"format":{"type":"json_schema","name":"permissive","schema":true,"strict":false}}}),
    );
    assert!(guardian::prepare_openai_chat(input, context(GuardianOperation::Review)).is_ok());
}
#[test]
fn review_parser_matches_optional_native_fields_without_invented_fallbacks() {
    for text in [
        r#"{"outcome":"deny"}"#,
        r#"{"outcome":"deny","rationale":"because"}"#,
        r#"{"outcome":"allow","risk_level":"low","future":7}"#,
    ] {
        let response = serde_json::from_value(json!({"id":"c","choices":[{"finish_reason":"stop","index":0,"logprobs":null,"message":{"content":text,"refusal":null,"role":"assistant"}}],"created":1,"model":"m","object":"chat.completion"})).unwrap();
        let guardian::GuardianExtraction::Review(result) =
            guardian::extract_chat(response, GuardianOperation::Review).unwrap()
        else {
            unreachable!()
        };
        assert!(result.user_authorization.is_none());
    }
}

#[test]
fn equivalent_service_tiers_are_mapped_and_unsupported_tiers_fail() {
    let mut escaped = context(GuardianOperation::Review);
    escaped.target_model = "models/m %".into();
    let prepared = guardian::prepare_gemini(source(json!({})), escaped)
        .unwrap()
        .value;
    let guardian::GuardianDialectRequest::Gemini(request) = prepared.request() else {
        unreachable!()
    };
    assert_eq!(request.path, "/v1beta/models/m%20%25:generateContent");

    let claude = encoded(
        guardian::prepare_claude(
            source(json!({"service_tier":"default"})),
            context(GuardianOperation::Review),
        )
        .unwrap()
        .value,
    );
    assert_eq!(claude["service_tier"], "standard_only");
    for (source_tier, native_tier) in [
        ("auto", "unspecified"),
        ("default", "standard"),
        ("flex", "flex"),
        ("priority", "priority"),
    ] {
        let gemini = encoded(
            guardian::prepare_gemini(
                source(json!({"service_tier":source_tier})),
                context(GuardianOperation::Review),
            )
            .unwrap()
            .value,
        );
        assert_eq!(gemini["serviceTier"], native_tier);
    }
    assert!(
        guardian::prepare_claude(
            source(json!({"service_tier":"priority"})),
            context(GuardianOperation::Review)
        )
        .is_err()
    );
}
