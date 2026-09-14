use gproxy_protocol::{
    Dialect, HttpBody, WireRequest, WireResponse,
    adapt::generate::{
        chat_claude::*, chat_gemini::*, chat_responses::*, claude_gemini::*, claude_responses::*,
        gemini_responses::*, *,
    },
    capability::{
        CapabilityError, CapabilityFuture, CapabilityLimits, Upstream, UpstreamConnection,
    },
    codec::CodecLimits,
    transform::{
        TransformError, TransformErrorKind,
        generate::{claude_chat, claude_gemini, gemini_chat},
        identity::IdNamespace,
    },
};
use serde_json::{Value, json};
use std::{sync::Mutex, time::Duration};
fn ready<F: std::future::Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut cx) {
        std::task::Poll::Ready(v) => v,
        _ => panic!("unexpected pending"),
    }
}
struct Host {
    response: Mutex<Option<WireResponse<HttpBody>>>,
    sent: Mutex<Vec<WireRequest<HttpBody>>>,
}
impl Host {
    fn new(body: Value) -> Self {
        Self::raw(200, body.to_string())
    }
    fn raw(status: u16, body: String) -> Self {
        Self {
            response: Mutex::new(Some(WireResponse {
                status: http::StatusCode::from_u16(status).unwrap(),
                headers: http::HeaderMap::from_iter([(
                    http::header::CONTENT_LENGTH,
                    http::HeaderValue::from_static("999"),
                )]),
                body: HttpBody::Bytes(body.into()),
            })),
            sent: Mutex::new(Vec::new()),
        }
    }
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
            let response = self.response.lock().unwrap().take();
            match response {
                Some(value) => Ok(value),
                None => std::future::pending().await,
            }
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
            operation_total: Duration::from_secs(10),
            stream_idle: Duration::from_secs(1),
            read_bytes: 65536,
            write_bytes: 65536,
            ws_frame_bytes: 65536,
        }
    }
}
fn ids(client: Dialect, upstream: Dialect) -> GenerationIdentity {
    GenerationIdentity::new(IdNamespace([1; 16]), IdNamespace([2; 16]), client, upstream).unwrap()
}
fn endpoint() -> Endpoint {
    Endpoint::new("/selected/generate").unwrap()
}
fn input(dialect: &str) -> Value {
    match dialect {
        "h" => {
            json!({"model":"client","max_completion_tokens":64,"messages":[{"role":"user","content":"hi","foreign":"ignore"}],"foreign":"ignore"})
        }
        "c" => {
            json!({"model":"client","max_tokens":64,"messages":[{"role":"user","content":"hi","foreign":"ignore"}],"foreign":"ignore"})
        }
        "r" => json!({"model":"client","max_output_tokens":64,"input":"hi","foreign":"ignore"}),
        "g" => {
            json!({"contents":[{"role":"user","parts":[{"text":"hi","foreign":"ignore"}]}],"generationConfig":{"maxOutputTokens":64},"foreign":"ignore"})
        }
        _ => unreachable!(),
    }
}
fn output(dialect: &str) -> Value {
    match dialect {
        "h" => {
            json!({"id":"chat-native","object":"chat.completion","created":123,"model":"selected","choices":[{"index":0,"message":{"role":"assistant","content":"answer","refusal":null,"foreign":"ignore"},"finish_reason":"stop","logprobs":null}],"usage":{"prompt_tokens":4,"completion_tokens":2,"total_tokens":6,"prompt_tokens_details":{"cached_tokens":0},"completion_tokens_details":{"reasoning_tokens":0}},"foreign":"ignore"})
        }
        "c" => {
            json!({"id":"msg-native","type":"message","role":"assistant","model":"selected","content":[{"type":"text","text":"answer","foreign":"ignore"}],"stop_reason":"end_turn","usage":{"input_tokens":4,"output_tokens":2,"cache_creation_input_tokens":0,"cache_read_input_tokens":0,"output_tokens_details":{"thinking_tokens":0}},"foreign":"ignore"})
        }
        "g" => {
            json!({"responseId":"g-native","modelVersion":"selected","candidates":[{"index":0,"content":{"role":"model","parts":[{"text":"answer","foreign":"ignore"}]},"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":4,"candidatesTokenCount":2,"totalTokenCount":6,"cachedContentTokenCount":0,"thoughtsTokenCount":0},"foreign":"ignore"})
        }
        "r" => {
            json!({"id":"resp-native","object":"response","created_at":123,"model":"selected","status":"completed","error":null,"incomplete_details":null,"instructions":null,"metadata":null,"parallel_tool_calls":true,"temperature":null,"top_p":null,"tool_choice":"auto","tools":[],"output":[{"id":"msg-native","type":"message","role":"assistant","status":"completed","content":[{"type":"output_text","text":"answer","annotations":[],"logprobs":[],"foreign":"ignore"}]}],"usage":{"input_tokens":4,"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},"output_tokens":2,"output_tokens_details":{"reasoning_tokens":0},"total_tokens":6},"foreign":"ignore"})
        }
        _ => unreachable!(),
    }
}
fn auto() -> gproxy_protocol::wire::openai::responses::ToolChoice {
    serde_json::from_value(json!("auto")).unwrap()
}
fn check<C: serde::Serialize>(value: GenerationOutcome<C>, host: &Host, input_dialect: &str) {
    let GenerationOutcome::Success { response, .. } = value else {
        panic!("rejected")
    };
    assert_eq!(response.status, http::StatusCode::OK);
    assert!(!response.headers.contains_key(http::header::CONTENT_LENGTH));
    let value = serde_json::to_value(response.body).unwrap();
    assert!(!value.to_string().contains("foreign"));
    let answer = match input_dialect {
        "h" => &value["choices"][0]["message"]["content"],
        "c" => &value["content"][0]["text"],
        "g" => &value["candidates"][0]["content"]["parts"][0]["text"],
        "r" => &value["output"][0]["content"][0]["text"],
        _ => unreachable!(),
    };
    assert_eq!(answer, "answer");
    let sent = host.sent.lock().unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].path, "/selected/generate");
    let HttpBody::Bytes(bytes) = &sent[0].body else {
        panic!("not bytes")
    };
    assert!(!std::str::from_utf8(bytes).unwrap().contains("foreign"));
}

fn chat_via_claude() -> ChatViaClaude {
    ChatViaClaude::prepare(
        serde_json::from_value(input("h")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::OpenAiChat, Dialect::Claude),
    )
    .unwrap()
}

fn codec_limits() -> CodecLimits {
    CodecLimits {
        max_buffer_bytes: 65536,
        max_value_bytes: 65536,
        max_body_bytes: 65536,
        max_line_bytes: 65536,
        max_part_bytes: 65536,
        max_parts: 32,
    }
}

#[derive(Default)]
struct Store {
    entries: Mutex<std::collections::BTreeMap<String, gproxy_protocol::capability::StateEntry>>,
    attempts: Mutex<usize>,
    fail_at: Option<usize>,
}
impl gproxy_protocol::capability::StateStore for Store {
    type Scope = ();
    fn get<'a>(
        &'a self,
        _: &'a (),
        key: &'a str,
    ) -> CapabilityFuture<
        'a,
        Result<Option<gproxy_protocol::capability::StateEntry>, CapabilityError>,
    > {
        Box::pin(async move { Ok(self.entries.lock().unwrap().get(key).cloned()) })
    }
    fn compare_exchange<'a>(
        &'a self,
        _: &'a (),
        key: &'a str,
        expected: Option<gproxy_protocol::capability::Version>,
        replacement: Option<gproxy_protocol::capability::StateWrite>,
    ) -> CapabilityFuture<'a, Result<gproxy_protocol::capability::CasResult, CapabilityError>> {
        use gproxy_protocol::capability::{CasResult, StateEntry, Version};
        Box::pin(async move {
            let mut n = self.attempts.lock().unwrap();
            *n += 1;
            if self.fail_at == Some(*n) {
                return Ok(CasResult::Conflict);
            }
            let mut entries = self.entries.lock().unwrap();
            if entries.get(key).map(|e| e.version.clone()) != expected {
                return Ok(CasResult::Conflict);
            }
            let value = replacement.unwrap();
            let version = Version::from_bytes(n.to_be_bytes().to_vec());
            entries.insert(
                key.into(),
                StateEntry {
                    payload: value.payload,
                    version: version.clone(),
                    expires_at: value.expires_at,
                },
            );
            Ok(CasResult::Applied(Some(version)))
        })
    }
    fn limits(&self) -> CapabilityLimits {
        Host::new(json!(null)).limits()
    }
}
fn state(store: &Store, dialect: Dialect) -> GenerationStateAccess<'_, Store> {
    GenerationStateAccess {
        store,
        scope: &(),
        target: gproxy_protocol::transform::identity::IdentityTarget::new("selected", dialect)
            .unwrap()
            .with_origin("actual-upstream")
            .unwrap(),
        conversation_key: "conversation".into(),
        expires_at: std::time::UNIX_EPOCH + Duration::from_secs(1000),
        now: std::time::UNIX_EPOCH,
        max_records: 64,
    }
}

fn gemini_tools_host() -> Host {
    let mut body = output("g");
    body["candidates"][0]["content"]["parts"] = json!([{ "functionCall":{"name":"same","args":{"a":1}} },{ "functionCall":{"name":"same","args":{"a":2}} }]);
    Host::new(body)
}
fn chat_gemini() -> ChatViaGemini {
    ChatViaGemini::prepare(
        serde_json::from_value(input("h")).unwrap(),
        "selected",
        endpoint(),
        ids(Dialect::OpenAiChat, Dialect::Gemini),
        &Default::default(),
    )
    .unwrap()
}
fn gemini_facts(
    _: &gproxy_protocol::wire::gemini::GenerateContentResponseBody,
) -> Result<gemini_chat::GeminiChatResponseSupplement, TransformError> {
    Ok(gemini_chat::GeminiChatResponseSupplement {
        created_unix_seconds: Some(123),
    })
}

struct Resources {
    reads: Mutex<Vec<(String, gproxy_protocol::capability::ResourceReference)>>,
    bytes: bytes::Bytes,
    mime: String,
    length: Option<u64>,
}
impl Resources {
    fn png() -> Self {
        use base64::Engine;
        let bytes:bytes::Bytes=base64::engine::general_purpose::STANDARD.decode("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC").unwrap().into();
        Self {
            length: Some(bytes.len() as u64),
            bytes,
            mime: "image/png".into(),
            reads: Default::default(),
        }
    }
}
impl gproxy_protocol::capability::ResourceAccess for Resources {
    type Scope = String;
    type PublishedHandle = ();
    fn resolve<'a>(
        &'a self,
        _: &'a String,
        _: &'a gproxy_protocol::capability::ResourceReference,
    ) -> CapabilityFuture<'a, Result<gproxy_protocol::capability::ResourceMetadata, CapabilityError>>
    {
        panic!("unused")
    }
    fn read<'a>(
        &'a self,
        scope: &'a String,
        reference: &'a gproxy_protocol::capability::ResourceReference,
    ) -> CapabilityFuture<'a, Result<gproxy_protocol::capability::ResourceRead, CapabilityError>>
    {
        Box::pin(async move {
            self.reads
                .lock()
                .unwrap()
                .push((scope.clone(), reference.clone()));
            Ok(gproxy_protocol::capability::ResourceRead {
                metadata: gproxy_protocol::capability::ResourceMetadata {
                    mime: Some(self.mime.clone()),
                    length: self.length,
                    filename: Some("actual.png".into()),
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
        _: gproxy_protocol::capability::PublicationKind,
        _: gproxy_protocol::capability::ResourceMetadata,
        _: HttpBody,
    ) -> CapabilityFuture<
        'a,
        Result<gproxy_protocol::capability::PublishedResource<()>, CapabilityError>,
    > {
        panic!("unused")
    }
    fn publication_status<'a>(
        &'a self,
        _: &'a String,
        _: &'a str,
    ) -> CapabilityFuture<
        'a,
        Result<gproxy_protocol::capability::PublicationStatus<()>, CapabilityError>,
    > {
        panic!("unused")
    }
    fn release<'a>(
        &'a self,
        _: &'a String,
        _: &'a (),
    ) -> CapabilityFuture<'a, Result<(), CapabilityError>> {
        panic!("unused")
    }
    fn limits(&self) -> CapabilityLimits {
        Host::new(json!(null)).limits()
    }
}
fn resources<'a>(access: &'a Resources, scope: &'a String) -> GenerationResources<'a, Resources> {
    GenerationResources {
        access,
        scope,
        limits: codec_limits(),
        max_references: 8,
        now: std::time::UNIX_EPOCH,
    }
}

#[path = "adapt_generate/buffered.rs"]
mod buffered;

#[path = "adapt_generate/state.rs"]
mod state;

#[path = "adapt_generate/signed.rs"]
mod signed;

#[path = "adapt_generate/orphan.rs"]
mod orphan;
#[path = "adapt_generate/parity.rs"]
mod parity;
#[path = "adapt_generate/resources.rs"]
mod resources;

#[path = "adapt_generate/review.rs"]
mod review;

#[path = "adapt_generate/chat_form.rs"]
mod chat_form;

#[path = "adapt_generate/chat_form_roles.rs"]
mod chat_form_roles;
