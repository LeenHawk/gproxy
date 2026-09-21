use gproxy_protocol::{
    Dialect,
    transform::{
        generate::stream::chat::*,
        identity::{IdNamespace, IdentityFlow, TargetIdPolicy},
    },
    wire::openai::chat::{response as r, stream as s},
};
use serde_json::{Value, json};
fn collector() -> ChatStreamCollector {
    ChatStreamCollector::new(
        IdentityFlow::new(IdNamespace::with_bytes([17; 16])),
        TargetIdPolicy::new(Dialect::OpenAiChat),
    )
}
fn chunk(choices: Value) -> s::ChatCompletionChunk {
    serde_json::from_value(json!({"id":"r","object":"chat.completion.chunk","created":123,"model":"actual","choices":choices})).unwrap()
}
fn terminal(index: i64, reason: &str) -> Value {
    json!({"index":index,"delta":{},"finish_reason":reason})
}
fn response() -> r::GenerateContentResponseBody {
    serde_json::from_value(json!({"id":"r","object":"chat.completion","created":123,"model":"actual","service_tier":"priority","system_fingerprint":"fp","choices":[{"index":0,"message":{"role":"assistant","content":"a","refusal":"r","tool_calls":[{"id":"call-a","type":"function","function":{"name":"f","arguments":"{\"x\":1}"}}]},"finish_reason":"tool_calls","logprobs":{"content":[{"token":"a","bytes":[97],"logprob":-0.1,"top_logprobs":[]}],"refusal":null}},{"index":1,"message":{"role":"assistant","content":"b","refusal":null},"finish_reason":"length","logprobs":null}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15,"prompt_tokens_details":{"cache_write_tokens":2,"cached_tokens":3,"audio_tokens":1},"completion_tokens_details":{"reasoning_tokens":2,"accepted_prediction_tokens":1,"rejected_prediction_tokens":20,"audio_tokens":1}}})).unwrap()
}
#[test]
fn multi_choice_late_usage_done_and_details_roundtrip() {
    let source = response();
    let chunks = synthesize_chat_stream(source.clone(), Default::default())
        .unwrap()
        .value;
    assert!(chunks.last().unwrap().choices.is_empty());
    let mut c = collector();
    for chunk in chunks {
        c.push(chunk).unwrap();
    }
    c.push_done().unwrap();
    assert_eq!(c.finish().unwrap().value, source);
}
#[test]
fn split_arguments_late_ids_and_same_names_have_distinct_identities() {
    let mut c = collector();
    c.push(chunk(json!([{"index":0,"delta":{"role":"assistant","tool_calls":[{"index":0,"function":{"name":"f","arguments":"{\"x\":"}},{"index":1,"function":{"name":"f","arguments":"{}"}}]}}]))).unwrap();
    c.push(chunk(json!([{"index":0,"delta":{"tool_calls":[{"index":0,"id":"late-id","function":{"arguments":"1}"}}]},"finish_reason":"tool_calls"}]))).unwrap();
    c.push_done().unwrap();
    let out = serde_json::to_value(c.finish().unwrap().value).unwrap();
    let calls = &out["choices"][0]["message"]["tool_calls"];
    assert_eq!(calls[0]["id"], "late-id");
    assert_eq!(calls[0]["function"]["arguments"], "{\"x\":1}");
    assert_ne!(calls[0]["id"], calls[1]["id"]);
    let mut c = collector();
    c.push(chunk(json!([{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"name":"f"}},{"index":1,"function":{"name":"f"}}]},"finish_reason":"tool_calls"}]))).unwrap();
    c.push_done().unwrap();
    let out = serde_json::to_value(c.finish().unwrap().value).unwrap();
    let calls = &out["choices"][0]["message"]["tool_calls"];
    assert_ne!(calls[0]["id"], calls[1]["id"]);
}
#[test]
fn finish_is_not_done_and_every_choice_must_finish() {
    let mut c = collector();
    c.push(chunk(json!([terminal(0, "stop")]))).unwrap();
    assert!(c.finish().is_err());
    let mut c = collector();
    c.push(chunk(
        json!([terminal(0,"stop"),{"index":1,"delta":{"content":"partial"}}]),
    ))
    .unwrap();
    assert!(c.push_done().is_err());
    assert!(c.finish().is_err());
    let mut c = collector();
    c.push(chunk(json!([terminal(0, "stop")]))).unwrap();
    assert!(
        c.push(chunk(json!([{"index":0,"delta":{"content":"late"}}])))
            .is_err()
    );
    assert!(c.finish().is_err());
}
#[test]
fn conflicts_and_limits_poison_collector() {
    let mut c = collector();
    c.push(chunk(json!([{"index":0,"delta":{"content":"x"}}])))
        .unwrap();
    let mut n = chunk(json!([terminal(0, "stop")]));
    n.model = "other".into();
    assert!(c.push(n).is_err());
    assert!(c.push_done().is_err());
    let mut c = collector();
    assert!(c.push(chunk(json!([terminal(-1, "stop")]))).is_err());
    for limits in [
        ChatStreamLimits {
            max_bytes: 1,
            ..Default::default()
        },
        ChatStreamLimits {
            max_tool_calls: 1,
            ..Default::default()
        },
    ] {
        let mut c = ChatStreamCollector::with_limits(
            IdentityFlow::new(IdNamespace::with_bytes([1; 16])),
            TargetIdPolicy::new(Dialect::OpenAiChat),
            limits,
        );
        assert!(c.push(chunk(json!([{"index":0,"delta":{"tool_calls":[{"index":0,"function":{"name":"f"}},{"index":1,"function":{"name":"f"}}]}}]))).is_err());
    }
}
#[test]
fn logprobs_accumulate_and_empty_content_survives() {
    let mut c = collector();
    for (text, finish) in [("", None), ("b", Some("stop"))] {
        c.push(chunk(json!([{"index":0,"delta":{"content":text},"finish_reason":finish,"logprobs":{"content":[{"token":text,"bytes":null,"logprob":-1.0,"top_logprobs":[]}]}}]))).unwrap();
    }
    c.push_done().unwrap();
    let out = c.finish().unwrap().value;
    assert_eq!(out.choices[0].message.content.as_deref(), Some("b"));
    let logs = out.choices[0]
        .logprobs
        .as_ref()
        .unwrap()
        .content
        .as_ref()
        .unwrap();
    assert_eq!(logs.len(), 2);
    assert!(logs[0].bytes.is_none());
    let mut c = collector();
    c.push(chunk(
        json!([{"index":0,"delta":{"content":""},"finish_reason":"stop"}]),
    ))
    .unwrap();
    c.push_done().unwrap();
    assert_eq!(
        c.finish().unwrap().value.choices[0]
            .message
            .content
            .as_deref(),
        Some("")
    );
}
#[test]
fn legacy_function_fragments_roundtrip() {
    let mut c = collector();
    c.push(chunk(
        json!([{"index":0,"delta":{"function_call":{"name":"old","arguments":"{\"x\":"}}}]),
    ))
    .unwrap();
    c.push(chunk(json!([{"index":0,"delta":{"function_call":{"arguments":"2}"}},"finish_reason":"function_call"}]))).unwrap();
    c.push_done().unwrap();
    let source = c.finish().unwrap().value;
    assert_eq!(
        source.choices[0]
            .message
            .function_call
            .as_ref()
            .unwrap()
            .arguments,
        "{\"x\":2}"
    );
    let mut c = collector();
    for chunk in synthesize_chat_stream(source.clone(), Default::default())
        .unwrap()
        .value
    {
        c.push(chunk).unwrap();
    }
    c.push_done().unwrap();
    assert_eq!(c.finish().unwrap().value, source);
}
#[test]
fn nested_extensions_never_enter_results() {
    let value = json!({"id":"r","object":"chat.completion.chunk","model":"m","created":1,"x-rest":true,"choices":[{"index":0,"delta":{"content":"ok","x-rest":true},"finish_reason":"stop","x-rest":true}],"usage":{"prompt_tokens":1,"completion_tokens":2,"total_tokens":3,"prompt_tokens_details":{"cached_tokens":0,"x-rest":true},"x-rest":true}});
    let mut c = collector();
    c.push(serde_json::from_value(value).unwrap()).unwrap();
    c.push_done().unwrap();
    let mut out = c.finish().unwrap().value;
    assert!(
        !serde_json::to_value(&out)
            .unwrap()
            .to_string()
            .contains("x-rest")
    );
    out.rest.insert("x-rest".into(), json!(true));
    out.choices[0]
        .message
        .rest
        .insert("x-rest".into(), json!(true));
    let chunks = synthesize_chat_stream(out, Default::default())
        .unwrap()
        .value;
    assert!(
        !serde_json::to_value(chunks)
            .unwrap()
            .to_string()
            .contains("x-rest")
    );
}
#[test]
fn malformed_usage_and_synthesis_limits_are_explicit() {
    let mut c = collector();
    let mut n = chunk(json!([terminal(0, "stop")]));
    n.usage = Some(Some(
        serde_json::from_value(json!({"prompt_tokens":2,"completion_tokens":1,"total_tokens":4}))
            .unwrap(),
    ));
    assert!(c.push(n).is_ok());
    assert!(
        synthesize_chat_stream(
            response(),
            ChatStreamLimits {
                max_events: 1,
                ..Default::default()
            }
        )
        .is_err()
    );
    let mut source = response();
    source.choices[0].message.audio = Some(Some(
        serde_json::from_value(json!({"id":"a","data":"YQ==","expires_at":1,"transcript":"a"}))
            .unwrap(),
    ));
    assert!(synthesize_chat_stream(source, Default::default()).is_err());
}

#[test]
fn terminal_protocol_errors_poison_and_chat_dialect_is_required() {
    let complete:s::ChatCompletionChunk=serde_json::from_value(json!({"id":"r","model":"m","created":1,"object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"ok"},"finish_reason":"stop"}]})).unwrap();
    let mut c = collector();
    c.push(complete.clone()).unwrap();
    c.push_done().unwrap();
    assert!(c.push_done().is_err());
    assert!(c.finish().is_err());
    let mut c = collector();
    c.push(complete.clone()).unwrap();
    c.push_done().unwrap();
    assert!(c.push(complete.clone()).is_err());
    assert!(c.finish().is_err());
    let mut wrong = ChatStreamCollector::new(
        IdentityFlow::new(IdNamespace::with_bytes([99; 16])),
        TargetIdPolicy::new(Dialect::OpenAi),
    );
    assert!(wrong.push(complete).is_err());
}
