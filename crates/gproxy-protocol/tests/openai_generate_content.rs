use gproxy_protocol::openai::responses::generate::*;
use http::{HeaderMap, HeaderValue, Method};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

// Create a model response.md:15-5760, all 31 request fields. The nested
// input/tool definitions were checked against the approved CountTokens shapes.
fn full_request() -> Value {
    json!({
        "background":true,
        "context_management":[{"type":"compaction","compact_threshold":1000}],
        "conversation":{"id":"conv"},
        "include":["web_search_call.action.sources","code_interpreter_call.outputs","computer_call_output.output.image_url","file_search_call.results","message.input_image.image_url","message.output_text.logprobs","reasoning.encrypted_content"],
        "input":[{"role":"user","content":"hello"}],
        "instructions":"Answer briefly",
        "max_output_tokens":1000,
        "max_tool_calls":3,
        "metadata":{"purpose":"schema-test"},
        "model":"future-model-id",
        "moderation":{"model":"omni-moderation-latest","policy":{"input":{"mode":"score"},"output":{"mode":"block"}}},
        "parallel_tool_calls":true,
        "previous_response_id":"resp",
        "prompt":{"id":"pmpt","variables":{
            "string":"hello",
            "text":{"type":"input_text","text":"hello","prompt_cache_breakpoint":{"mode":"explicit"}},
            "image":{"type":"input_image","detail":"original","file_id":null,"image_url":"https://example.com/a.png"},
            "file":{"type":"input_file","detail":"high","file_id":"file","filename":"a.pdf"}
        },"version":"2"},
        "prompt_cache_key":"cache",
        "prompt_cache_options":{"mode":"explicit","ttl":"30m"},
        "prompt_cache_retention":"24h",
        "reasoning":{"effort":"max","summary":"detailed","context":"all_turns"},
        "safety_identifier":"user-hash",
        "service_tier":"fast",
        "store":false,
        "stream":false,
        "stream_options":{"include_obfuscation":true},
        "temperature":0.7,
        "text":{"format":{"type":"json_schema","name":"answer","schema":{"type":"object"},"strict":null},"verbosity":"low"},
        "tool_choice":{"type":"function","name":"lookup"},
        "tools":[{"type":"function","name":"lookup","parameters":{},"strict":null}],
        "top_logprobs":4,
        "top_p":0.9,
        "truncation":"auto",
        "user":"legacy-user"
    })
}

#[test]
fn all_request_fields_are_typed_and_alias_keeps_five_http_elements() {
    let wire = full_request();
    assert_eq!(wire.as_object().unwrap().len(), 31);
    let body: GenerateContentRequestBody = serde_json::from_value(wire.clone()).unwrap();
    assert!(body.rest.is_empty());
    assert_eq!(body.model.as_deref(), Some("future-model-id"));
    assert_eq!(
        body.metadata.as_ref().unwrap().as_ref().unwrap()["purpose"],
        "schema-test"
    );
    let context = &body.context_management.as_ref().unwrap().as_ref().unwrap()[0];
    assert!(context.rest.is_empty());
    assert_eq!(context.compact_threshold, Some(Some(1000)));
    let moderation = body.moderation.as_ref().unwrap().as_ref().unwrap();
    assert!(moderation.rest.is_empty());
    let policy = moderation.policy.as_ref().unwrap().as_ref().unwrap();
    assert!(policy.rest.is_empty());
    assert!(
        policy
            .input
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .rest
            .is_empty()
    );
    assert!(
        policy
            .output
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .rest
            .is_empty()
    );
    let prompt = body.prompt.as_ref().unwrap().as_ref().unwrap();
    assert!(prompt.rest.is_empty());
    let variables = prompt.variables.as_ref().unwrap().as_ref().unwrap();
    assert!(matches!(
        &variables["string"],
        PromptVariableValue::String(_)
    ));
    match &variables["text"] {
        PromptVariableValue::Text(v) => {
            assert!(v.rest.is_empty());
            assert!(v.prompt_cache_breakpoint.as_ref().unwrap().rest.is_empty());
        }
        _ => panic!(),
    }
    match &variables["image"] {
        PromptVariableValue::Image(v) => assert!(v.rest.is_empty()),
        _ => panic!(),
    }
    match &variables["file"] {
        PromptVariableValue::File(v) => assert!(v.rest.is_empty()),
        _ => panic!(),
    }
    assert!(body.prompt_cache_options.as_ref().unwrap().rest.is_empty());
    assert!(
        body.stream_options
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .rest
            .is_empty()
    );
    let mut headers = HeaderMap::new();
    headers.insert("x-test", HeaderValue::from_static("request"));
    let request: GenerateContentRequest = GenerateContentRequest {
        method: Method::POST,
        path: "/v1/responses".into(),
        query: Some("x=1&x=2".into()),
        headers,
        body,
    };
    assert_eq!(request.method, Method::POST);
    assert_eq!(request.path, "/v1/responses");
    assert_eq!(request.query.as_deref(), Some("x=1&x=2"));
    assert_eq!(request.headers["x-test"], "request");
    assert_eq!(serde_json::to_value(request.body).unwrap(), wire);
}

#[test]
fn all_top_level_fields_preserve_their_documented_optionality() {
    // The request has no schema-required top-level fields, including model.
    let empty: GenerateContentRequestBody = serde_json::from_value(json!({})).unwrap();
    assert!(empty.rest.is_empty());
    assert_eq!(serde_json::to_value(empty).unwrap(), json!({}));
    let full = full_request();
    let nullable = [
        "background",
        "context_management",
        "conversation",
        "include",
        "instructions",
        "max_output_tokens",
        "max_tool_calls",
        "metadata",
        "moderation",
        "parallel_tool_calls",
        "previous_response_id",
        "prompt",
        "prompt_cache_key",
        "prompt_cache_retention",
        "reasoning",
        "safety_identifier",
        "service_tier",
        "store",
        "stream",
        "stream_options",
        "temperature",
        "top_logprobs",
        "top_p",
        "truncation",
    ];
    let ordinary = [
        "input",
        "model",
        "prompt_cache_options",
        "text",
        "tool_choice",
        "tools",
        "user",
    ];
    assert_eq!(nullable.len() + ordinary.len(), 31);
    for field in nullable.iter().chain(ordinary.iter()) {
        let mut wire = full.clone();
        wire.as_object_mut().unwrap().remove(*field);
        let value: GenerateContentRequestBody = serde_json::from_value(wire.clone()).unwrap();
        assert!(value.rest.is_empty());
        assert_eq!(serde_json::to_value(value).unwrap(), wire, "absent {field}");
    }
    for field in nullable {
        let mut wire = full.clone();
        wire[field] = Value::Null;
        let value: GenerateContentRequestBody = serde_json::from_value(wire.clone()).unwrap();
        assert!(value.rest.is_empty());
        assert_eq!(
            serde_json::to_value(value).unwrap(),
            wire,
            "nullable {field}"
        );
    }
    for field in ordinary {
        let mut wire = full.clone();
        wire[field] = Value::Null;
        assert!(
            serde_json::from_value::<GenerateContentRequestBody>(wire).is_err(),
            "{field} does not permit null"
        );
    }
}

fn three_states<T: DeserializeOwned + Serialize>(base: Value, field: &str, value: Value) {
    for value in [None, Some(Value::Null), Some(value)] {
        let mut wire = base.clone();
        if let Some(v) = value {
            wire[field] = v;
        }
        let parsed: T = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(
            serde_json::to_value(parsed).unwrap(),
            wire,
            "{}.{field}",
            std::any::type_name::<T>()
        );
    }
}

#[test]
fn nested_nullable_fields_and_ordinary_options_are_distinct() {
    three_states::<ContextManagement>(
        json!({"type":"compaction"}),
        "compact_threshold",
        json!(1000),
    );
    three_states::<ResponseModeration>(json!({"model":"moderation"}), "policy", json!({}));
    for field in ["input", "output"] {
        three_states::<ModerationPolicy>(json!({}), field, json!({"mode":"score"}));
    }
    three_states::<ResponsePrompt>(json!({"id":"p"}), "variables", json!({"x":"y"}));
    three_states::<ResponsePrompt>(json!({"id":"p"}), "version", json!("1"));
    let options: PromptCacheOptions = serde_json::from_value(json!({})).unwrap();
    assert!(options.rest.is_empty());
    assert_eq!(serde_json::to_value(options).unwrap(), json!({}));
    for field in ["mode", "ttl"] {
        let mut wire = json!({});
        wire[field] = Value::Null;
        assert!(serde_json::from_value::<PromptCacheOptions>(wire).is_err());
    }
    let options: StreamOptions = serde_json::from_value(json!({})).unwrap();
    assert!(options.rest.is_empty());
    assert_eq!(serde_json::to_value(options).unwrap(), json!({}));
    assert!(serde_json::from_value::<StreamOptions>(json!({"include_obfuscation":null})).is_err());
}

#[test]
fn prompt_variables_and_metadata_reject_values_outside_their_native_unions() {
    for invalid in [
        json!(null),
        json!(true),
        json!(42),
        json!([]),
        json!({}),
        json!({"type":"unknown","text":"x"}),
        json!({"type":"input_image","image_url":"https://x"}),
    ] {
        assert!(
            serde_json::from_value::<ResponsePrompt>(json!({"id":"p","variables":{"x":invalid}}))
                .is_err()
        );
    }
    for invalid in [json!(null), json!(true), json!(42), json!([]), json!({})] {
        assert!(
            serde_json::from_value::<GenerateContentRequestBody>(json!({"metadata":{"k":invalid}}))
                .is_err()
        );
    }
    let wire = json!({"type":"input_image","detail":"auto","image_url":"https://x","text":"future-extension"});
    let variable: PromptVariableValue = serde_json::from_value(wire.clone()).unwrap();
    match &variable {
        PromptVariableValue::Image(v) => assert_eq!(v.rest["text"], "future-extension"),
        _ => panic!(),
    }
    assert_eq!(serde_json::to_value(variable).unwrap(), wire);
}

fn closed<T: DeserializeOwned + Serialize>(values: &[&str]) {
    for value in values {
        let parsed: T = serde_json::from_value(json!(value)).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), json!(value));
    }
    assert!(serde_json::from_value::<T>(json!("unknown-value")).is_err());
}

#[test]
fn closed_enums_and_open_string_fields_follow_the_source() {
    closed::<ResponseIncludable>(&[
        "web_search_call.action.sources",
        "code_interpreter_call.outputs",
        "computer_call_output.output.image_url",
        "file_search_call.results",
        "message.input_image.image_url",
        "message.output_text.logprobs",
        "reasoning.encrypted_content",
    ]);
    closed::<ModerationMode>(&["score", "block"]);
    closed::<PromptCachingMode>(&["implicit", "explicit"]);
    closed::<PromptCacheTtl>(&["30m"]);
    closed::<PromptCacheRetention>(&["in_memory", "24h"]);
    closed::<ServiceTier>(&["auto", "default", "flex", "scale", "priority", "fast"]);
    closed::<GenerateTruncation>(&["auto", "disabled"]);
    assert!(serde_json::from_value::<PromptCacheTtl>(json!("24h")).is_err());
    // context_management.type is string, not a guessed compaction-only enum.
    let context: ContextManagement =
        serde_json::from_value(json!({"type":"future-strategy"})).unwrap();
    assert!(context.rest.is_empty());
    for model in ["gpt-5.6-sol", "future-model"] {
        let parsed: GenerateContentRequestBody =
            serde_json::from_value(json!({"model":model})).unwrap();
        assert_eq!(parsed.model.as_deref(), Some(model));
    }
}

#[test]
fn nested_required_fields_cannot_be_omitted_or_null() {
    macro_rules! required {
        ($ty:ty,$wire:expr,$field:literal) => {{
            let wire = $wire;
            let _: $ty = serde_json::from_value(wire.clone()).unwrap();
            let mut missing = wire.clone();
            missing.as_object_mut().unwrap().remove($field);
            assert!(serde_json::from_value::<$ty>(missing).is_err());
            let mut null = wire;
            null[$field] = Value::Null;
            assert!(serde_json::from_value::<$ty>(null).is_err());
        }};
    }
    required!(ContextManagement, json!({"type":"compaction"}), "type");
    required!(ResponseModeration, json!({"model":"moderation"}), "model");
    required!(ModerationPolicyMode, json!({"mode":"block"}), "mode");
    required!(ResponsePrompt, json!({"id":"p"}), "id");
}

#[test]
fn only_snake_case_names_are_recognized_and_extensions_round_trip() {
    let wire =
        json!({"maxOutputTokens":100,"promptCacheOptions":{"mode":"explicit"},"future":true});
    let parsed: GenerateContentRequestBody = serde_json::from_value(wire.clone()).unwrap();
    assert!(parsed.max_output_tokens.is_none());
    assert!(parsed.prompt_cache_options.is_none());
    assert_eq!(parsed.rest.len(), 3);
    assert_eq!(serde_json::to_value(parsed).unwrap(), wire);
    let wire = json!({"includeObfuscation":true,"future":1});
    let parsed: StreamOptions = serde_json::from_value(wire.clone()).unwrap();
    assert!(parsed.include_obfuscation.is_none());
    assert_eq!(serde_json::to_value(parsed).unwrap(), wire);
    let wire = json!({"type":"compaction","compactThreshold":5});
    let parsed: ContextManagement = serde_json::from_value(wire.clone()).unwrap();
    assert!(parsed.compact_threshold.is_none());
    assert_eq!(serde_json::to_value(parsed).unwrap(), wire);
}

#[test]
fn typed_builders_emit_native_tags_and_preserve_explicit_null() {
    assert_eq!(
        serde_json::to_value(GenerateContentRequestBody::builder().build()).unwrap(),
        json!({})
    );
    let request = GenerateContentRequestBody::builder()
        .model("future-model")
        .background(None)
        .prompt_cache_options(
            PromptCacheOptions::builder()
                .mode(PromptCachingMode::Explicit)
                .ttl(PromptCacheTtl::ThirtyMinutes)
                .build(),
        )
        .prompt(Some(
            ResponsePrompt::builder("p".into()).version(None).build(),
        ))
        .moderation(Some(
            ResponseModeration::builder("moderation".into())
                .policy(Some(
                    ModerationPolicy::builder()
                        .input(Some(
                            ModerationPolicyMode::builder(ModerationMode::Block).build(),
                        ))
                        .build(),
                ))
                .build(),
        ))
        .stream_options(Some(
            StreamOptions::builder().include_obfuscation(false).build(),
        ))
        .build();
    assert_eq!(
        serde_json::to_value(request).unwrap(),
        json!({"model":"future-model","background":null,"prompt_cache_options":{"mode":"explicit","ttl":"30m"},"prompt":{"id":"p","version":null},"moderation":{"model":"moderation","policy":{"input":{"mode":"block"}}},"stream_options":{"include_obfuscation":false}})
    );
}
