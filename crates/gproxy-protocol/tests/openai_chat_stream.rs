use gproxy_protocol::openai::chat::GenerateContentRequestBody;
use gproxy_protocol::openai::chat::stream::{
    ChatCompletionChunk, ChunkObject, Delta, DeltaRole, DeltaToolCallType,
};

#[test]
fn chat_stream_real_chunk_sequence_preserves_partial_text_and_tool_calls() {
    let chunks = [
        serde_json::json!({"id":"chatcmpl-123","object":"chat.completion.chunk","created":1694268190,"model":"gpt-4o-mini","system_fingerprint":"fp","choices":[{"index":0,"delta":{"role":"assistant","content":""},"logprobs":null,"finish_reason":null}]}),
        serde_json::json!({"id":"chatcmpl-123","object":"chat.completion.chunk","created":1694268190,"model":"gpt-4o-mini","choices":[{"index":0,"delta":{"content":"Hello"},"logprobs":null,"finish_reason":null}]}),
        serde_json::json!({"id":"chatcmpl-123","object":"chat.completion.chunk","created":1694268190,"model":"gpt-4o-mini","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"lookup","arguments":"{\"q\":"}}]},"logprobs":null,"finish_reason":null}],"future_chunk":true}),
        serde_json::json!({"id":"chatcmpl-123","object":"chat.completion.chunk","created":1694268190,"model":"gpt-4o-mini","choices":[{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"arguments":"\"x\"}"}}]},"logprobs":null,"finish_reason":"tool_calls"}]}),
        serde_json::json!({"id":"chatcmpl-123","object":"chat.completion.chunk","created":1694268190,"model":"gpt-4o-mini","choices":[],"usage":{"prompt_tokens":5,"completion_tokens":3,"total_tokens":8}}),
    ];
    for value in &chunks {
        let parsed: ChatCompletionChunk = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(parsed).unwrap(), *value);
    }
    let first: ChatCompletionChunk = serde_json::from_value(chunks[0].clone()).unwrap();
    assert!(matches!(first.object, ChunkObject::ChatCompletionChunk));
    assert!(matches!(
        first.choices[0].delta.role,
        Some(Some(DeltaRole::Assistant))
    ));
    assert_eq!(first.choices[0].delta.content, Some(Some("".to_owned())));
    let tool: ChatCompletionChunk = serde_json::from_value(chunks[2].clone()).unwrap();
    let call = &tool.choices[0]
        .delta
        .tool_calls
        .as_ref()
        .unwrap()
        .as_ref()
        .unwrap()[0];
    assert_eq!(call.index, 0);
    assert!(matches!(
        call.type_,
        Some(Some(DeltaToolCallType::Function))
    ));
    assert_eq!(
        call.function
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap()
            .name
            .as_ref()
            .unwrap()
            .as_deref(),
        Some("lookup")
    );
    assert!(tool.rest.contains_key("future_chunk"));
    let final_chunk: ChatCompletionChunk = serde_json::from_value(chunks[4].clone()).unwrap();
    assert!(final_chunk.choices.is_empty());
    assert_eq!(final_chunk.usage.unwrap().unwrap().total_tokens, 8);
}

#[test]
fn chat_stream_request_keeps_stream_controls_in_normal_request_body() {
    let value = serde_json::json!({
        "messages":[{"role":"user","content":"hello"}],"model":"gpt-4o-mini",
        "stream":true,"stream_options":{"include_usage":true,"include_obfuscation":false},"future":1
    });
    let parsed: GenerateContentRequestBody = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(parsed.stream, Some(Some(true)));
    assert!(parsed.stream_options.is_some());
    assert!(parsed.rest.contains_key("future"));
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
}

#[test]
fn chat_stream_nullable_fields_and_builder_are_typed() {
    let value = serde_json::json!({
        "id":"x","object":"chat.completion.chunk","created":1,"model":"m",
        "choices":[{"index":0,"delta":{"content":null,"refusal":"no","audio":{"data":"AA=="}},"logprobs":null,"finish_reason":null}],"usage":null
    });
    let parsed: ChatCompletionChunk = serde_json::from_value(value.clone()).unwrap();
    assert_eq!(parsed.choices[0].delta.content, Some(None));
    assert_eq!(parsed.choices[0].finish_reason, Some(None));
    assert_eq!(parsed.usage, Some(None));
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    assert!(serde_json::from_value::<ChatCompletionChunk>(serde_json::json!({"id":"x","object":"chat.completion","created":1,"model":"m","choices":[]})).is_err());
    let delta = Delta::builder().build();
    assert_eq!(serde_json::to_value(delta).unwrap(), serde_json::json!({}));
}

#[test]
fn sdk_optional_fields_keep_missing_null_and_partial_values() {
    let value = serde_json::json!({
        "id":"c","object":"chat.completion.chunk","created":1,"model":"m",
        "choices":[{"index":0,"delta":{"role":"developer","content":"","refusal":"r","function_call":{"arguments":"{","name":"f"},"tool_calls":[{"index":0,"id":"t","type":"function","function":{"name":"f","arguments":"{"}}]},"finish_reason":"stop","logprobs":{"content":[{"token":"x","logprob":-1.0,"top_logprobs":[{"token":"y","logprob":-2.0,"bytes":null}]}],"refusal":null}}],
        "usage":{"prompt_tokens":1,"completion_tokens":2,"total_tokens":3,"prompt_tokens_details":{"audio_tokens":0,"cache_write_tokens":0,"cached_tokens":0,"image_tokens":0,"text_tokens":1},"completion_tokens_details":{"accepted_prediction_tokens":0,"rejected_prediction_tokens":0,"audio_tokens":0,"reasoning_tokens":0,"text_tokens":2}},
        "moderation":{"input":{"type":"error","code":"x","message":"x"},"output":{"type":"moderation_results","model":"m","results":[{"categories":{},"category_applied_input_types":{},"category_scores":{},"flagged":false,"model":"m","type":"moderation_result"}]}},"obfuscation":"x","service_tier":"priority","system_fingerprint":"f"
    });
    let parsed: ChatCompletionChunk = serde_json::from_value(value.clone()).unwrap();
    assert!(
        format!("{parsed:?}")
            .split("rest: ")
            .skip(1)
            .all(|rest| rest.starts_with("{}"))
    );
    assert_eq!(serde_json::to_value(parsed).unwrap(), value);
    for path in [
        "/moderation",
        "/obfuscation",
        "/service_tier",
        "/system_fingerprint",
        "/usage",
        "/choices/0/logprobs",
        "/choices/0/finish_reason",
        "/choices/0/delta/role",
        "/choices/0/delta/content",
        "/choices/0/delta/refusal",
        "/choices/0/delta/function_call",
        "/choices/0/delta/function_call/name",
        "/choices/0/delta/function_call/arguments",
        "/choices/0/delta/tool_calls",
        "/choices/0/delta/tool_calls/0/id",
        "/choices/0/delta/tool_calls/0/type",
        "/choices/0/delta/tool_calls/0/function",
        "/choices/0/delta/tool_calls/0/function/name",
        "/choices/0/delta/tool_calls/0/function/arguments",
        "/choices/0/logprobs/content",
        "/choices/0/logprobs/refusal",
        "/usage/completion_tokens_details",
        "/usage/prompt_tokens_details",
        "/usage/prompt_tokens_details/image_tokens",
        "/usage/completion_tokens_details/text_tokens",
    ] {
        let (parent, key) = path.rsplit_once('/').unwrap();
        let mut missing = value.clone();
        missing
            .pointer_mut(parent)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .remove(key);
        assert_eq!(
            serde_json::to_value(
                serde_json::from_value::<ChatCompletionChunk>(missing.clone()).unwrap()
            )
            .unwrap(),
            missing,
            "missing {path}"
        );
        let mut null = value.clone();
        *null.pointer_mut(path).unwrap() = serde_json::Value::Null;
        assert_eq!(
            serde_json::to_value(
                serde_json::from_value::<ChatCompletionChunk>(null.clone()).unwrap()
            )
            .unwrap(),
            null,
            "null {path}"
        );
    }
}

#[test]
fn sdk_roles_function_only_fragments_and_required_fields_are_exact() {
    use gproxy_protocol::openai::chat::stream::{ChunkLogprobs, DeltaToolCall, StreamChoice};
    for role in ["developer", "system", "user", "assistant", "tool"] {
        let value = serde_json::json!({"role":role});
        assert_eq!(
            serde_json::to_value(serde_json::from_value::<Delta>(value.clone()).unwrap()).unwrap(),
            value
        );
    }
    assert!(serde_json::from_value::<Delta>(serde_json::json!({"role":"function"})).is_err());
    assert!(
        serde_json::from_value::<DeltaToolCall>(serde_json::json!({"index":0,"type":"custom"}))
            .is_err()
    );
    assert!(
        serde_json::from_value::<DeltaToolCall>(serde_json::json!({"function":{"arguments":"x"}}))
            .is_err()
    );
    assert!(serde_json::from_value::<StreamChoice>(serde_json::json!({"delta":{}})).is_err());
    for value in [
        serde_json::json!({}),
        serde_json::json!({"content":null}),
        serde_json::json!({"content":[{"token":"a","logprob":0.0,"top_logprobs":[]}],"refusal":null}),
    ] {
        assert_eq!(
            serde_json::to_value(serde_json::from_value::<ChunkLogprobs>(value.clone()).unwrap())
                .unwrap(),
            value
        );
    }
    assert!(serde_json::from_str::<ChatCompletionChunk>("[DONE]").is_err());
    let delta: Delta = serde_json::from_value(
        serde_json::json!({"audio":{"data":"extension"},"custom":{"input":"extension"}}),
    )
    .unwrap();
    assert_eq!(delta.rest.len(), 2);
}

#[test]
fn stream_http_response_keeps_raw_bytes_separate_from_typed_chunks() {
    use gproxy_protocol::openai::chat::stream::{StreamChoice, StreamRequest, StreamResponse};
    use gproxy_protocol::{WireRequest, WireResponse, connection::ByteStream};
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
    let request: StreamRequest = WireRequest {
        method: http::Method::POST,
        path: "/v1/chat/completions".into(),
        query: None,
        headers: http::HeaderMap::new(),
        body: GenerateContentRequestBody::builder(vec![], "m".into())
            .stream(Some(true))
            .build(),
    };
    assert_eq!(request.method, http::Method::POST);
    assert_eq!(request.path, "/v1/chat/completions");
    assert!(request.query.is_none());
    assert!(request.headers.is_empty());
    assert_eq!(
        serde_json::to_value(request.body).unwrap(),
        serde_json::json!({"messages":[],"model":"m","stream":true})
    );
    let raw: ByteStream = Box::pin(Empty);
    let mut headers = http::HeaderMap::new();
    headers.insert("content-type", "text/event-stream".parse().unwrap());
    let response: StreamResponse = WireResponse {
        status: http::StatusCode::OK,
        headers: headers.clone(),
        body: raw,
    };
    assert_eq!(response.status, http::StatusCode::OK);
    assert_eq!(response.headers, headers);
    let chunk = ChatCompletionChunk::builder(
        "c".into(),
        vec![StreamChoice::builder(0, Delta::builder().build()).build()],
        1,
        "m".into(),
        ChunkObject::ChatCompletionChunk,
    )
    .build();
    assert_eq!(
        serde_json::to_value(chunk).unwrap(),
        serde_json::json!({"id":"c","choices":[{"index":0,"delta":{}}],"created":1,"model":"m","object":"chat.completion.chunk"})
    );
}
