use base64::Engine;
use gproxy_protocol::{
    HttpBody, WireRequest, WireResponse,
    adapt::{
        JsonInvocation,
        video::{self, VideoBinding, VideoLimits, VideoProgress},
    },
    capability::{
        CapabilityError, CapabilityFuture, CapabilityLimits, CasResult, PublicationKind,
        PublicationStatus, PublishedResource, ResourceAccess, ResourceMetadata, ResourceRead,
        ResourceReference, StateEntry, StateStore, StateWrite, Upstream, UpstreamConnection,
        Version,
    },
    codec::CodecLimits,
    connection::{Bytes, HeaderMap, StatusCode},
    transform::video::OpenAiVideoResponseContext,
    wire::openai::video as o,
};
use serde_json::json;
const PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";
fn png() -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(PNG)
        .unwrap()
}
fn expiry() -> std::time::SystemTime {
    std::time::SystemTime::now() + Duration::from_secs(600)
}
fn binding(id: &str) -> VideoBinding {
    VideoBinding {
        client_id: id.into(),
        origin: "google-test".into(),
        model: "veo".into(),
        polling_url: format!("https://proxy.test/videos/{id}"),
        operation_prefix: "/v1beta/".into(),
    }
}
use std::{sync::Mutex, time::Duration};

fn ready<F: std::future::Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    loop {
        match future.as_mut().poll(&mut cx) {
            std::task::Poll::Ready(value) => return value,
            std::task::Poll::Pending => continue,
        }
    }
}

struct Host {
    responses: Mutex<Vec<WireResponse<HttpBody>>>,
    sent: Mutex<Vec<WireRequest<HttpBody>>>,
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
            Ok(self.responses.lock().unwrap().remove(0))
        })
    }
    fn connect<'a>(
        &'a self,
        _: &'a (),
        _: WireRequest<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        panic!("unexpected websocket")
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

struct Resources;
impl ResourceAccess for Resources {
    type Scope = ();
    type PublishedHandle = ();
    fn resolve<'a>(
        &'a self,
        _: &'a (),
        _: &'a ResourceReference,
    ) -> CapabilityFuture<'a, Result<ResourceMetadata, CapabilityError>> {
        Box::pin(async {
            Ok(ResourceMetadata {
                mime: Some("image/png".into()),
                length: Some(png().len() as u64),
                filename: None,
                expires_at: None,
            })
        })
    }
    fn read<'a>(
        &'a self,
        _: &'a (),
        _: &'a ResourceReference,
    ) -> CapabilityFuture<'a, Result<ResourceRead, CapabilityError>> {
        Box::pin(async {
            Ok(ResourceRead {
                metadata: ResourceMetadata {
                    mime: Some("image/png".into()),
                    length: Some(png().len() as u64),
                    filename: None,
                    expires_at: None,
                },
                body: HttpBody::Bytes(Bytes::from(png())),
            })
        })
    }
    fn publish<'a>(
        &'a self,
        _: &'a (),
        _: &'a str,
        _: PublicationKind,
        metadata: ResourceMetadata,
        _: HttpBody,
    ) -> CapabilityFuture<'a, Result<PublishedResource<()>, CapabilityError>> {
        Box::pin(async move {
            Ok(PublishedResource {
                handle: (),
                reference: ResourceReference::Url("https://cdn.test/video.mp4".into()),
                metadata,
            })
        })
    }
    fn publication_status<'a>(
        &'a self,
        _: &'a (),
        _: &'a str,
    ) -> CapabilityFuture<'a, Result<PublicationStatus<()>, CapabilityError>> {
        Box::pin(async { Ok(PublicationStatus::Missing) })
    }
    fn release<'a>(
        &'a self,
        _: &'a (),
        _: &'a (),
    ) -> CapabilityFuture<'a, Result<(), CapabilityError>> {
        Box::pin(async { Ok(()) })
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
struct Store {
    entry: Mutex<Option<StateEntry>>,
    fail_cas: bool,
    fail_after: Option<usize>,
    attempts: Mutex<usize>,
}
impl StateStore for Store {
    type Scope = ();
    fn get<'a>(
        &'a self,
        _: &'a (),
        _: &'a str,
    ) -> CapabilityFuture<'a, Result<Option<StateEntry>, CapabilityError>> {
        Box::pin(async { Ok(self.entry.lock().unwrap().clone()) })
    }
    fn compare_exchange<'a>(
        &'a self,
        _: &'a (),
        _: &'a str,
        expected: Option<Version>,
        replacement: Option<StateWrite>,
    ) -> CapabilityFuture<'a, Result<CasResult, CapabilityError>> {
        Box::pin(async move {
            let mut attempts = self.attempts.lock().unwrap();
            *attempts += 1;
            if self.fail_cas || self.fail_after == Some(*attempts) {
                return Ok(CasResult::Conflict);
            }
            let mut state = self.entry.lock().unwrap();
            if state.as_ref().map(|s| s.version.clone()) != expected {
                return Ok(CasResult::Conflict);
            }
            let version = Version::from_bytes(attempts.to_be_bytes());
            *state = replacement.map(|write| StateEntry {
                payload: write.payload,
                version: version.clone(),
                expires_at: write.expires_at,
            });
            Ok(CasResult::Applied(Some(version)))
        })
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
fn limits() -> VideoLimits {
    VideoLimits {
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
fn template(path: &str) -> WireRequest<()> {
    WireRequest {
        method: http::Method::POST,
        path: path.into(),
        query: None,
        headers: HeaderMap::new(),
        body: (),
    }
}
fn input() -> o::CreateVideoRequestBody {
    serde_json::from_value(
        json!({"model":"google/veo","prompt":"sunset","duration":4,"aspect_ratio":"16:9"}),
    )
    .unwrap()
}
fn response(value: serde_json::Value) -> WireResponse<HttpBody> {
    WireResponse {
        status: StatusCode::OK,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&value).unwrap())),
    }
}
fn context() -> OpenAiVideoResponseContext {
    OpenAiVideoResponseContext {
        operation_name: "operations/123".into(),
        polling_url: "https://proxy.test/videos/job".into(),
        resources: std::collections::BTreeMap::new(),
    }
}

#[test]
fn create_pending_maps_once_and_preserves_operation_binding() {
    let host = Host {
        responses: Mutex::new(vec![response(
            json!({"name":"operations/123","done":false}),
        )]),
        sent: Mutex::default(),
    };
    let result = ready(video::openai_to_gemini_create(
        &host,
        &(),
        template("/v1beta/models/veo:predictLongRunning"),
        input(),
        "veo",
        Default::default(),
        context(),
        limits(),
    ))
    .unwrap();
    let JsonInvocation::Success(response) = result else {
        panic!("expected success")
    };
    assert_eq!(response.body.value.status, o::VideoStatus::InProgress);
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}

#[test]
fn query_done_maps_actual_public_video_url_and_raw_rejection() {
    let host = Host {
        responses: Mutex::new(vec![response(
            json!({"name":"operations/123","done":true,"response":{"generateVideoResponse":{"generatedSamples":[{"video":{"uri":"https://cdn.test/video.mp4"}}]}}}),
        )]),
        sent: Mutex::default(),
    };
    let result = ready(video::openai_to_gemini_query(
        &host,
        &(),
        WireRequest {
            method: http::Method::GET,
            path: "/v1beta/operations/123".into(),
            query: None,
            headers: HeaderMap::new(),
            body: (),
        },
        context(),
        limits(),
    ))
    .unwrap();
    let JsonInvocation::Success(response) = result else {
        panic!("expected success")
    };
    assert_eq!(response.body.value.status, o::VideoStatus::Completed);
    assert_eq!(
        response.body.value.unsigned_urls.unwrap(),
        vec!["https://cdn.test/video.mp4"]
    );
}

#[test]
fn media_resource_facts_are_required_before_send_and_http_rejection_is_raw() {
    let media = serde_json::from_value(json!({"model":"google/veo","prompt":"x","frame_images":[{"type":"image_url","image_url":{"url":"image-1"},"frame_type":"first_frame"}]})).unwrap();
    let host = Host {
        responses: Mutex::new(Vec::new()),
        sent: Mutex::default(),
    };
    let error = ready(video::openai_to_gemini_create(
        &host,
        &(),
        template("/v1beta/models/veo:predictLongRunning"),
        media,
        "veo",
        Default::default(),
        context(),
        limits(),
    ))
    .unwrap_err();
    assert_eq!(
        error.kind(),
        gproxy_protocol::transform::TransformErrorKind::MissingMetadata
    );
    assert!(host.sent.lock().unwrap().is_empty());
    let host = Host {
        responses: Mutex::new(vec![WireResponse {
            status: StatusCode::BAD_REQUEST,
            headers: HeaderMap::new(),
            body: HttpBody::Bytes(Bytes::from_static(b"provider error")),
        }]),
        sent: Mutex::default(),
    };
    let result = ready(video::openai_to_gemini_create(
        &host,
        &(),
        template("/v1beta/models/veo:predictLongRunning"),
        input(),
        "veo",
        Default::default(),
        context(),
        limits(),
    ))
    .unwrap();
    let JsonInvocation::Rejected(response) = result else {
        panic!("expected raw rejection")
    };
    let HttpBody::Bytes(body) = response.body else {
        panic!("bytes")
    };
    assert_eq!(body.as_ref(), b"provider error");
}

#[test]
fn native_sora_create_preserves_native_seconds_size_and_status() {
    let host = Host {
        responses: Mutex::new(vec![response(
            json!({"id":"video_1","created_at":123,"model":"sora-2","object":"video","progress":0,"seconds":"4","size":"1280x720","status":"queued","completed_at":null,"error":null,"expires_at":null,"prompt":"sunset","remixed_from_video_id":null}),
        )]),
        sent: Mutex::default(),
    };
    let request = WireRequest {
        method: http::Method::POST,
        path: "/videos".into(),
        query: None,
        headers: HeaderMap::new(),
        body: serde_json::from_value(
            json!({"prompt":"sunset","model":"sora-2","seconds":"4","size":"1280x720"}),
        )
        .unwrap(),
    };
    let result = ready(video::native_create(&host, &(), request, limits())).unwrap();
    let JsonInvocation::Success(response) = result else {
        panic!("expected native success")
    };
    assert_eq!(response.body.id, "video_1");
    assert_eq!(response.body.status, o::NativeVideoStatus::Queued);
}

#[test]
fn composed_create_reads_reference_and_cas_saves_operation_before_reply() {
    let host = Host {
        responses: Mutex::new(vec![response(
            json!({"name":"operations/456","done":false}),
        )]),
        sent: Mutex::default(),
    };
    let store = Store {
        entry: Mutex::new(None),
        fail_cas: false,
        fail_after: None,
        attempts: Mutex::default(),
    };
    let input = serde_json::from_value(json!({"model":"google/veo","prompt":"sunset","frame_images":[{"type":"image_url","image_url":{"url":"https://source.test/image.png"},"frame_type":"first_frame"}]})).unwrap();
    let result = ready(video::openai_to_gemini_create_composed(
        &host,
        &(),
        &Resources,
        &(),
        &store,
        &(),
        template("/v1beta/models/veo:predictLongRunning"),
        input,
        binding("client-1"),
        expiry(),
        &mut VideoProgress::default(),
        limits(),
    ))
    .unwrap();
    let JsonInvocation::Success(response) = result else {
        panic!("expected success")
    };
    assert_eq!(response.body.value.status, o::VideoStatus::InProgress);
    assert!(store.entry.lock().unwrap().is_some());
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}

#[test]
fn composed_query_uses_saved_operation_binding_without_recreation() {
    let host = Host {
        responses: Mutex::new(vec![
            response(json!({"name":"operations/456","done":false})),
            response(json!({"name":"operations/456","done":false})),
        ]),
        sent: Mutex::default(),
    };
    let store = Store {
        entry: Mutex::new(None),
        fail_cas: false,
        fail_after: None,
        attempts: Mutex::default(),
    };
    let input = serde_json::from_value(json!({"model":"google/veo","prompt":"sunset"})).unwrap();
    ready(video::openai_to_gemini_create_composed(
        &host,
        &(),
        &Resources,
        &(),
        &store,
        &(),
        template("/v1beta/models/veo:predictLongRunning"),
        input,
        binding("client-2"),
        expiry(),
        &mut VideoProgress::default(),
        limits(),
    ))
    .unwrap();
    let result = ready(video::openai_to_gemini_query_composed(
        &host,
        &(),
        &Resources,
        &(),
        &store,
        &(),
        WireRequest {
            method: http::Method::GET,
            path: "/v1beta/operations/456".into(),
            query: None,
            headers: HeaderMap::new(),
            body: (),
        },
        binding("client-2"),
        expiry(),
        &mut VideoProgress::default(),
        limits(),
    ))
    .unwrap();
    let JsonInvocation::Success(response) = result else {
        panic!("expected query success")
    };
    assert_eq!(response.body.value.status, o::VideoStatus::InProgress);
    assert_eq!(host.sent.lock().unwrap().len(), 2);
}

fn store() -> Store {
    Store {
        entry: Mutex::default(),
        fail_cas: false,
        fail_after: None,
        attempts: Mutex::default(),
    }
}
fn host(values: Vec<serde_json::Value>) -> Host {
    Host {
        responses: Mutex::new(values.into_iter().map(response).collect()),
        sent: Mutex::default(),
    }
}
fn create(
    host: &Host,
    store: &Store,
    progress: &mut VideoProgress,
) -> Result<
    JsonInvocation<gproxy_protocol::transform::Converted<o::VideoGenerationResponseBody>>,
    gproxy_protocol::transform::TransformError,
> {
    ready(video::openai_to_gemini_create_composed(
        host,
        &(),
        &Resources,
        &(),
        store,
        &(),
        template("/v1beta/models/veo:predictLongRunning"),
        input(),
        binding("client"),
        expiry(),
        progress,
        limits(),
    ))
}
fn query(
    host: &Host,
    store: &Store,
    path: &str,
    b: VideoBinding,
) -> Result<
    JsonInvocation<gproxy_protocol::transform::Converted<o::VideoGenerationResponseBody>>,
    gproxy_protocol::transform::TransformError,
> {
    let mut t = template(path);
    t.method = http::Method::GET;
    ready(video::openai_to_gemini_query_composed(
        host,
        &(),
        &Resources,
        &(),
        store,
        &(),
        t,
        b,
        expiry(),
        &mut VideoProgress::default(),
        limits(),
    ))
}
#[test]
fn duplicate_create_conflicts_before_a_second_upstream_side_effect() {
    let host = host(vec![json!({"name":"operations/a","done":false})]);
    let store = store();
    let JsonInvocation::Success(r) = create(&host, &store, &mut VideoProgress::default()).unwrap()
    else {
        panic!()
    };
    assert_eq!(r.body.value.id, "client");
    assert!(create(&host, &store, &mut VideoProgress::default()).is_err());
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}
#[test]
fn reserve_conflict_has_no_upstream_send() {
    let host = host(vec![]);
    let mut store = store();
    store.fail_cas = true;
    let mut progress = VideoProgress::default();
    assert!(create(&host, &store, &mut progress).is_err());
    assert!(!progress.send_started);
    assert!(host.sent.lock().unwrap().is_empty());
}
#[test]
fn post_create_cas_failure_retains_native_response_and_reservation() {
    let host = host(vec![json!({"name":"operations/a","done":false})]);
    let mut store = store();
    store.fail_after = Some(2);
    let mut progress = VideoProgress::default();
    assert!(create(&host, &store, &mut progress).is_err());
    assert_eq!(
        progress
            .native_response
            .as_ref()
            .unwrap()
            .body
            .name
            .as_deref(),
        Some("operations/a")
    );
    assert!(progress.raw_response.is_some());
    assert!(progress.reserved);
    assert!(!progress.binding_saved);
    assert!(create(&host, &store, &mut VideoProgress::default()).is_err());
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}
#[test]
fn bound_query_rejects_wrong_path_origin_model_and_client_before_send() {
    let host = host(vec![json!({"name":"operations/a","done":false})]);
    let store = store();
    create(&host, &store, &mut VideoProgress::default()).unwrap();
    assert!(query(&host, &store, "/v1beta/operations/b", binding("client")).is_err());
    for field in ["origin", "model", "client", "poll"] {
        let mut b = binding("client");
        match field {
            "origin" => b.origin = "different".into(),
            "model" => b.model = "other".into(),
            "client" => b.client_id = "other".into(),
            _ => b.polling_url = "https://other.test/poll".into(),
        };
        assert!(query(&host, &store, "/v1beta/operations/a", b).is_err());
    }
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}
#[test]
fn malformed_success_preserves_raw_and_blocks_recreation() {
    let host = host(vec![json!({"name":42})]);
    let store = store();
    let mut progress = VideoProgress::default();
    assert!(create(&host, &store, &mut progress).is_err());
    assert!(progress.raw_response.is_some());
    assert!(progress.native_response.is_none());
    assert!(create(&host, &store, &mut VideoProgress::default()).is_err());
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}
#[test]
fn operation_mismatch_after_query_preserves_the_previous_durable_binding() {
    let host = host(vec![
        json!({"name":"operations/a","done":false}),
        json!({"name":"operations/b","done":false}),
    ]);
    let store = store();
    create(&host, &store, &mut VideoProgress::default()).unwrap();
    assert!(query(&host, &store, "/v1beta/operations/a", binding("client")).is_err());
    let s: video::VideoJobState =
        serde_json::from_slice(&store.entry.lock().unwrap().as_ref().unwrap().payload).unwrap();
    assert_eq!(s.operation.unwrap().name.as_deref(), Some("operations/a"));
}
#[test]
fn nested_unknown_extensions_never_reach_durable_job_state() {
    let host = host(vec![
        json!({"name":"operations/a","done":false,"sentinel":"source","metadata":{"declared":"keep"}}),
    ]);
    let store = store();
    let input =
        serde_json::from_value(json!({"model":"google/veo","prompt":"x","sentinel":"request"}))
            .unwrap();
    ready(video::openai_to_gemini_create_composed(
        &host,
        &(),
        &Resources,
        &(),
        &store,
        &(),
        template("/v1beta/models/veo:predictLongRunning"),
        input,
        binding("client"),
        expiry(),
        &mut VideoProgress::default(),
        limits(),
    ))
    .unwrap();
    let data = String::from_utf8(
        store
            .entry
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .payload
            .to_vec(),
    )
    .unwrap();
    assert!(!data.contains("sentinel"));
    assert!(data.contains("declared"));
}
#[test]
fn encoded_video_is_bounded_before_decode_and_publication() {
    let mut l = limits();
    l.codec.max_body_bytes = 2;
    assert!(
        ready(video::publish_video_output(
            &Resources,
            &(),
            "op",
            "AAAA".repeat(100).as_str(),
            "video/mp4",
            expiry(),
            l
        ))
        .is_err()
    );
    assert!(
        ready(video::publish_video_output(
            &Resources,
            &(),
            "op",
            "aGk=",
            "video/mp4",
            expiry(),
            limits()
        ))
        .is_err()
    );
}

struct VideoResources {
    read_calls: Mutex<Vec<ResourceReference>>,
    published: Mutex<std::collections::BTreeMap<String, PublishedResource<()>>>,
    fail_after_publish: std::sync::atomic::AtomicBool,
    read_cap: u64,
    body: Vec<u8>,
}
fn videos() -> VideoResources {
    VideoResources {
        read_calls: Mutex::default(),
        published: Mutex::default(),
        fail_after_publish: std::sync::atomic::AtomicBool::new(false),
        read_cap: 64 * 1024,
        body: mp4().to_vec(),
    }
}
fn mp4() -> &'static [u8] {
    b"\0\0\0\x10ftypisom\0\0\0\0"
}
impl ResourceAccess for VideoResources {
    type Scope = ();
    type PublishedHandle = ();
    fn resolve<'a>(
        &'a self,
        scope: &'a (),
        reference: &'a ResourceReference,
    ) -> CapabilityFuture<'a, Result<ResourceMetadata, CapabilityError>> {
        Resources.resolve(scope, reference)
    }
    fn read<'a>(
        &'a self,
        _: &'a (),
        r: &'a ResourceReference,
    ) -> CapabilityFuture<'a, Result<ResourceRead, CapabilityError>> {
        Box::pin(async move {
            self.read_calls.lock().unwrap().push(r.clone());
            Ok(ResourceRead {
                metadata: ResourceMetadata {
                    mime: Some("video/mp4".into()),
                    length: Some(self.body.len() as u64),
                    filename: None,
                    expires_at: None,
                },
                body: HttpBody::Bytes(Bytes::from(self.body.clone())),
            })
        })
    }
    fn publish<'a>(
        &'a self,
        _: &'a (),
        id: &'a str,
        _: PublicationKind,
        metadata: ResourceMetadata,
        body: HttpBody,
    ) -> CapabilityFuture<'a, Result<PublishedResource<()>, CapabilityError>> {
        Box::pin(async move {
            if let Some(old) = self.published.lock().unwrap().get(id) {
                return Ok(old.clone());
            }
            let HttpBody::Bytes(body) = body else {
                panic!()
            };
            assert_eq!(body.as_ref(), self.body.as_slice());
            let publication = PublishedResource {
                handle: (),
                reference: ResourceReference::Url(format!(
                    "https://public.test/{}.mp4",
                    self.published.lock().unwrap().len()
                )),
                metadata,
            };
            self.published
                .lock()
                .unwrap()
                .insert(id.into(), publication.clone());
            if self
                .fail_after_publish
                .swap(false, std::sync::atomic::Ordering::SeqCst)
            {
                return Err(CapabilityError::new(
                    gproxy_protocol::capability::CapabilityErrorKind::Transport,
                    gproxy_protocol::capability::CapabilityErrorStage::BodyTransfer,
                    "lost publication response",
                ));
            }
            Ok(publication)
        })
    }
    fn publication_status<'a>(
        &'a self,
        _: &'a (),
        id: &'a str,
    ) -> CapabilityFuture<'a, Result<PublicationStatus<()>, CapabilityError>> {
        Box::pin(async move {
            Ok(self
                .published
                .lock()
                .unwrap()
                .get(id)
                .cloned()
                .map_or(PublicationStatus::Missing, PublicationStatus::Published))
        })
    }
    fn release<'a>(
        &'a self,
        scope: &'a (),
        handle: &'a (),
    ) -> CapabilityFuture<'a, Result<(), CapabilityError>> {
        Resources.release(scope, handle)
    }
    fn limits(&self) -> CapabilityLimits {
        let mut l = Resources.limits();
        l.read_bytes = self.read_cap;
        l
    }
}
fn completed(uri: bool) -> serde_json::Value {
    let video = if uri {
        json!({"uri":"https://private.google.test/result"})
    } else {
        json!({"encodedVideo":base64::engine::general_purpose::STANDARD.encode(mp4()),"encoding":"video/mp4"})
    };
    json!({"name":"operations/a","done":true,"response":{"generateVideoResponse":{"generatedSamples":[{"video":video}]}}})
}
#[test]
fn completed_private_uri_is_read_in_scope_and_published_before_exposure() {
    let host = host(vec![completed(true)]);
    let store = store();
    let resources = videos();
    let mut progress = VideoProgress::default();
    let result = ready(video::openai_to_gemini_create_composed(
        &host,
        &(),
        &resources,
        &(),
        &store,
        &(),
        template("/v1beta/models/veo:predictLongRunning"),
        input(),
        binding("client"),
        expiry(),
        &mut progress,
        limits(),
    ))
    .unwrap();
    let JsonInvocation::Success(r) = result else {
        panic!()
    };
    assert_eq!(
        r.body.value.unsigned_urls,
        Some(vec!["https://public.test/0.mp4".into()])
    );
    assert_eq!(r.body.value.id, "client");
    assert_eq!(
        resources.read_calls.lock().unwrap().as_slice(),
        [ResourceReference::Url(
            "https://private.google.test/result".into()
        )]
    );
    assert!(progress.binding_saved);
    assert_eq!(progress.publications.len(), 1);
    let saved: video::VideoJobState =
        serde_json::from_slice(&store.entry.lock().unwrap().as_ref().unwrap().payload).unwrap();
    assert_eq!(
        saved
            .published_urls
            .get("https://private.google.test/result")
            .unwrap(),
        "https://public.test/0.mp4"
    );
    assert_eq!(
        saved
            .operation
            .unwrap()
            .response
            .unwrap()
            .generate_video_response
            .unwrap()
            .generated_samples
            .unwrap()[0]
            .video
            .as_ref()
            .unwrap()
            .uri
            .as_deref(),
        Some("https://private.google.test/result")
    );
}
#[test]
fn publication_lost_result_is_recovered_without_recreating_video() {
    let host = host(vec![completed(false)]);
    let store = store();
    let resources = videos();
    resources
        .fail_after_publish
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let mut progress = VideoProgress::default();
    assert!(
        ready(video::openai_to_gemini_create_composed(
            &host,
            &(),
            &resources,
            &(),
            &store,
            &(),
            template("/v1beta/models/veo:predictLongRunning"),
            input(),
            binding("client"),
            expiry(),
            &mut progress,
            limits()
        ))
        .is_err()
    );
    assert_eq!(progress.publication_ids.len(), 1);
    assert!(matches!(
        ready(resources.publication_status(&(), &progress.publication_ids[0])).unwrap(),
        PublicationStatus::Published(_)
    ));
    let result = ready(video::recover_created_result(
        &resources,
        &(),
        &store,
        &(),
        binding("client"),
        expiry(),
        &mut progress,
        limits(),
    ))
    .unwrap();
    let JsonInvocation::Success(r) = result else {
        panic!()
    };
    assert_eq!(r.body.value.status, o::VideoStatus::Completed);
    assert_eq!(host.sent.lock().unwrap().len(), 1);
    assert_eq!(resources.published.lock().unwrap().len(), 1);
}
#[test]
fn retained_creation_can_complete_after_binding_cas_failure() {
    let host = host(vec![json!({"name":"operations/a","done":false})]);
    let mut store = store();
    store.fail_after = Some(2);
    let mut progress = VideoProgress::default();
    assert!(create(&host, &store, &mut progress).is_err());
    let JsonInvocation::Success(r) = ready(video::recover_created_result(
        &Resources,
        &(),
        &store,
        &(),
        binding("client"),
        expiry(),
        &mut progress,
        limits(),
    ))
    .unwrap() else {
        panic!()
    };
    assert_eq!(r.body.value.id, "client");
    assert_eq!(host.sent.lock().unwrap().len(), 1);
    assert!(
        ready(video::recover_created_result(
            &Resources,
            &(),
            &store,
            &(),
            binding("different"),
            expiry(),
            &mut progress,
            limits()
        ))
        .is_err()
    );
}
#[test]
fn host_resource_read_limit_is_enforced_and_native_result_remains_recoverable() {
    let host = host(vec![completed(true)]);
    let store = store();
    let mut resources = videos();
    resources.read_cap = 4;
    let mut progress = VideoProgress::default();
    let error = ready(video::openai_to_gemini_create_composed(
        &host,
        &(),
        &resources,
        &(),
        &store,
        &(),
        template("/v1beta/models/veo:predictLongRunning"),
        input(),
        binding("client"),
        expiry(),
        &mut progress,
        limits(),
    ))
    .unwrap_err();
    assert_eq!(
        error.kind(),
        gproxy_protocol::transform::TransformErrorKind::Limit
    );
    assert!(progress.native_response.is_some());
    assert!(progress.binding_saved);
    assert!(resources.published.lock().unwrap().is_empty());
}
struct PausedHost(Mutex<usize>);
impl Upstream for PausedHost {
    type Target = ();
    fn send<'a>(
        &'a self,
        _: &'a (),
        _: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            *self.0.lock().unwrap() += 1;
            std::future::pending().await
        })
    }
    fn connect<'a>(
        &'a self,
        _: &'a (),
        _: WireRequest<()>,
    ) -> CapabilityFuture<'a, Result<UpstreamConnection, CapabilityError>> {
        panic!()
    }
    fn limits(&self) -> CapabilityLimits {
        Resources.limits()
    }
}
#[test]
fn dropping_inflight_create_keeps_reservation_and_never_authorizes_a_retry() {
    let host = PausedHost(Mutex::new(0));
    let store = store();
    let mut progress = VideoProgress::default();
    let mut future = Box::pin(video::openai_to_gemini_create_composed(
        &host,
        &(),
        &Resources,
        &(),
        &store,
        &(),
        template("/v1beta/models/veo:predictLongRunning"),
        input(),
        binding("client"),
        expiry(),
        &mut progress,
        limits(),
    ));
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(std::future::Future::poll(future.as_mut(), &mut cx).is_pending());
    drop(future);
    assert!(progress.send_started);
    assert!(progress.reserved);
    assert!(progress.native_response.is_none());
    assert!(
        ready(video::openai_to_gemini_create_composed(
            &host,
            &(),
            &Resources,
            &(),
            &store,
            &(),
            template("/v1beta/models/veo:predictLongRunning"),
            input(),
            binding("client"),
            expiry(),
            &mut VideoProgress::default(),
            limits()
        ))
        .is_err()
    );
    assert_eq!(*host.0.lock().unwrap(), 1);
}
#[test]
fn aggregate_output_resource_limit_retains_first_publication_and_native_result() {
    let mut value = completed(true);
    value["response"]["generateVideoResponse"]["generatedSamples"] = json!([{"video":{"uri":"https://private.test/1"}},{"video":{"uri":"https://private.test/2"}}]);
    let host = host(vec![value]);
    let store = store();
    let mut resources = videos();
    resources.body.resize(5000, 0);
    let mut progress = VideoProgress::default();
    let mut l = limits();
    l.codec.max_body_bytes = 8192;
    let error = ready(video::openai_to_gemini_create_composed(
        &host,
        &(),
        &resources,
        &(),
        &store,
        &(),
        template("/v1beta/models/veo:predictLongRunning"),
        input(),
        binding("client"),
        expiry(),
        &mut progress,
        l,
    ))
    .unwrap_err();
    assert_eq!(
        error.kind(),
        gproxy_protocol::transform::TransformErrorKind::Limit
    );
    assert_eq!(progress.publications.len(), 1);
    assert_eq!(resources.published.lock().unwrap().len(), 1);
    assert!(progress.native_response.is_some());
}
#[test]
fn query_diagnostic_is_separate_and_completed_optional_metadata_can_refresh() {
    let mut first = completed(false);
    first["metadata"] = json!({"note":"first"});
    let mut second = first.clone();
    second["metadata"] = json!({"note":"updated"});
    let host = host(vec![first, second]);
    let store = store();
    let resources = videos();
    let JsonInvocation::Success(first) = ready(video::openai_to_gemini_create_composed(
        &host,
        &(),
        &resources,
        &(),
        &store,
        &(),
        template("/v1beta/models/veo:predictLongRunning"),
        input(),
        binding("client"),
        expiry(),
        &mut VideoProgress::default(),
        limits(),
    ))
    .unwrap() else {
        panic!()
    };
    assert!(!first.body.report.diagnostics.is_empty());
    let mut t = template("/v1beta/operations/a");
    t.method = http::Method::GET;
    let JsonInvocation::Success(second) = ready(video::openai_to_gemini_query_composed(
        &host,
        &(),
        &resources,
        &(),
        &store,
        &(),
        t,
        binding("client"),
        expiry(),
        &mut VideoProgress::default(),
        limits(),
    ))
    .unwrap() else {
        panic!()
    };
    assert_eq!(first.body.value, second.body.value);
    assert_eq!(resources.published.lock().unwrap().len(), 1);
}
#[test]
fn aggregate_factual_resource_bound_is_checked_before_send() {
    use gproxy_protocol::transform::video::ResolvedVideoResource;
    let host = host(vec![]);
    let facts = (0..3)
        .map(|i| {
            let reference = format!("r{i}");
            (
                reference.clone(),
                ResolvedVideoResource {
                    reference,
                    url: None,
                    bytes_base64_encoded: Some("x".repeat(500)),
                    mime_type: Some("image/png".into()),
                },
            )
        })
        .collect();
    let mut l = limits();
    l.codec.max_body_bytes = 1000;
    let e = ready(video::openai_to_gemini_create(
        &host,
        &(),
        template("/v1beta/models/veo:predictLongRunning"),
        input(),
        "veo",
        facts,
        context(),
        l,
    ))
    .unwrap_err();
    assert_eq!(
        e.kind(),
        gproxy_protocol::transform::TransformErrorKind::Limit
    );
    assert!(host.sent.lock().unwrap().is_empty());
}
fn native_input() -> o::NativeCreateVideoRequestBody {
    serde_json::from_value(
        json!({"model":"sora-2","prompt":"sunset","seconds":"4","size":"1280x720"}),
    )
    .unwrap()
}
fn native_facts(
    source: &gproxy_protocol::wire::gemini::video::VideoOperation,
) -> Result<
    gproxy_protocol::transform::video::NativeVideoResponseFacts,
    gproxy_protocol::transform::TransformError,
> {
    use gproxy_protocol::transform::video::{NativePendingStatus, NativeVideoResponseFacts};
    Ok(NativeVideoResponseFacts {
        operation_name: source.name.clone().unwrap(),
        client_id: "client".into(),
        created_at: 123,
        model: "sora-2".into(),
        seconds: o::NativeVideoSeconds::Four,
        size: o::NativeVideoSize::Landscape720,
        progress: if source.done == Some(true) { 100 } else { 0 },
        pending_status: NativePendingStatus::InProgress,
        completed_at: None,
        expires_at: None,
        prompt: Some("sunset".into()),
        failure_code: None,
    })
}
#[test]
fn native_sora_create_and_query_preserve_bound_identity_then_download_actual_video() {
    let host = host(vec![
        json!({"name":"operations/a","done":false}),
        completed(true),
    ]);
    let store = store();
    let mut progress = VideoProgress::default();
    let JsonInvocation::Success(created) = ready(video::native_to_gemini_create_composed(
        &host,
        &(),
        &Resources,
        &(),
        &store,
        &(),
        template("/v1beta/models/veo:predictLongRunning"),
        native_input(),
        None,
        binding("client"),
        native_facts,
        &mut progress,
        limits(),
    ))
    .unwrap() else {
        panic!()
    };
    assert_eq!(created.body.value.id, "client");
    assert_eq!(created.body.value.created_at, 123);
    assert_eq!(created.body.value.status, o::NativeVideoStatus::InProgress);
    let resources = videos();
    assert!(
        ready(video::native_content(
            &resources,
            &(),
            &store,
            &(),
            binding("client"),
            limits()
        ))
        .is_err()
    );
    let mut t = template("/v1beta/operations/a");
    t.method = http::Method::GET;
    let JsonInvocation::Success(done) = ready(video::native_to_gemini_query_composed(
        &host,
        &(),
        &store,
        &(),
        t,
        binding("client"),
        native_facts,
        &mut VideoProgress::default(),
        limits(),
    ))
    .unwrap() else {
        panic!()
    };
    assert_eq!(done.body.value.id, "client");
    assert_eq!(done.body.value.status, o::NativeVideoStatus::Completed);
    let content = ready(video::native_content(
        &resources,
        &(),
        &store,
        &(),
        binding("client"),
        limits(),
    ))
    .unwrap();
    let HttpBody::Bytes(bytes) = content.body else {
        panic!()
    };
    assert_eq!(bytes.as_ref(), mp4());
    assert_eq!(
        resources.read_calls.lock().unwrap().as_slice(),
        [ResourceReference::Url(
            "https://private.google.test/result".into()
        )]
    );
    assert_eq!(host.sent.lock().unwrap().len(), 2);
}
#[test]
fn native_metadata_failure_recovers_actual_result_without_second_create() {
    let host = host(vec![completed(false)]);
    let store = store();
    let mut progress = VideoProgress::default();
    let failed = ready(video::native_to_gemini_create_composed(
        &host,
        &(),
        &Resources,
        &(),
        &store,
        &(),
        template("/v1beta/models/veo:predictLongRunning"),
        native_input(),
        None,
        binding("client"),
        |_| Err(gproxy_protocol::transform::TransformError::missing_metadata("caller.progress")),
        &mut progress,
        limits(),
    ));
    assert!(failed.is_err());
    assert!(progress.native_response.is_some());
    let JsonInvocation::Success(done) = ready(video::recover_native_result(
        &store,
        &(),
        binding("client"),
        native_facts,
        &mut progress,
        limits(),
    ))
    .unwrap() else {
        panic!()
    };
    assert_eq!(done.body.value.status, o::NativeVideoStatus::Completed);
    let content = ready(video::native_content(
        &Resources,
        &(),
        &store,
        &(),
        binding("client"),
        limits(),
    ))
    .unwrap();
    let HttpBody::Bytes(bytes) = content.body else {
        panic!()
    };
    assert_eq!(bytes.as_ref(), mp4());
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}
#[test]
fn native_results_cannot_reinterpret_a_different_source_contract() {
    let host = host(vec![json!({"name":"operations/a","done":false})]);
    let store = store();
    create(&host, &store, &mut VideoProgress::default()).unwrap();
    let mut t = template("/v1beta/operations/a");
    t.method = http::Method::GET;
    assert!(
        ready(video::native_to_gemini_query_composed(
            &host,
            &(),
            &store,
            &(),
            t,
            binding("client"),
            native_facts,
            &mut VideoProgress::default(),
            limits()
        ))
        .is_err()
    );
    assert!(
        ready(video::native_content(
            &Resources,
            &(),
            &store,
            &(),
            binding("client"),
            limits()
        ))
        .is_err()
    );
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}
#[test]
fn native_return_facts_must_match_effective_duration_size_and_client_model() {
    for field in ["seconds", "size", "model", "id"] {
        let host = host(vec![json!({"name":"operations/a","done":false})]);
        let store = store();
        let mut progress = VideoProgress::default();
        assert!(
            ready(video::native_to_gemini_create_composed(
                &host,
                &(),
                &Resources,
                &(),
                &store,
                &(),
                template("/v1beta/models/veo:predictLongRunning"),
                native_input(),
                None,
                binding("client"),
                |source| {
                    let mut facts = native_facts(source)?;
                    match field {
                        "seconds" => facts.seconds = o::NativeVideoSeconds::Eight,
                        "size" => facts.size = o::NativeVideoSize::Portrait720,
                        "model" => facts.model = "different".into(),
                        _ => facts.client_id = "different".into(),
                    };
                    Ok(facts)
                },
                &mut progress,
                limits()
            ))
            .is_err(),
            "{field}"
        );
        assert!(progress.native_response.is_some());
        assert_eq!(host.sent.lock().unwrap().len(), 1);
    }
}
#[test]
fn native_create_reference_file_is_read_using_resource_id() {
    struct ImageAccess(Mutex<Vec<ResourceReference>>);
    impl ResourceAccess for ImageAccess {
        type Scope = ();
        type PublishedHandle = ();
        fn resolve<'a>(
            &'a self,
            s: &'a (),
            r: &'a ResourceReference,
        ) -> CapabilityFuture<'a, Result<ResourceMetadata, CapabilityError>> {
            Resources.resolve(s, r)
        }
        fn read<'a>(
            &'a self,
            s: &'a (),
            r: &'a ResourceReference,
        ) -> CapabilityFuture<'a, Result<ResourceRead, CapabilityError>> {
            self.0.lock().unwrap().push(r.clone());
            Resources.read(s, r)
        }
        fn publish<'a>(
            &'a self,
            s: &'a (),
            id: &'a str,
            k: PublicationKind,
            m: ResourceMetadata,
            b: HttpBody,
        ) -> CapabilityFuture<'a, Result<PublishedResource<()>, CapabilityError>> {
            Resources.publish(s, id, k, m, b)
        }
        fn publication_status<'a>(
            &'a self,
            s: &'a (),
            id: &'a str,
        ) -> CapabilityFuture<'a, Result<PublicationStatus<()>, CapabilityError>> {
            Resources.publication_status(s, id)
        }
        fn release<'a>(
            &'a self,
            s: &'a (),
            h: &'a (),
        ) -> CapabilityFuture<'a, Result<(), CapabilityError>> {
            Resources.release(s, h)
        }
        fn limits(&self) -> CapabilityLimits {
            Resources.limits()
        }
    }
    let access = ImageAccess(Mutex::default());
    let host = host(vec![json!({"name":"operations/a","done":false})]);
    let store = store();
    let mut input = native_input();
    input.input_reference = Some(serde_json::from_value(json!({"file_id":"file_actual"})).unwrap());
    ready(video::native_to_gemini_create_composed(
        &host,
        &(),
        &access,
        &(),
        &store,
        &(),
        template("/v1beta/models/veo:predictLongRunning"),
        input,
        None,
        binding("client"),
        native_facts,
        &mut VideoProgress::default(),
        limits(),
    ))
    .unwrap();
    assert_eq!(
        access.0.lock().unwrap().as_slice(),
        [ResourceReference::Id("file_actual".into())]
    );
    let sent = host.sent.lock().unwrap();
    let HttpBody::Bytes(bytes) = &sent[0].body else {
        panic!()
    };
    let body: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    assert_eq!(body["instances"][0]["image"]["bytesBase64Encoded"], PNG);
}
#[test]
fn composed_create_model_path_is_checked_before_reserving_or_sending() {
    let host = host(vec![]);
    let store = store();
    let mut progress = VideoProgress::default();
    assert!(
        ready(video::openai_to_gemini_create_composed(
            &host,
            &(),
            &Resources,
            &(),
            &store,
            &(),
            template("/v1beta/models/other:predictLongRunning"),
            input(),
            binding("client"),
            expiry(),
            &mut progress,
            limits()
        ))
        .is_err()
    );
    assert!(host.sent.lock().unwrap().is_empty());
    assert!(store.entry.lock().unwrap().is_none());
    assert!(!progress.reserved);
}
/// Job state used to be written without an expiry and so was kept forever.
/// Every write now sets one a week out, and a poll pushes it out again.
#[test]
fn job_state_expires_a_week_after_its_latest_write() {
    let week = video::VIDEO_STATE_TTL;
    assert_eq!(week, Duration::from_secs(7 * 24 * 60 * 60));
    let expiry = |store: &Store| {
        store
            .entry
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .expires_at
            .expect("job state carries an expiry")
    };
    let host = host(vec![
        json!({"name":"operations/a","done":false}),
        json!({"name":"operations/a","done":false}),
    ]);
    let store = store();
    let before = std::time::SystemTime::now();
    create(&host, &store, &mut VideoProgress::default()).unwrap();
    let created = expiry(&store);
    assert!(created >= before + week);
    assert!(created <= std::time::SystemTime::now() + week);
    query(&host, &store, "/v1beta/operations/a", binding("client")).unwrap();
    assert!(expiry(&store) >= created);
}
