use gproxy_protocol::{
    Dialect, HttpBody, WireRequest, WireResponse,
    adapt::generate::fanout::*,
    adapt::generate::*,
    capability::{
        CapabilityError, CapabilityFuture, CapabilityLimits, Upstream, UpstreamConnection,
    },
    codec::CodecLimits,
    transform::{TransformError, TransformErrorKind, generate::claude_chat, identity::IdNamespace},
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
    }
}

struct Host {
    responses: Mutex<std::collections::VecDeque<WireResponse<HttpBody>>>,
    sent: Mutex<Vec<WireRequest<HttpBody>>>,
}
impl Host {
    fn new(body: Value) -> Self {
        Self::many(vec![body])
    }
    fn many(bodies: Vec<Value>) -> Self {
        Self {
            responses: Mutex::new(
                bodies
                    .into_iter()
                    .map(|body| WireResponse {
                        status: http::StatusCode::OK,
                        headers: Default::default(),
                        body: HttpBody::Bytes(body.to_string().into()),
                    })
                    .collect(),
            ),
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
            let raw = self.responses.lock().unwrap().pop_front();
            match raw {
                Some(r) => Ok(r),
                None => std::future::pending().await,
            }
        })
    }
    fn connect<'a>(
        &'a self,
        _: &'a (),
        _: WireRequest<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        panic!("unused")
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
fn setup(client: Dialect, _upstream: Dialect) -> FanoutTarget {
    FanoutTarget {
        endpoint: Endpoint::new("/selected/generate").unwrap(),

        options: FanoutOptions {
            response_policy: gproxy_protocol::transform::identity::TargetIdPolicy::new(client),
            namespace: IdNamespace([5; 16]),
        },
    }
}
fn chat_input() -> gproxy_protocol::wire::openai::chat::GenerateContentRequestBody {
    let mut v = input("h");
    v["n"] = json!(2);
    serde_json::from_value(v).unwrap()
}
fn gemini_input() -> gproxy_protocol::wire::gemini::GenerateContentRequestBody {
    let mut v = input("g");
    v["generationConfig"]["candidateCount"] = json!(2);
    serde_json::from_value(v).unwrap()
}
fn clock(
    _: usize,
    _: &gproxy_protocol::wire::claude::generate_content::GenerateContentResponseBody,
) -> Result<claude_chat::ResponseSupplement, TransformError> {
    Ok(claude_chat::ResponseSupplement {
        created_unix_seconds: Some(123),
    })
}
fn two(dialect: &str) -> Vec<Value> {
    (0..2)
        .map(|i| {
            let mut v = output(dialect);
            v["id"] = json!(format!("native-{i}"));
            v
        })
        .collect()
}
fn assert_sends(host: &Host) {
    let sent = host.sent.lock().unwrap();
    assert_eq!(sent.len(), 2);
    for req in sent.iter() {
        assert_eq!(req.path, "/selected/generate");
        let HttpBody::Bytes(bytes) = &req.body else {
            panic!("not bytes")
        };
        assert!(!String::from_utf8_lossy(bytes).contains("foreign"));
        let v: Value = serde_json::from_slice(bytes).unwrap();
        assert!(v.get("n").is_none());
        assert!(v.get("candidateCount").is_none());
    }
}
#[test]
fn four_typed_fanouts_execute_real_posts_order_candidates_and_sum_actual_charges() {
    let store = Store::default();
    let state = state(&store, Dialect::Claude);
    let host = Host::many(two("c"));
    let mut p = ready(ChatViaClaudeFanout::prepare(
        chat_input(),
        setup(Dialect::OpenAiChat, Dialect::Claude),
        &state,
    ))
    .unwrap();
    let mut progress = FanoutProgress::default();
    let out = ready(p.invoke(&host, &(), codec_limits(), &state, &mut progress, clock))
        .unwrap()
        .value;
    assert_eq!(out.choices.len(), 2);
    assert_eq!(out.choices[1].index, 1);
    assert_eq!(out.usage.unwrap().prompt_tokens, 8);
    assert_eq!(out.created, 123);
    assert_eq!(out.id, p.response_id());
    assert_sends(&host);

    let store = Store::default();
    let state = state_fn(&store, Dialect::OpenAi);
    let host = Host::many(two("r"));
    let mut p = ready(ChatViaResponsesFanout::prepare(
        chat_input(),
        setup(Dialect::OpenAiChat, Dialect::OpenAi),
        &state,
    ))
    .unwrap();
    let out = ready(p.invoke(
        &host,
        &(),
        codec_limits(),
        &state,
        &mut FanoutProgress::default(),
        |_, _| Ok(()),
    ))
    .unwrap()
    .value;
    assert_eq!(out.choices.len(), 2);
    assert_eq!(out.usage.unwrap().total_tokens, 12);
    assert_sends(&host);

    let store = Store::default();
    let state = state_fn(&store, Dialect::Claude);
    let host = Host::many(two("c"));
    let mut p = ready(GeminiViaClaudeFanout::prepare(
        gemini_input(),
        setup(Dialect::Gemini, Dialect::Claude),
        &state,
        None,
    ))
    .unwrap();
    let out = ready(p.invoke(
        &host,
        &(),
        codec_limits(),
        &state,
        &mut FanoutProgress::default(),
        |_, _| Ok(Default::default()),
    ))
    .unwrap()
    .value;
    assert_eq!(out.candidates.unwrap()[1].index, Some(1));
    assert_eq!(out.usage_metadata.unwrap().prompt_token_count, Some(8));
    assert_sends(&host);

    let store = Store::default();
    let state = state_fn(&store, Dialect::OpenAi);
    let host = Host::many(two("r"));
    let mut p = ready(GeminiViaResponsesFanout::prepare(
        gemini_input(),
        setup(Dialect::Gemini, Dialect::OpenAi),
        &state,
    ))
    .unwrap();
    let out = ready(p.invoke(
        &host,
        &(),
        codec_limits(),
        &state,
        &mut FanoutProgress::default(),
        |_, _| Ok(Default::default()),
    ))
    .unwrap()
    .value;
    assert_eq!(out.candidates.unwrap().len(), 2);
    assert_eq!(out.usage_metadata.unwrap().total_token_count, Some(12));
    assert_sends(&host);
}
#[test]
fn gemini_fanout_without_generation_config_prepares_a_single_child() {
    let mut body = input("g");
    body.as_object_mut().unwrap().remove("generationConfig");
    let body: gproxy_protocol::wire::gemini::GenerateContentRequestBody =
        serde_json::from_value(body).unwrap();
    let store = Store::default();
    let state = state_fn(&store, Dialect::Claude);
    assert!(
        ready(GeminiViaClaudeFanout::prepare(
            body.clone(),
            setup(Dialect::Gemini, Dialect::Claude),
            &state,
            Some(64),
        ))
        .is_ok()
    );
    let state = state_fn(&store, Dialect::OpenAi);
    assert!(
        ready(GeminiViaResponsesFanout::prepare(
            body,
            setup(Dialect::Gemini, Dialect::OpenAi),
            &state,
        ))
        .is_ok()
    );
}
fn state_fn(store: &Store, dialect: Dialect) -> GenerationStateAccess<'_, Store> {
    state(store, dialect)
}
#[test]
fn duplicate_native_call_ids_across_independent_children_get_counted_aliases() {
    let store = Store::default();
    let state = state(&store, Dialect::Claude);
    let bodies = two("c")
        .into_iter()
        .enumerate()
        .map(|(i, mut v)| {
            v["content"] =
                json!([{"type":"tool_use","id":"same-call","name":"lookup","input":{"child":i}}]);
            v["stop_reason"] = json!("tool_use");
            v
        })
        .collect();
    let host = Host::many(bodies);
    let mut p = ready(ChatViaClaudeFanout::prepare(
        chat_input(),
        setup(Dialect::OpenAiChat, Dialect::Claude),
        &state,
    ))
    .unwrap();
    let out = serde_json::to_value(
        ready(p.invoke(
            &host,
            &(),
            codec_limits(),
            &state,
            &mut FanoutProgress::default(),
            clock,
        ))
        .unwrap()
        .value,
    )
    .unwrap();
    let first = out["choices"][0]["message"]["tool_calls"][0]["id"]
        .as_str()
        .unwrap();
    let second = out["choices"][1]["message"]["tool_calls"][0]["id"]
        .as_str()
        .unwrap();
    // The first child forwards Claude's ID; the second repeats it and gets
    // the counted alias, which names the same ID. Neither is recorded.
    assert_eq!(first, "same-call");
    assert_eq!(second, "call_gpe_same-call");
    assert!(store.entries.lock().unwrap().is_empty());
}
#[test]
fn cancellation_and_known_rejection_never_repeat_started_calls_or_complete_partial_group() {
    let store = Store::default();
    let state = state(&store, Dialect::Claude);
    let host = Host::many(vec![output("c")]);
    let mut p = ready(ChatViaClaudeFanout::prepare(
        chat_input(),
        setup(Dialect::OpenAiChat, Dialect::Claude),
        &state,
    ))
    .unwrap();
    let mut progress = FanoutProgress::default();
    {
        let mut f = Box::pin(p.invoke(&host, &(), codec_limits(), &state, &mut progress, clock));
        let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(f.as_mut().poll(&mut cx).is_pending());
    }
    assert!(progress.children()[0].raw_response.is_some());
    assert!(progress.children()[1].send_started);
    let mut fresh = FanoutProgress::default();
    assert_eq!(
        ready(p.invoke(&host, &(), codec_limits(), &state, &mut fresh, clock))
            .unwrap_err()
            .kind(),
        TransformErrorKind::Conflict
    );
    assert_eq!(host.sent.lock().unwrap().len(), 2);

    let store = Store::default();
    let state = state_fn(&store, Dialect::Claude);
    let host = Host::many(two("c"));
    host.responses.lock().unwrap()[1].status = http::StatusCode::TOO_MANY_REQUESTS;
    let mut p = ready(ChatViaClaudeFanout::prepare(
        chat_input(),
        setup(Dialect::OpenAiChat, Dialect::Claude),
        &state,
    ))
    .unwrap();
    let mut progress = FanoutProgress::default();
    assert!(ready(p.invoke(&host, &(), codec_limits(), &state, &mut progress, clock)).is_err());
    assert_eq!(
        progress.children()[1].raw_response.as_ref().unwrap().status,
        429
    );
    assert!(ready(p.invoke(&host, &(), codec_limits(), &state, &mut progress, clock)).is_err());
    assert_eq!(host.sent.lock().unwrap().len(), 2);
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

#[test]
fn materializes_foreign_image_once_before_all_child_posts() {
    let store = Store::default();
    let state = state(&store, Dialect::Claude);
    let access = Resources::png();
    let scope = "client".to_string();
    let resources = resources(&access, &scope);
    let mut input = serde_json::to_value(chat_input()).unwrap();
    input["messages"] = json!([{"role":"user","content":[{"type":"image_url","image_url":{"url":"https://foreign/image.png"}}]}]);
    let mut p = ready(ChatViaClaudeFanout::prepare_with_capabilities(
        serde_json::from_value(input).unwrap(),
        setup(Dialect::OpenAiChat, Dialect::Claude),
        &state,
        &resources,
    ))
    .unwrap();
    assert_eq!(access.reads.lock().unwrap().len(), 1);
    let host = Host::many(two("c"));
    ready(p.invoke(
        &host,
        &(),
        codec_limits(),
        &state,
        &mut FanoutProgress::default(),
        clock,
    ))
    .unwrap();
    assert_eq!(access.reads.lock().unwrap().len(), 1);
    assert_sends(&host);
    let sent = host.sent.lock().unwrap();
    for req in sent.iter() {
        let HttpBody::Bytes(bytes) = &req.body else {
            panic!()
        };
        let v: Value = serde_json::from_slice(bytes).unwrap();
        assert_eq!(v["messages"][0]["content"][0]["source"]["type"], "base64");
    }
}
#[test]
fn child_limits_fail_before_any_post_and_a_completed_group_never_sends_again() {
    let store = Store::default();
    let state = state(&store, Dialect::Claude);
    let host = Host::many(two("c"));
    let mut p = ready(ChatViaClaudeFanout::prepare(
        chat_input(),
        setup(Dialect::OpenAiChat, Dialect::Claude),
        &state,
    ))
    .unwrap();
    let mut progress = FanoutProgress::default();
    let tiny = CodecLimits {
        max_value_bytes: 1,
        ..codec_limits()
    };
    assert!(ready(p.invoke(&host, &(), tiny, &state, &mut progress, clock)).is_err());
    assert!(host.sent.lock().unwrap().is_empty());
    ready(p.invoke(&host, &(), codec_limits(), &state, &mut progress, clock)).unwrap();
    assert_eq!(
        ready(p.invoke(
            &host,
            &(),
            codec_limits(),
            &state,
            &mut FanoutProgress::default(),
            clock
        ))
        .unwrap_err()
        .kind(),
        TransformErrorKind::Conflict
    );
    assert_eq!(host.sent.lock().unwrap().len(), 2);
}

#[path = "adapt_generate_fanout/review.rs"]
mod review;
