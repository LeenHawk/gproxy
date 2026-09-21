use gproxy_protocol::{
    HttpBody, WireRequest, WireResponse,
    adapt::guardian::{self, GuardianLimits},
    capability::{
        CapabilityError, CapabilityFuture, CapabilityLimits, Upstream, UpstreamConnection,
    },
    codec::CodecLimits,
    connection::{Bytes, HeaderMap, StatusCode},
    transform::guardian::{
        self as transform, GuardianOperation, GuardianPreparedRequest, GuardianRequestContext,
    },
};
use serde_json::{Value, json};
use std::{
    future::Future,
    sync::Mutex,
    task::{Context, Poll},
    time::Duration,
};

struct Host {
    sent: Mutex<Vec<WireRequest<HttpBody>>>,
    response_status: StatusCode,
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
            let operation = match &request.body {
                HttpBody::Bytes(bytes) => {
                    let value: Value = serde_json::from_slice(bytes).unwrap();
                    value.to_string().contains("high or low")
                }
                _ => false,
            };
            let path = request.path.clone();
            self.sent.lock().unwrap().push(request);
            if self.pending {
                std::future::pending::<()>().await;
            }
            Ok(WireResponse {
                status: self.response_status,
                headers: HeaderMap::new(),
                body: HttpBody::Bytes(Bytes::from(response_body(operation, &path))),
            })
        })
    }
    fn connect<'a>(
        &'a self,
        _: &'a (),
        _: WireRequest<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        panic!("unexpected connect")
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
fn codec_limits() -> CodecLimits {
    CodecLimits {
        max_buffer_bytes: 64 * 1024,
        max_value_bytes: 64 * 1024,
        max_body_bytes: 64 * 1024,
        max_line_bytes: 64 * 1024,
        max_part_bytes: 64 * 1024,
        max_parts: 8,
    }
}
fn limits() -> GuardianLimits {
    GuardianLimits {
        codec: codec_limits(),
        max_request_bytes: 64 * 1024,
    }
}
fn ready<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut cx) {
        Poll::Ready(value) => value,
        Poll::Pending => panic!("fixture should be ready"),
    }
}

fn source_request() -> gproxy_protocol::openai::guardian::GuardianRequestBody {
    serde_json::from_value(json!({
        "model":"guardian-client", "instructions":"Assess the action.",
        "input":[
            {"type":"message","id":"msg-1","role":"user","content":[{"type":"input_text","text":"hello"}]},
            {"type":"function_call","id":"item-1","name":"exec_command","arguments":"{\"command\":\"echo hi\"}","call_id":"call-1"}
        ],
        "tools":[{"type":"function","name":"exec_command"}], "tool_choice":"none", "parallel_tool_calls":false,
        "reasoning":null, "store":false, "stream":true, "include":[]
    })).unwrap()
}
fn response_body(classify: bool, path: &str) -> Vec<u8> {
    let text = if classify {
        "low".to_string()
    } else {
        json!({"risk_level":"high","user_authorization":"low","outcome":"deny","rationale":"The action is high risk."}).to_string()
    };
    let value = if path == "/v1/messages" {
        json!({"type":"message","id":"m","content":[{"type":"text","text":text}],"model":"m","role":"assistant","stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":1,"output_tokens":1}})
    } else if path == "/v1/chat/completions" {
        json!({"id":"c","choices":[{"finish_reason":"stop","index":0,"logprobs":null,"message":{"content":text,"refusal":null,"role":"assistant"}}],"created":1,"model":"m","object":"chat.completion"})
    } else if path == "/v1/responses" {
        json!({"id":"r","status":"completed","created_at":1,"error":null,"incomplete_details":null,"instructions":null,"metadata":{},"model":"m","object":"response","output":[],"output_text":text,"parallel_tool_calls":true,"temperature":null,"tool_choice":"auto","tools":[],"top_p":null})
    } else {
        json!({"candidates":[{"content":{"role":"model","parts":[{"text":text}]},"finishReason":"STOP"}]})
    };
    serde_json::to_vec(&value).unwrap()
}
fn prepare(dialect: &str, operation: GuardianOperation) -> GuardianPreparedRequest {
    let context = GuardianRequestContext {
        target_model: if dialect == "gemini" {
            "gemini-model".into()
        } else {
            "selected-model".into()
        },
        max_tokens: 128,
        operation,
    };
    match dialect {
        "claude" => {
            transform::prepare_claude(source_request(), context)
                .unwrap()
                .value
        }
        "gemini" => {
            transform::prepare_gemini(source_request(), context)
                .unwrap()
                .value
        }
        "chat" => {
            transform::prepare_openai_chat(source_request(), context)
                .unwrap()
                .value
        }
        _ => {
            transform::prepare_openai_responses(source_request(), context)
                .unwrap()
                .value
        }
    }
}

#[test]
fn both_operations_succeed_for_all_four_target_dialects_and_preserve_declared_history() {
    for dialect in ["claude", "gemini", "chat", "responses"] {
        for operation in [GuardianOperation::Review, GuardianOperation::Classify] {
            let host = Host {
                sent: Mutex::default(),
                response_status: StatusCode::OK,
                pending: false,
            };
            let request = prepare(dialect, operation);
            match operation {
                GuardianOperation::Review => {
                    let guardian::GuardianReviewInvocation::Success { result, .. } =
                        ready(guardian::review(&host, &(), request, limits())).unwrap()
                    else {
                        panic!("unexpected rejection")
                    };
                    assert_eq!(result.outcome, transform::GuardianOutcome::Deny);
                }
                GuardianOperation::Classify => {
                    let guardian::GuardianClassifyInvocation::Success { result, .. } =
                        ready(guardian::classify(&host, &(), request, limits())).unwrap()
                    else {
                        panic!("unexpected rejection")
                    };
                    assert_eq!(result, transform::GuardianClassifyResult::Low);
                }
            }
            let sent = host.sent.lock().unwrap();
            assert_eq!(sent.len(), 1);
            let HttpBody::Bytes(bytes) = &sent[0].body else {
                panic!("typed request must be encoded")
            };
            let body: Value = serde_json::from_slice(bytes).unwrap();
            let payload = body.to_string();
            assert!(payload.contains("call-1"));
            assert!(payload.contains("exec_command"));
            assert!(payload.contains("declared_tools"));
        }
    }
}

#[test]
fn rejected_http_is_returned_without_retry() {
    let host = Host {
        sent: Mutex::default(),
        response_status: StatusCode::TOO_MANY_REQUESTS,
        pending: false,
    };
    let guardian::GuardianReviewInvocation::Rejected(response) = ready(guardian::review(
        &host,
        &(),
        prepare("chat", GuardianOperation::Review),
        limits(),
    ))
    .unwrap() else {
        panic!("expected rejection")
    };
    assert_eq!(response.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}

#[test]
fn prepared_operation_mismatch_fails_before_send() {
    let host = Host {
        sent: Mutex::default(),
        response_status: StatusCode::OK,
        pending: false,
    };
    let error = ready(guardian::review(
        &host,
        &(),
        prepare("chat", GuardianOperation::Classify),
        limits(),
    ))
    .unwrap_err();
    assert_eq!(
        error.kind(),
        gproxy_protocol::transform::TransformErrorKind::Conflict
    );
    assert!(host.sent.lock().unwrap().is_empty());
}

#[test]
fn preflight_cap_rejects_before_side_effect() {
    let host = Host {
        sent: Mutex::default(),
        response_status: StatusCode::OK,
        pending: false,
    };
    let error = ready(guardian::review(
        &host,
        &(),
        prepare("responses", GuardianOperation::Review),
        GuardianLimits {
            codec: codec_limits(),
            max_request_bytes: 1,
        },
    ))
    .unwrap_err();
    assert_eq!(
        error.kind(),
        gproxy_protocol::transform::TransformErrorKind::Limit
    );
    assert!(host.sent.lock().unwrap().is_empty());
}

#[test]
fn dropping_pending_call_cancels_without_a_second_attempt() {
    let host = Host {
        sent: Mutex::default(),
        response_status: StatusCode::OK,
        pending: true,
    };
    let mut future = Box::pin(guardian::classify(
        &host,
        &(),
        prepare("chat", GuardianOperation::Classify),
        limits(),
    ));
    let mut cx = Context::from_waker(std::task::Waker::noop());
    assert!(matches!(future.as_mut().poll(&mut cx), Poll::Pending));
    drop(future);
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}

use gproxy_protocol::capability::{
    PublicationKind, PublicationStatus, PublishedResource, ResourceAccess, ResourceMetadata,
    ResourceRead, ResourceReference,
};
const PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";
struct Resources {
    reads: Mutex<Vec<(String, ResourceReference)>>,
    bytes: Bytes,
    mime: String,
    length: Option<u64>,
    pending: bool,
}
impl Default for Resources {
    fn default() -> Self {
        use base64::Engine;
        Self {
            reads: Mutex::default(),
            bytes: base64::engine::general_purpose::STANDARD
                .decode(PNG)
                .unwrap()
                .into(),
            mime: "image/png".into(),
            length: None,
            pending: false,
        }
    }
}
impl ResourceAccess for Resources {
    type Scope = String;
    type PublishedHandle = ();
    fn resolve<'a>(
        &'a self,
        _: &'a String,
        _: &'a ResourceReference,
    ) -> CapabilityFuture<'a, Result<ResourceMetadata, CapabilityError>> {
        panic!("read provides metadata")
    }
    fn read<'a>(
        &'a self,
        scope: &'a String,
        reference: &'a ResourceReference,
    ) -> CapabilityFuture<'a, Result<ResourceRead, CapabilityError>> {
        Box::pin(async move {
            self.reads
                .lock()
                .unwrap()
                .push((scope.clone(), reference.clone()));
            if self.pending {
                return std::future::pending().await;
            }
            Ok(ResourceRead {
                metadata: ResourceMetadata {
                    mime: Some(self.mime.clone()),
                    length: Some(self.length.unwrap_or(self.bytes.len() as u64)),
                    filename: None,
                    expires_at: None,
                },
                body: HttpBody::Bytes(self.bytes.clone()),
            })
        })
    }
    fn publish<'a>(
        &'a self,
        _: &'a String,
        _: &'a str,
        _: PublicationKind,
        _: ResourceMetadata,
        _: HttpBody,
    ) -> CapabilityFuture<'a, Result<PublishedResource<()>, CapabilityError>> {
        panic!("Guardian does not publish")
    }
    fn publication_status<'a>(
        &'a self,
        _: &'a String,
        _: &'a str,
    ) -> CapabilityFuture<'a, Result<PublicationStatus<()>, CapabilityError>> {
        panic!("Guardian does not publish")
    }
    fn release<'a>(
        &'a self,
        _: &'a String,
        _: &'a (),
    ) -> CapabilityFuture<'a, Result<(), CapabilityError>> {
        panic!("Guardian does not release")
    }
    fn limits(&self) -> CapabilityLimits {
        CapabilityLimits {
            operation_total: Duration::from_secs(30),
            stream_idle: Duration::from_secs(5),
            read_bytes: 65536,
            write_bytes: 65536,
            ws_frame_bytes: 1024,
        }
    }
}
fn media_source() -> gproxy_protocol::openai::guardian::GuardianRequestBody {
    let mut input = source_request();
    input.input = serde_json::from_value(json!([
        {"type":"message","role":"user","content":[{"type":"input_image","image_url":"https://assets.test/image.png","detail":"auto"}]},
        {"type":"function_call_output","call_id":"call-1","output":[{"type":"input_image","image_url":"https://assets.test/image.png","detail":"auto"}]}
    ])).unwrap();
    input
}
fn resource_limits() -> guardian::GuardianResourceLimits {
    guardian::GuardianResourceLimits {
        codec: codec_limits(),
        max_media: 4,
        max_resource_bytes: 65536,
        max_total_resource_bytes: 131072,
    }
}
fn resource_context() -> GuardianRequestContext {
    GuardianRequestContext {
        target_model: "m".into(),
        max_tokens: 128,
        operation: GuardianOperation::Review,
    }
}
#[test]
fn scoped_media_resolution_deduplicates_reads_and_composes_with_generation() {
    let resources = Resources::default();
    let scope = "authorized-user-and-upstream".to_owned();
    let result = ready(guardian::prepare_with_resources(
        &resources,
        &scope,
        media_source(),
        transform::GuardianTarget::Gemini,
        resource_context(),
        resource_limits(),
    ))
    .unwrap();
    assert_eq!(
        *resources.reads.lock().unwrap(),
        vec![(
            scope.clone(),
            ResourceReference::Url("https://assets.test/image.png".into())
        )]
    );
    assert!(
        serde_json::to_string(result.value.source())
            .unwrap()
            .contains("https://assets.test/image.png")
    );
    let transform::GuardianDialectRequest::Gemini(request) = result.value.request() else {
        unreachable!()
    };
    let json = serde_json::to_string(&request.body).unwrap();
    assert_eq!(json.matches(PNG).count(), 2);
    assert!(!json.contains("https://assets.test/image.png"));
    let host = Host {
        sent: Mutex::default(),
        response_status: StatusCode::OK,
        pending: false,
    };
    assert!(matches!(
        ready(guardian::review(&host, &(), result.value, limits())).unwrap(),
        guardian::GuardianReviewInvocation::Success { .. }
    ));
    assert_eq!(host.sent.lock().unwrap().len(), 1);
    for target in [
        transform::GuardianTarget::OpenAiResponses,
        transform::GuardianTarget::OpenAiChat,
        transform::GuardianTarget::Claude,
    ] {
        let resources = Resources::default();
        ready(guardian::prepare_with_resources(
            &resources,
            &scope,
            media_source(),
            target,
            resource_context(),
            resource_limits(),
        ))
        .unwrap();
        assert!(
            resources.reads.lock().unwrap().is_empty(),
            "native URL target must not fetch unnecessarily"
        );
    }
}
#[test]
fn resource_options_count_and_native_media_fail_before_any_read() {
    for mutation in 0..4 {
        let resources = Resources::default();
        let mut input = media_source();
        let mut cap = resource_limits();
        match mutation {
            0 => input.access_programs = serde_json::from_value(json!({"cyber":"standard"})).ok(),
            1 => input.tool_choice = "required".into(),
            2 => cap.max_media = 1,
            _ => {
                input.input.push(serde_json::from_value(json!({"type":"message","role":"user","content":[{"type":"input_image","image_url":"data:image/png;base64,AA==","detail":"auto"}]})).unwrap());
            }
        }
        let result = ready(guardian::prepare_with_resources(
            &resources,
            &"scope".into(),
            input,
            transform::GuardianTarget::Gemini,
            resource_context(),
            cap,
        ));
        assert_eq!(result.is_err(), mutation == 2);
        if mutation == 2 {
            assert!(resources.reads.lock().unwrap().is_empty());
        } else {
            assert!(!resources.reads.lock().unwrap().is_empty());
        }
    }
}
#[test]
fn resource_actual_length_mime_aggregate_and_cancellation_are_enforced() {
    for mutation in 0..3 {
        let mut resources = Resources::default();
        let mut cap = resource_limits();
        match mutation {
            0 => resources.length = Some(1),
            1 => resources.mime = "image/jpeg".into(),
            _ => cap.max_total_resource_bytes = resources.bytes.len() as u64,
        }
        let result = ready(guardian::prepare_with_resources(
            &resources,
            &"scope".into(),
            media_source(),
            transform::GuardianTarget::Gemini,
            resource_context(),
            cap,
        ));
        assert_eq!(result.is_err(), mutation != 1);
        assert!(!resources.reads.lock().unwrap().is_empty());
    }
    let resources = Resources {
        pending: true,
        ..Default::default()
    };
    let scope = "scope".to_owned();
    let mut future = Box::pin(guardian::prepare_with_resources(
        &resources,
        &scope,
        media_source(),
        transform::GuardianTarget::Gemini,
        resource_context(),
        resource_limits(),
    ));
    let mut cx = Context::from_waker(std::task::Waker::noop());
    assert!(future.as_mut().poll(&mut cx).is_pending());
    drop(future);
    assert_eq!(resources.reads.lock().unwrap().len(), 1);
}
