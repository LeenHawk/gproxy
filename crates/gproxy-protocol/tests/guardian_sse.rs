use gproxy_protocol::{
    HttpBody, WireResponse,
    adapt::guardian::{
        GuardianClassifyInvocation, GuardianNativeResponse, GuardianReviewInvocation,
        GuardianStreamContext, GuardianStreamInvocation, GuardianStreamLimits,
    },
    codec::{self, CodecLimits, SseDecoder, SseFrame},
    transform::{
        generate::{
            chat_responses::{ChatUsageSupplement, ResponsesResponseContext},
            claude_responses::{ClaudeResponseContext, ResponsesUsageFacts},
            gemini_responses::{GeminiResponseContext, GeminiUsageFacts},
            stream::responses::ResponsesStreamCollector,
        },
        guardian::{GuardianClassifyResult, GuardianReviewResult},
        identity::{IdNamespace, IdentityFlow, IdentityRole},
    },
    wire::openai::responses::{self as r, stream::StreamEvent},
};
use http::{HeaderMap, HeaderValue, StatusCode};
use serde_json::{Value, json};
use std::{
    future::Future,
    task::{Context, Poll},
};
fn ready<F: Future>(f: F) -> F::Output {
    let mut f = std::pin::pin!(f);
    match f
        .as_mut()
        .poll(&mut Context::from_waker(std::task::Waker::noop()))
    {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("synthetic byte stream must be ready"),
    }
}
fn flow() -> IdentityFlow {
    IdentityFlow::new(IdNamespace::with_bytes([5; 16]))
}
fn codec() -> CodecLimits {
    CodecLimits {
        max_body_bytes: 1024 * 1024,
        max_buffer_bytes: 1024 * 1024,
        max_value_bytes: 1024 * 1024,
        max_line_bytes: 1024 * 1024,
        max_part_bytes: 1024 * 1024,
        max_parts: 1000,
    }
}
fn limits() -> GuardianStreamLimits {
    GuardianStreamLimits {
        codec: codec(),
        stream: Default::default(),
    }
}
fn request() -> r::GenerateContentRequestBody {
    serde_json::from_value(json!({"model":"guardian-client","instructions":"policy","parallel_tool_calls":false,"tool_choice":"auto"})).unwrap()
}
fn response<T>(body: T) -> WireResponse<T> {
    let mut headers = HeaderMap::new();
    headers.insert("x-request-id", HeaderValue::from_static("upstream-id"));
    headers.insert("content-length", HeaderValue::from_static("999"));
    WireResponse {
        status: StatusCode::OK,
        headers,
        body,
    }
}
fn native_reply(
    which: usize,
    first: &str,
    last: &str,
) -> (GuardianNativeResponse, GuardianStreamContext) {
    let text = format!("{first}{last}");
    match which {
        0 => {
            let body=serde_json::from_value(json!({"id":"r","created_at":7,"model":"m","object":"response","status":"completed","error":null,"incomplete_details":null,"instructions":null,"metadata":null,"parallel_tool_calls":false,"temperature":null,"tool_choice":"auto","tools":[],"top_p":null,"x-extension":"DROP","output":[{"type":"message","id":"m1","role":"assistant","status":"completed","content":[{"type":"output_text","text":first,"annotations":[],"logprobs":[],"x-extension":"DROP"}]},{"type":"message","id":"m2","role":"assistant","status":"completed","content":[{"type":"output_text","text":last,"annotations":[],"logprobs":[]}]}],"usage":{"input_tokens":3,"output_tokens":2,"total_tokens":5,"input_tokens_details":{"cache_write_tokens":0,"cached_tokens":0},"output_tokens_details":{"reasoning_tokens":0}}})).unwrap();
            (
                GuardianNativeResponse::Responses(response(body)),
                GuardianStreamContext::Responses,
            )
        }
        1 => {
            let body=serde_json::from_value(json!({"id":"r","created":7,"model":"m","object":"chat.completion","choices":[{"index":0,"finish_reason":"stop","logprobs":null,"message":{"role":"assistant","content":text,"refusal":null}}],"usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5,"prompt_tokens_details":{"cache_write_tokens":0,"cached_tokens":0},"completion_tokens_details":{"reasoning_tokens":0}}})).unwrap();
            (
                GuardianNativeResponse::Chat(response(body)),
                GuardianStreamContext::Chat(ResponsesResponseContext {
                    request: request(),
                    effective_parallel_tool_calls: false,
                    effective_tool_choice: serde_json::from_value(json!("auto")).unwrap(),
                    usage: ChatUsageSupplement::default(),
                    effective_prompt_cache_options: None,
                }),
            )
        }
        2 => {
            let body=serde_json::from_value(json!({"id":"msg","type":"message","model":"m","role":"assistant","content":[{"type":"text","text":first},{"type":"text","text":last}],"stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":3,"output_tokens":2,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens_details":{"thinking_tokens":0}}})).unwrap();
            (
                GuardianNativeResponse::Claude(response(body)),
                GuardianStreamContext::Claude(ClaudeResponseContext {
                    request: request(),
                    effective_parallel_tool_calls: false,
                    effective_tool_choice: serde_json::from_value(json!("auto")).unwrap(),
                    usage: ResponsesUsageFacts::default(),
                    created_at: 7,
                    effective_prompt_cache_options: None,
                }),
            )
        }
        _ => {
            let body=serde_json::from_value(json!({"responseId":"r","modelVersion":"m","candidates":[{"index":0,"content":{"role":"model","parts":[{"text":first},{"text":last}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":3,"candidatesTokenCount":2,"totalTokenCount":5,"cachedContentTokenCount":0}})).unwrap();
            (
                GuardianNativeResponse::Gemini(response(body)),
                GuardianStreamContext::Gemini(GeminiResponseContext {
                    request: request(),
                    effective_parallel_tool_calls: false,
                    effective_tool_choice: serde_json::from_value(json!("auto")).unwrap(),
                    usage: GeminiUsageFacts {
                        cache_write_tokens: Some(0),
                        cached_tokens: None,
                    },
                    created_at: 7,
                    effective_prompt_cache_options: None,
                }),
            )
        }
    }
}
fn events(value: GuardianStreamInvocation) -> Vec<StreamEvent> {
    let GuardianStreamInvocation::Success { response, .. } = value else {
        panic!("expected SSE success")
    };
    assert_eq!(response.headers["content-type"], "text/event-stream");
    assert_eq!(response.headers["x-request-id"], "upstream-id");
    assert!(!response.headers.contains_key("content-length"));
    let bytes = ready(codec::read_http_body(response.body, codec())).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("DROP"));
    let mut decoder = SseDecoder::new(codec());
    let frames = decoder.push(&bytes).unwrap();
    assert!(decoder.finish().unwrap().is_empty());
    frames
        .into_iter()
        .map(|frame| {
            let SseFrame::Event(event) = frame else {
                panic!("Responses must finish with native terminal event")
            };
            let value: Value = serde_json::from_str(&event.data).unwrap();
            assert_eq!(event.event.as_deref(), value["type"].as_str());
            serde_json::from_value(value).unwrap()
        })
        .collect()
}
#[test]
fn classify_all_four_backends_emit_complete_first_label_and_real_usage() {
    for which in 0..4 {
        let (native, context) = native_reply(which, " h", "igh\n");
        let result = GuardianClassifyInvocation::Success {
            result: GuardianClassifyResult::High,
            native,
        }
        .into_stream(context, &mut flow(), limits())
        .unwrap();
        let events = events(result);
        let deltas: Vec<_> = events
            .iter()
            .filter_map(|e| {
                if let StreamEvent::OutputTextDelta(v) = e {
                    Some(v.delta.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(deltas, vec!["high"], "backend {which}");
        let mut collector = ResponsesStreamCollector::new(Default::default());
        for e in events {
            collector.push(e).unwrap();
        }
        let r = collector.finish().unwrap().value;
        assert_eq!(r.created_at, 7);
        assert_eq!(r.usage.unwrap().unwrap().total_tokens, 5);
    }
}
#[test]
fn review_all_four_backends_keep_complete_json_text_and_native_lifecycle() {
    let expected: GuardianReviewResult =
        serde_json::from_value(json!({"outcome":"allow"})).unwrap();
    for which in 0..4 {
        let (native, context) = native_reply(which, "{\"outcome\":", "\"allow\"}");
        let value = GuardianReviewInvocation::Success {
            result: expected.clone(),
            native,
        }
        .into_stream(context, &mut flow(), limits())
        .unwrap();
        let events = events(value);
        let text: String = events
            .iter()
            .filter_map(|e| {
                if let StreamEvent::OutputTextDelta(v) = e {
                    Some(v.delta.as_str())
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(text, "{\"outcome\":\"allow\"}");
        assert!(matches!(events.first(), Some(StreamEvent::Created(_))));
        assert!(matches!(events.last(), Some(StreamEvent::Completed(_))));
    }
}
#[test]
fn contradictory_labels_context_and_encoding_failure_do_not_publish_identities() {
    let (native, context) = native_reply(1, "low", "");
    let mut ids = flow();
    assert!(
        GuardianClassifyInvocation::Success {
            result: GuardianClassifyResult::High,
            native
        }
        .into_stream(context, &mut ids, limits())
        .is_err()
    );
    assert!(
        ids.lookup_logical(
            IdentityRole::Response,
            &gproxy_protocol::Dialect::OpenAiChat,
            0
        )
        .is_none()
    );
    let (native, _) = native_reply(1, "high", "");
    assert!(
        GuardianClassifyInvocation::Success {
            result: GuardianClassifyResult::High,
            native
        }
        .into_stream(GuardianStreamContext::Responses, &mut ids, limits())
        .is_err()
    );
    let (native, context) = native_reply(1, "high", "");
    let mut small = limits();
    small.codec.max_body_bytes = 1000;
    let result = GuardianClassifyInvocation::Success {
        result: GuardianClassifyResult::High,
        native,
    }
    .into_stream(context, &mut ids, small);
    assert!(result.is_err());
    assert!(
        ids.lookup_logical(
            IdentityRole::Response,
            &gproxy_protocol::Dialect::OpenAiChat,
            0
        )
        .is_none()
    );
}
#[test]
fn rejected_upstream_body_stays_unread_and_unchanged() {
    let response = WireResponse {
        status: StatusCode::TOO_MANY_REQUESTS,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(bytes::Bytes::from_static(b"raw-error")),
    };
    let result = GuardianClassifyInvocation::Rejected(response)
        .into_stream(GuardianStreamContext::Responses, &mut flow(), limits())
        .unwrap();
    let GuardianStreamInvocation::Rejected(response) = result else {
        panic!("expected rejection")
    };
    assert_eq!(response.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(
        ready(codec::read_http_body(response.body, codec())).unwrap(),
        "raw-error"
    );
}

struct Host {
    calls: std::sync::atomic::AtomicUsize,
    body: Vec<u8>,
}
impl gproxy_protocol::capability::Upstream for Host {
    type Target = ();
    fn send<'a>(
        &'a self,
        _: &'a (),
        request: gproxy_protocol::WireRequest<HttpBody>,
    ) -> gproxy_protocol::capability::CapabilityFuture<
        'a,
        Result<WireResponse<HttpBody>, gproxy_protocol::capability::CapabilityError>,
    > {
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let HttpBody::Bytes(body) = request.body else {
            panic!("expected JSON request")
        };
        let body: Value = serde_json::from_slice(&body).unwrap();
        if request.path.starts_with("/v1beta/") {
            assert_eq!(request.path, "/v1beta/models/m:generateContent");
        } else {
            assert_eq!(body["model"], "m");
            assert_eq!(body["stream"], false);
        }
        Box::pin(async move {
            let mut headers = HeaderMap::new();
            headers.insert("x-request-id", HeaderValue::from_static("upstream-id"));
            Ok(WireResponse {
                status: StatusCode::OK,
                headers,
                body: HttpBody::Bytes(bytes::Bytes::copy_from_slice(&self.body)),
            })
        })
    }
    fn connect<'a>(
        &'a self,
        _: &'a (),
        _: gproxy_protocol::WireRequest<()>,
    ) -> gproxy_protocol::capability::CapabilityFuture<
        'a,
        Result<
            gproxy_protocol::capability::UpstreamConnection,
            gproxy_protocol::capability::CapabilityError,
        >,
    > {
        panic!("unexpected connect")
    }
    fn limits(&self) -> gproxy_protocol::capability::CapabilityLimits {
        gproxy_protocol::capability::CapabilityLimits {
            operation_total: std::time::Duration::from_secs(30),
            stream_idle: std::time::Duration::from_secs(5),
            read_bytes: 1024 * 1024,
            write_bytes: 1024 * 1024,
            ws_frame_bytes: 1024 * 1024,
        }
    }
}
fn prepared() -> gproxy_protocol::transform::guardian::GuardianPreparedRequest {
    let input=serde_json::from_value(json!({"model":"guardian","instructions":"Classify the request","input":[{"type":"message","role":"user","content":[{"type":"input_text","text":"read a file"}]}],"tools":null,"tool_choice":"auto","parallel_tool_calls":false,"reasoning":null,"store":false,"stream":true,"include":[]})).unwrap();
    gproxy_protocol::transform::guardian::prepare_openai_responses(
        input,
        gproxy_protocol::transform::guardian::GuardianRequestContext {
            target_model: "m".into(),
            max_tokens: 64,
            operation: gproxy_protocol::transform::guardian::GuardianOperation::Classify,
        },
    )
    .unwrap()
    .value
}
#[test]
fn composed_sse_invocation_preflights_context_and_limits_before_host_send() {
    let (native, _) = native_reply(0, "h", "igh");
    let GuardianNativeResponse::Responses(reply) = native else {
        unreachable!()
    };
    let host = Host {
        calls: Default::default(),
        body: serde_json::to_vec(&reply.body).unwrap(),
    };
    let (_, wrong_context) = native_reply(2, "high", "");
    let invocation_limits = gproxy_protocol::adapt::guardian::GuardianLimits {
        codec: codec(),
        max_request_bytes: 1024 * 1024,
    };
    assert!(
        ready(gproxy_protocol::adapt::guardian::classify_sse(
            &host,
            &(),
            prepared(),
            wrong_context,
            &mut flow(),
            invocation_limits,
            limits()
        ))
        .is_err()
    );
    let mut tiny = limits();
    tiny.stream.max_events = 1;
    assert!(
        ready(gproxy_protocol::adapt::guardian::classify_sse(
            &host,
            &(),
            prepared(),
            GuardianStreamContext::Responses,
            &mut flow(),
            invocation_limits,
            tiny
        ))
        .is_err()
    );
    assert_eq!(host.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    let reply = ready(gproxy_protocol::adapt::guardian::classify_sse(
        &host,
        &(),
        prepared(),
        GuardianStreamContext::Responses,
        &mut flow(),
        invocation_limits,
        limits(),
    ))
    .unwrap();
    assert_eq!(host.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    let events = events(reply);
    let text: Vec<_> = events
        .iter()
        .filter_map(|v| {
            if let StreamEvent::OutputTextDelta(v) = v {
                Some(v.delta.as_str())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(text, vec!["high"]);
}

fn prepared_backend(which: usize) -> gproxy_protocol::transform::guardian::GuardianPreparedRequest {
    use gproxy_protocol::transform::guardian as t;
    let input = prepared().source().clone();
    let context = t::GuardianRequestContext {
        target_model: "m".into(),
        max_tokens: 64,
        operation: t::GuardianOperation::Classify,
    };
    (match which {
        0 => t::prepare_openai_responses,
        1 => t::prepare_openai_chat,
        2 => t::prepare_claude,
        _ => t::prepare_gemini,
    })(input, context)
    .unwrap()
    .value
}
fn native_bytes(native: GuardianNativeResponse) -> Vec<u8> {
    match native {
        GuardianNativeResponse::Responses(v) => serde_json::to_vec(&v.body),
        GuardianNativeResponse::Chat(v) => serde_json::to_vec(&v.body),
        GuardianNativeResponse::Claude(v) => serde_json::to_vec(&v.body),
        GuardianNativeResponse::Gemini(v) => serde_json::to_vec(&v.body),
    }
    .unwrap()
}
fn bind(
    context: &mut GuardianStreamContext,
    prepared: &gproxy_protocol::transform::guardian::GuardianPreparedRequest,
) {
    let request = prepared.response_request().unwrap();
    match context {
        GuardianStreamContext::Responses => {}
        GuardianStreamContext::Chat(c) => c.request = request,
        GuardianStreamContext::Claude(c) => c.request = request,
        GuardianStreamContext::Gemini(c) => c.request = request,
    }
}
#[test]
fn composed_all_four_sse_backends_use_source_owned_templates_and_real_usage() {
    for which in 0..4 {
        let prepared = prepared_backend(which);
        let (native, mut context) = native_reply(which, " h", "igh\n");
        bind(&mut context, &prepared);
        let host = Host {
            calls: Default::default(),
            body: native_bytes(native),
        };
        let output = ready(gproxy_protocol::adapt::guardian::classify_sse(
            &host,
            &(),
            prepared,
            context,
            &mut flow(),
            gproxy_protocol::adapt::guardian::GuardianLimits {
                codec: codec(),
                max_request_bytes: 1024 * 1024,
            },
            limits(),
        ))
        .unwrap();
        let mut collector = ResponsesStreamCollector::new(Default::default());
        for event in events(output) {
            collector.push(event).unwrap();
        }
        let response = collector.finish().unwrap().value;
        assert_eq!(response.usage.unwrap().unwrap().total_tokens, 5);
        assert_eq!(host.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
#[test]
fn same_backend_foreign_source_controls_fail_before_send() {
    for field in ["instructions", "store", "reasoning", "user", "model"] {
        let prepared = prepared_backend(1);
        let (native, mut context) = native_reply(1, "high", "");
        bind(&mut context, &prepared);
        let GuardianStreamContext::Chat(c) = &mut context else {
            unreachable!()
        };
        match field {
            "instructions" => c.request.instructions = Some(Some("other policy".into())),
            "store" => c.request.store = Some(Some(true)),
            "reasoning" => {
                c.request.reasoning = Some(Some(
                    r::ReasoningConfig::builder()
                        .effort(Some(r::ReasoningEffort::High))
                        .build(),
                ))
            }
            "user" => c.request.user = Some("other-user".into()),
            _ => c.request.model = Some("other-source".into()),
        }
        let host = Host {
            calls: Default::default(),
            body: native_bytes(native),
        };
        let error = ready(gproxy_protocol::adapt::guardian::classify_sse(
            &host,
            &(),
            prepared,
            context,
            &mut flow(),
            gproxy_protocol::adapt::guardian::GuardianLimits {
                codec: codec(),
                max_request_bytes: 1024 * 1024,
            },
            limits(),
        ))
        .unwrap_err();
        assert_eq!(error.context(), "guardian.stream.source");
        assert_eq!(host.calls.load(std::sync::atomic::Ordering::SeqCst), 0);
    }
}
#[test]
fn invalid_generated_label_retains_native_usage_without_retry() {
    let (native, _) = native_reply(0, "not a", " label");
    let host = Host {
        calls: Default::default(),
        body: native_bytes(native),
    };
    let output = ready(gproxy_protocol::adapt::guardian::classify_sse(
        &host,
        &(),
        prepared(),
        GuardianStreamContext::Responses,
        &mut flow(),
        gproxy_protocol::adapt::guardian::GuardianLimits {
            codec: codec(),
            max_request_bytes: 1024 * 1024,
        },
        limits(),
    ))
    .unwrap();
    let GuardianStreamInvocation::Invalid { error, native } = output else {
        panic!("invalid semantic result must retain provenance")
    };
    assert_eq!(
        error.kind(),
        gproxy_protocol::transform::TransformErrorKind::InvalidResult
    );
    let GuardianNativeResponse::Responses(native) = *native else {
        unreachable!()
    };
    assert_eq!(native.body.usage.unwrap().unwrap().total_tokens, 5);
    assert_eq!(native.body.id, "r");
    assert_eq!(host.calls.load(std::sync::atomic::Ordering::SeqCst), 1);
}
