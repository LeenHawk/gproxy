use gproxy_protocol::{
    HttpBody, WireRequest, WireResponse,
    adapt::memory::{self, MemoryDialect, MemoryLimits},
    capability::{
        CapabilityError, CapabilityFuture, CapabilityLimits, Upstream, UpstreamConnection,
    },
    codec::CodecLimits,
    connection::{Bytes, HeaderMap, Method, StatusCode},
    openai::memory::RawMemory,
    transform::TransformErrorKind,
};
use serde_json::json;

use std::{
    future::Future,
    sync::Mutex,
    task::{Context, Poll},
    time::Duration,
};

struct Host {
    sent: Mutex<Vec<WireRequest<HttpBody>>>,
    responses: Mutex<Vec<WireResponse<HttpBody>>>,
    pending: bool,
}

impl Upstream for Host {
    type Target = ();

    fn send<'a>(
        &'a self,
        _: &'a (),
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            self.sent.lock().unwrap().push(request);
            if self.pending {
                std::future::pending().await
            }
            Ok(self.responses.lock().unwrap().remove(0))
        })
    }

    fn connect<'a>(
        &'a self,
        _: &'a (),
        _: WireRequest<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        panic!("unexpected WS call")
    }

    fn limits(&self) -> CapabilityLimits {
        CapabilityLimits {
            operation_total: Duration::from_secs(30),
            stream_idle: Duration::from_secs(5),
            read_bytes: 64 * 1024,
            write_bytes: 64 * 1024,
            ws_frame_bytes: 1024,
        }
    }
}

fn limits() -> MemoryLimits {
    MemoryLimits {
        max_bytes: 64 * 1024,
        codec: CodecLimits {
            max_buffer_bytes: 64 * 1024,
            max_value_bytes: 64 * 1024,
            max_body_bytes: 64 * 1024,
            max_line_bytes: 64 * 1024,
            max_part_bytes: 64 * 1024,
            max_parts: 8,
        },
    }
}

fn trace(id: &str) -> RawMemory {
    serde_json::from_value(json!({
        "id": id,
        "metadata": {"source_path": format!("/tmp/{id}")},
        "items": [{"opaque": true}, id]
    }))
    .unwrap()
}

fn result_text(id: &str) -> String {
    json!({"trace_summary": format!("trace-{id}"), "memory_summary": format!("memory-{id}")})
        .to_string()
}

fn ready<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixture should be immediately ready"),
    }
}

fn response(dialect: MemoryDialect, id: &str) -> WireResponse<HttpBody> {
    let text = result_text(id);
    let value = match dialect {
        MemoryDialect::Claude => json!({
            "type":"message", "id":"m", "content":[{"type":"text","text":text}],
            "model":"claude", "role":"assistant", "stop_reason":"end_turn",
            "stop_sequence":null, "usage":{"input_tokens":1,"output_tokens":1}
        }),
        MemoryDialect::Gemini => json!({
            "candidates":[{"content":{"role":"model","parts":[{"text":text}]},"finishReason":"STOP"}]
        }),
        MemoryDialect::OpenAiChat => json!({
            "id":"c", "choices":[{"finish_reason":"stop","index":0,"logprobs":null,
            "message":{"content":text,"refusal":null,"role":"assistant"}}],
            "created":1,"model":"chat","object":"chat.completion"
        }),
        MemoryDialect::OpenAiResponses => json!({
            "id":"r", "status":"completed", "created_at":1, "error":null, "incomplete_details":null,
            "instructions":null, "metadata":{}, "model":"responses", "object":"response",
            "output":[], "output_text":text, "parallel_tool_calls":true,
            "temperature":null, "tool_choice":"auto", "tools":[], "top_p":null
        }),
    };
    WireResponse {
        status: StatusCode::OK,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&value).unwrap())),
    }
}

fn run(dialect: MemoryDialect) {
    let host = Host {
        sent: Mutex::default(),
        responses: Mutex::new(vec![response(dialect, "a"), response(dialect, "b")]),
        pending: false,
    };
    let result = ready(memory::summarize(
        &host,
        &(),
        dialect,
        vec![trace("a"), trace("b")],
        "selected-model",
        128,
        limits(),
    ))
    .unwrap();
    assert_eq!(result[0].raw_memory, "trace-a");
    assert_eq!(result[1].memory_summary, "memory-b");
    let sent = host.sent.lock().unwrap();
    assert_eq!(sent.len(), 2);
    assert!(sent.iter().all(|request| request.method == Method::POST));
    for request in sent.iter() {
        let HttpBody::Bytes(bytes) = &request.body else {
            panic!("memory request must be buffered JSON")
        };
        let body: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        match dialect {
            MemoryDialect::OpenAiResponses => {
                assert_eq!(request.path, "/v1/responses");
                assert_eq!(body["model"], "selected-model");
                assert_eq!(body["text"]["format"]["type"], "json_schema");
                assert_eq!(body["text"]["format"]["strict"], true);
            }
            MemoryDialect::OpenAiChat => {
                assert_eq!(request.path, "/v1/chat/completions");
                assert_eq!(body["model"], "selected-model");
                assert_eq!(body["response_format"]["type"], "json_schema");
                assert_eq!(body["response_format"]["json_schema"]["strict"], true);
            }
            MemoryDialect::Claude => {
                assert_eq!(request.path, "/v1/messages");
                assert_eq!(body["model"], "selected-model");
                assert_eq!(body["max_tokens"], 128);
                assert_eq!(body["output_format"]["type"], "json_schema");
            }
            MemoryDialect::Gemini => {
                assert_eq!(
                    request.path,
                    "/v1beta/models/selected-model:generateContent"
                );
                assert_eq!(
                    body["generationConfig"]["responseMimeType"],
                    "application/json"
                );
                assert_eq!(
                    body["generationConfig"]["responseJsonSchema"]["required"],
                    json!(["trace_summary", "memory_summary"])
                );
            }
        }
    }
}

#[test]
fn all_four_dialects_call_once_per_trace_and_preserve_order() {
    for dialect in [
        MemoryDialect::OpenAiResponses,
        MemoryDialect::OpenAiChat,
        MemoryDialect::Claude,
        MemoryDialect::Gemini,
    ] {
        run(dialect);
    }
}

#[test]
fn rejected_trace_stops_without_retry_or_partial_success() {
    let dialect = MemoryDialect::OpenAiChat;
    let host = Host {
        sent: Mutex::default(),
        responses: Mutex::new(vec![WireResponse {
            status: StatusCode::TOO_MANY_REQUESTS,
            headers: HeaderMap::new(),
            body: HttpBody::Bytes(Bytes::from_static(b"rate limited")),
        }]),
        pending: false,
    };
    let error = ready(memory::summarize(
        &host,
        &(),
        dialect,
        vec![trace("a"), trace("b")],
        "selected-model",
        128,
        limits(),
    ))
    .unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::InvalidResult);
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}

#[test]
fn trace_byte_cap_rejects_before_send() {
    let dialect = MemoryDialect::Gemini;
    let host = Host {
        sent: Mutex::default(),
        responses: Mutex::new(vec![response(dialect, "a")]),
        pending: false,
    };
    let mut bounded = limits();
    bounded.max_bytes = 1;
    let error = ready(memory::summarize(
        &host,
        &(),
        dialect,
        vec![trace("a")],
        "selected-model",
        128,
        bounded,
    ))
    .unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::Limit);
    assert!(host.sent.lock().unwrap().is_empty());
}

#[test]
fn dropping_an_inflight_call_cancels_before_the_next_trace() {
    let host = Host {
        sent: Mutex::default(),
        responses: Mutex::default(),
        pending: true,
    };
    let future = memory::summarize(
        &host,
        &(),
        MemoryDialect::OpenAiResponses,
        vec![trace("a"), trace("b")],
        "selected-model",
        128,
        limits(),
    );
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(std::task::Waker::noop());
    assert!(matches!(future.as_mut().poll(&mut cx), Poll::Pending));
    drop(future);
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}

#[test]
fn all_preflight_checks_happen_before_first_call_and_empty_batch_has_no_side_effect() {
    let host = Host {
        sent: Mutex::default(),
        responses: Mutex::default(),
        pending: false,
    };
    let mut bad = trace("b");
    bad.metadata.source_path.clear();
    assert!(
        ready(memory::summarize(
            &host,
            &(),
            MemoryDialect::Claude,
            vec![trace("a"), bad],
            "model",
            128,
            limits()
        ))
        .is_err()
    );
    assert!(host.sent.lock().unwrap().is_empty());
    assert!(
        ready(memory::summarize(
            &host,
            &(),
            MemoryDialect::Claude,
            vec![trace("a"), trace("a")],
            "model",
            128,
            limits()
        ))
        .is_err()
    );
    assert!(host.sent.lock().unwrap().is_empty());
    assert!(
        ready(memory::summarize(
            &host,
            &(),
            MemoryDialect::Claude,
            Vec::new(),
            "model",
            128,
            limits()
        ))
        .unwrap()
        .is_empty()
    );
    assert!(host.sent.lock().unwrap().is_empty());
}

#[test]
fn reasoning_is_typed_and_unrepresentable_effort_is_not_silently_lowered() {
    use gproxy_protocol::transform::memory as mapping;
    let reasoning: gproxy_protocol::openai::guardian::Reasoning =
        serde_json::from_value(json!({"effort":"high","foreign":{"summary":"detailed"}})).unwrap();
    let mapping::MemoryDialectRequest::Claude(request) =
        mapping::build_claude("trace".into(), "claude".into(), 128, Some(&reasoning)).unwrap()
    else {
        panic!()
    };
    assert_eq!(
        request.body.output_config.unwrap().effort,
        Some(gproxy_protocol::claude::count_tokens::Effort::High)
    );
    let reasoning: gproxy_protocol::openai::guardian::Reasoning =
        serde_json::from_value(json!({"effort":"none"})).unwrap();
    let mapping::MemoryDialectRequest::Gemini(request) = mapping::build_gemini(
        "trace".into(),
        "models/gemini".into(),
        128,
        Some(&reasoning),
    )
    .unwrap() else {
        panic!()
    };
    assert_eq!(request.path, "/v1beta/models/gemini:generateContent");
    assert_eq!(
        request
            .body
            .generation_config
            .unwrap()
            .thinking_config
            .unwrap()
            .thinking_budget,
        Some(0)
    );
    let reasoning: gproxy_protocol::openai::guardian::Reasoning =
        serde_json::from_value(json!({"effort":"xhigh"})).unwrap();
    assert!(mapping::build_gemini("trace".into(), "model".into(), 128, Some(&reasoning)).is_ok());
    assert!(mapping::build_gemini("trace".into(), "model?key=x".into(), 128, None).is_err());
}

#[test]
fn truncated_refused_and_failed_native_responses_cannot_be_summaries() {
    use gproxy_protocol::transform::memory as mapping;
    let valid = result_text("a");
    let chat=serde_json::from_value(json!({"id":"c","created":1,"model":"m","object":"chat.completion","choices":[{"index":0,"finish_reason":"length","logprobs":null,"message":{"role":"assistant","content":valid,"refusal":null}}]})).unwrap();
    assert!(mapping::chat_text(chat).is_err());
    let claude=serde_json::from_value(json!({"type":"message","id":"c","model":"m","role":"assistant","stop_reason":"refusal","content":[{"type":"text","text":result_text("a")}],"usage":{"input_tokens":1,"output_tokens":1}})).unwrap();
    assert!(mapping::claude_text(claude).is_err());
    let gemini=serde_json::from_value(json!({"candidates":[{"finishReason":"MAX_TOKENS","content":{"parts":[{"text":result_text("a")}]}}]})).unwrap();
    assert!(mapping::gemini_text(gemini).is_err());
    let response=serde_json::from_value(json!({"id":"r","status":"failed","created_at":1,"model":"m","object":"response","output_text":result_text("a"),"output":[],"parallel_tool_calls":true,"temperature":null,"tool_choice":"auto","tools":[],"top_p":null,"error":null,"incomplete_details":null,"instructions":null,"metadata":null})).unwrap();
    assert!(mapping::responses_text(response).is_err());
    for invalid in [
        "{\"trace_summary\":\"a\",\"trace_summary\":\"b\",\"memory_summary\":\"c\"}",
        "{\"trace_summary\":\"a\",\"memory_summary\":1}",
        "{\"trace_summary\":\"a\",\"memory_summary\":\"b\",\"other\":true}",
    ] {
        assert!(mapping::parse_output(invalid, limits().codec).is_err());
    }
    assert_eq!(
        mapping::parse_output(
            "{\"trace_summary\":\"\",\"memory_summary\":\"\"}",
            limits().codec
        )
        .unwrap(),
        (String::new(), String::new())
    );
}

#[test]
fn later_http_failure_retains_trace_identity_progress_and_unread_body() {
    let body = HttpBody::Stream(Box::pin(futures_util::stream::poll_fn(
        |_| -> Poll<Option<Result<Bytes, gproxy_protocol::connection::TransportError>>> {
            panic!("error body consumed")
        },
    )));
    let host = Host {
        sent: Mutex::default(),
        responses: Mutex::new(vec![
            response(MemoryDialect::OpenAiChat, "a"),
            WireResponse {
                status: StatusCode::TOO_MANY_REQUESTS,
                headers: HeaderMap::new(),
                body,
            },
        ]),
        pending: false,
    };
    let error = ready(memory::summarize(
        &host,
        &(),
        MemoryDialect::OpenAiChat,
        vec![trace("a"), trace("b"), trace("c")],
        "model",
        128,
        limits(),
    ))
    .unwrap_err();
    assert_eq!(error.attempted_calls, 2);
    assert_eq!(error.completed_traces, 1);
    assert_eq!(error.trace_id.as_deref(), Some("b"));
    assert!(matches!(error.failure, memory::MemoryFailure::Rejected(_)));
    assert_eq!(host.sent.lock().unwrap().len(), 2);
}

#[test]
fn formal_trace_items_keep_arbitrary_json_while_metadata_extensions_are_dropped() {
    let trace:RawMemory=serde_json::from_value(json!({"id":"trace","metadata":{"source_path":"C:\\work\\trace.jsonl","foreign":"no"},"items":[{"foreign":{"keep":true}}],"foreign":"no"})).unwrap();
    let payload =
        gproxy_protocol::transform::memory::trace_payload(&trace, limits().codec).unwrap();
    let value: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(value["source_path"], "C:\\work\\trace.jsonl");
    assert_eq!(value["items"][0]["foreign"]["keep"], true);
    assert!(value.get("foreign").is_none());
}
