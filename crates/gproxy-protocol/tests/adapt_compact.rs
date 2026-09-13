use gproxy_protocol::{
    HttpBody, WireRequest, WireResponse,
    adapt::compact::{self, CompactLimits},
    capability::{
        CapabilityError, CapabilityFuture, CapabilityLimits, Upstream, UpstreamConnection,
    },
    codec::CodecLimits,
    connection::{Bytes, HeaderMap, StatusCode},
    openai::compact::ClientCompactRequestBody as CompactRequestBody,
    transform::{TransformErrorKind, compact::CompactDialectRequest},
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
        panic!("unexpected WS")
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

fn limits() -> CompactLimits {
    CompactLimits {
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

fn input() -> CompactRequestBody {
    serde_json::from_value(json!({
        "model":"source-model","parallel_tool_calls":true,
        "input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"old"}]},{"type":"message","role":"assistant","content":[{"type":"output_text","text":"tail"}]}]
    }))
    .unwrap()
}

fn response(dialect: CompactDialectRequest) -> WireResponse<HttpBody> {
    let summary = "preserved decisions and unresolved work";
    let body = match dialect {
        CompactDialectRequest::Responses => json!({
            "id":"r","status":"completed","created_at":1,"error":null,"incomplete_details":null,"instructions":null,"metadata":{},"model":"target","object":"response","output":[{"id":"m","content":[{"annotations":[],"logprobs":[],"text":summary,"type":"output_text"}],"role":"assistant","status":"completed","type":"message"}],"parallel_tool_calls":true,"temperature":null,"tool_choice":"auto","tools":[],"top_p":null,"usage":{"input_tokens":2,"input_tokens_details":{"cache_write_tokens":0,"cached_tokens":0},"output_tokens":5,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":7}
        }),
        CompactDialectRequest::Chat => {
            json!({"id":"c","choices":[{"finish_reason":"stop","index":0,"logprobs":null,"message":{"content":summary,"refusal":null,"role":"assistant"}}],"created":1,"model":"target","object":"chat.completion","usage":{"prompt_tokens":2,"completion_tokens":5,"total_tokens":7}})
        }
        CompactDialectRequest::Claude => {
            json!({"type":"message","id":"c","content":[{"type":"text","text":summary}],"model":"target","role":"assistant","stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":2,"output_tokens":5}})
        }
        CompactDialectRequest::Gemini => {
            json!({"candidates":[{"content":{"role":"model","parts":[{"text":summary}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":2,"candidatesTokenCount":5,"totalTokenCount":7}})
        }
    };
    WireResponse {
        status: StatusCode::OK,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&body).unwrap())),
    }
}

fn ready<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixture pending"),
    }
}

#[test]
fn all_four_generation_dialects_return_usable_replacement_history() {
    for dialect in [
        CompactDialectRequest::Responses,
        CompactDialectRequest::Chat,
        CompactDialectRequest::Claude,
        CompactDialectRequest::Gemini,
    ] {
        let host = Host {
            sent: Mutex::default(),
            responses: Mutex::new(vec![response(dialect)]),
            pending: false,
        };
        let result = ready(compact::compact(
            &host,
            &(),
            input(),
            dialect,
            "target-model",
            128,
            1,
            limits(),
        ))
        .unwrap();
        let wire = serde_json::to_value(result).unwrap();
        assert_eq!(wire["output"].as_array().unwrap().len(), 2);
        assert_eq!(
            wire["output"][0]["content"][0]["text"],
            "preserved decisions and unresolved work"
        );
        assert_eq!(wire["output"][1]["content"][0]["text"], "tail");
        assert!(wire.get("usage").is_none());
        assert!(wire.get("id").is_none());
        assert_eq!(host.sent.lock().unwrap().len(), 1);
    }
}

#[test]
fn rejection_does_not_retry_or_return_partial_history() {
    let host = Host {
        sent: Mutex::default(),
        responses: Mutex::new(vec![WireResponse {
            status: StatusCode::BAD_GATEWAY,
            headers: HeaderMap::new(),
            body: HttpBody::Bytes(Bytes::from_static(b"failed")),
        }]),
        pending: false,
    };
    let error = ready(compact::compact(
        &host,
        &(),
        input(),
        CompactDialectRequest::Chat,
        "target",
        128,
        1,
        limits(),
    ))
    .unwrap_err();
    assert_eq!(error.kind(), TransformErrorKind::InvalidResult);
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}

#[test]
fn dropping_inflight_compaction_cancels_without_next_call() {
    let host = Host {
        sent: Mutex::default(),
        responses: Mutex::default(),
        pending: true,
    };
    let future = compact::compact(
        &host,
        &(),
        input(),
        CompactDialectRequest::Claude,
        "target",
        128,
        1,
        limits(),
    );
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(std::task::Waker::noop());
    assert!(matches!(future.as_mut().poll(&mut cx), Poll::Pending));
    drop(future);
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}

#[test]
fn only_prefix_is_sent_and_complete_tail_with_native_items_is_preserved() {
    let input:CompactRequestBody=serde_json::from_value(json!({"model":"source","parallel_tool_calls":true,"instructions":"original rules","input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"PREFIX_ONLY"}],"foreign":1},{"type":"message","role":"user","content":[{"type":"input_text","text":"TAIL_ONLY"}],"foreign":2},{"type":"compaction","encrypted_content":"native-tail-secret"}],"foreign":3})).unwrap();
    let host = Host {
        sent: Mutex::default(),
        responses: Mutex::new(vec![response(CompactDialectRequest::Responses)]),
        pending: false,
    };
    let result = ready(compact::compact(
        &host,
        &(),
        input,
        CompactDialectRequest::Responses,
        "target",
        128,
        1,
        limits(),
    ))
    .unwrap();
    let wire = serde_json::to_value(result).unwrap();
    assert_eq!(wire.as_object().unwrap().len(), 1);
    assert_eq!(wire["output"].as_array().unwrap().len(), 3);
    assert_eq!(wire["output"][1]["role"], "user");
    assert_eq!(wire["output"][2]["encrypted_content"], "native-tail-secret");
    assert!(wire["output"][1].get("foreign").is_none());
    let sent = host.sent.lock().unwrap();
    let HttpBody::Bytes(body) = &sent[0].body else {
        panic!()
    };
    let payload = std::str::from_utf8(body).unwrap();
    assert!(payload.contains("PREFIX_ONLY"));
    assert!(!payload.contains("TAIL_ONLY"));
    assert!(!payload.contains("native-tail-secret"));
    assert!(payload.contains("original rules"));
}
#[test]
fn empty_prefix_is_noop_and_split_call_result_or_opaque_prefix_fail_before_send() {
    let host = Host {
        sent: Mutex::default(),
        responses: Mutex::default(),
        pending: false,
    };
    let result = ready(compact::compact(
        &host,
        &(),
        input(),
        CompactDialectRequest::Claude,
        "target",
        128,
        0,
        limits(),
    ))
    .unwrap();
    assert_eq!(result.output.len(), 2);
    assert!(host.sent.lock().unwrap().is_empty());
    let input:CompactRequestBody=serde_json::from_value(json!({"model":"source","parallel_tool_calls":true,"input":[{"type":"function_call","call_id":"call","name":"f","arguments":"{}"},{"type":"function_call_output","call_id":"call","output":"result"}]})).unwrap();
    assert!(
        ready(compact::compact(
            &host,
            &(),
            input,
            CompactDialectRequest::Chat,
            "target",
            128,
            1,
            limits()
        ))
        .is_err()
    );
    assert!(host.sent.lock().unwrap().is_empty());
    let input:CompactRequestBody=serde_json::from_value(json!({"model":"source","parallel_tool_calls":true,"input":[{"type":"compaction","encrypted_content":"opaque"}]})).unwrap();
    assert!(
        ready(compact::compact(
            &host,
            &(),
            input,
            CompactDialectRequest::Chat,
            "target",
            128,
            1,
            limits()
        ))
        .is_err()
    );
    assert!(host.sent.lock().unwrap().is_empty());
}
#[test]
fn truncated_summary_and_preflight_aggregate_overflow_never_return_replacement() {
    let mut invalid = response(CompactDialectRequest::Chat);
    let HttpBody::Bytes(bytes) = invalid.body else {
        panic!()
    };
    let mut body: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    body["choices"][0]["finish_reason"] = json!("length");
    invalid.body = HttpBody::Bytes(Bytes::from(serde_json::to_vec(&body).unwrap()));
    let host = Host {
        sent: Mutex::default(),
        responses: Mutex::new(vec![invalid]),
        pending: false,
    };
    let error = ready(compact::compact(
        &host,
        &(),
        input(),
        CompactDialectRequest::Chat,
        "target",
        128,
        1,
        limits(),
    ))
    .unwrap_err();
    assert_eq!(error.attempted_calls, 1);
    let host = Host {
        sent: Mutex::default(),
        responses: Mutex::default(),
        pending: false,
    };
    let mut bounds = limits();
    bounds.max_bytes = 16;
    assert!(
        ready(compact::compact(
            &host,
            &(),
            input(),
            CompactDialectRequest::Chat,
            "target",
            128,
            1,
            bounds
        ))
        .is_err()
    );
    assert!(host.sent.lock().unwrap().is_empty());
}
#[test]
fn selected_image_is_sent_as_native_image_instead_of_only_json_text() {
    let input:CompactRequestBody=serde_json::from_value(json!({"model":"source","parallel_tool_calls":true,"input":[{"type":"message","role":"user","content":[{"type":"input_image","image_url":"data:image/png;base64,AQI="}]}]})).unwrap();
    for dialect in [
        CompactDialectRequest::Responses,
        CompactDialectRequest::Chat,
        CompactDialectRequest::Claude,
        CompactDialectRequest::Gemini,
    ] {
        let host = Host {
            sent: Mutex::default(),
            responses: Mutex::new(vec![response(dialect)]),
            pending: false,
        };
        ready(compact::compact(
            &host,
            &(),
            input.clone(),
            dialect,
            "target",
            128,
            1,
            limits(),
        ))
        .unwrap();
        let sent = host.sent.lock().unwrap();
        let HttpBody::Bytes(bytes) = &sent[0].body else {
            panic!()
        };
        let wire: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        match dialect {
            CompactDialectRequest::Responses => {
                assert_eq!(wire["input"][1]["content"][0]["type"], "input_image")
            }
            CompactDialectRequest::Chat => {
                assert_eq!(wire["messages"][1]["content"][0]["type"], "image_url")
            }
            CompactDialectRequest::Claude => {
                assert_eq!(wire["messages"][1]["content"][0]["type"], "image")
            }
            CompactDialectRequest::Gemini => assert_eq!(
                wire["contents"][1]["parts"][0]["inlineData"]["mimeType"],
                "image/png"
            ),
        }
    }
}
