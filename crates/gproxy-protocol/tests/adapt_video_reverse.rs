use gproxy_protocol::{
    HttpBody, WireRequest, WireResponse,
    adapt::{
        JsonInvocation,
        video::{self, ReverseVideoBinding, ReverseVideoKind, ReverseVideoProgress, VideoLimits},
    },
    capability::{
        CapabilityError, CapabilityFuture, CapabilityLimits, CasResult, PublicationKind,
        PublicationStatus, PublishedResource, ResourceAccess, ResourceMetadata, ResourceRead,
        ResourceReference, StateEntry, StateStore, StateWrite, Upstream, UpstreamConnection,
        Version,
    },
    codec::CodecLimits,
    connection::{Bytes, HeaderMap, StatusCode},
    wire::gemini::video as g,
};
use serde_json::{Value, json};
use std::{
    sync::Mutex,
    time::{Duration, SystemTime},
};
const PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";
fn ready<F: std::future::Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    match future.as_mut().poll(&mut cx) {
        std::task::Poll::Ready(v) => v,
        _ => panic!("unexpected pending"),
    }
}
fn expiry() -> SystemTime {
    SystemTime::now() + Duration::from_secs(600)
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
            let response = {
                let mut values = self.responses.lock().unwrap();
                if values.is_empty() {
                    None
                } else {
                    Some(values.remove(0))
                }
            };
            if let Some(r) = response {
                Ok(r)
            } else {
                std::future::pending().await
            }
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
        caps()
    }
}
fn caps() -> CapabilityLimits {
    CapabilityLimits {
        operation_total: Duration::from_secs(30),
        stream_idle: Duration::from_secs(5),
        read_bytes: 65536,
        write_bytes: 65536,
        ws_frame_bytes: 1024,
    }
}
#[derive(Default)]
struct Resources {
    reads: Mutex<Vec<(String, ResourceReference)>>,
    published: Mutex<Vec<(String, String)>>,
    fail_at: Option<usize>,
}
impl ResourceAccess for Resources {
    type Scope = String;
    type PublishedHandle = ();
    fn resolve<'a>(
        &'a self,
        _: &'a String,
        _: &'a ResourceReference,
    ) -> CapabilityFuture<'a, Result<ResourceMetadata, CapabilityError>> {
        panic!("unused")
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
                .push((scope.into(), reference.clone()));
            let bytes = Bytes::from_static(b"\0\0\0\x18ftypmp42");
            Ok(ResourceRead {
                metadata: ResourceMetadata {
                    mime: Some("video/mp4".into()),
                    length: Some(bytes.len() as u64),
                    filename: None,
                    expires_at: None,
                },
                body: HttpBody::Bytes(bytes),
            })
        })
    }
    fn publish<'a>(
        &'a self,
        scope: &'a String,
        id: &'a str,
        _: PublicationKind,
        metadata: ResourceMetadata,
        body: HttpBody,
    ) -> CapabilityFuture<'a, Result<PublishedResource<()>, CapabilityError>> {
        Box::pin(async move {
            let n = {
                let mut values = self.published.lock().unwrap();
                values.push((scope.into(), id.into()));
                values.len()
            };
            let _ = body;
            if self.fail_at == Some(n) {
                return Err(CapabilityError::new(
                    gproxy_protocol::capability::CapabilityErrorKind::Transport,
                    gproxy_protocol::capability::CapabilityErrorStage::Start,
                    "publication failure",
                ));
            }
            Ok(PublishedResource {
                handle: (),
                reference: ResourceReference::Url(format!("https://cdn.test/{id}")),
                metadata,
            })
        })
    }
    fn publication_status<'a>(
        &'a self,
        _: &'a String,
        _: &'a str,
    ) -> CapabilityFuture<'a, Result<PublicationStatus<()>, CapabilityError>> {
        Box::pin(async { Ok(PublicationStatus::Missing) })
    }
    fn release<'a>(
        &'a self,
        _: &'a String,
        _: &'a (),
    ) -> CapabilityFuture<'a, Result<(), CapabilityError>> {
        Box::pin(async { Ok(()) })
    }
    fn limits(&self) -> CapabilityLimits {
        caps()
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
        max_resource_facts: 8,
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

fn store() -> Store {
    Store {
        entry: Mutex::new(None),
        fail_cas: false,
        fail_after: None,
        attempts: Mutex::new(0),
    }
}
fn response(value: Value) -> WireResponse<HttpBody> {
    WireResponse {
        status: StatusCode::OK,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&value).unwrap())),
    }
}
fn host(values: Vec<Value>) -> Host {
    Host {
        responses: Mutex::new(values.into_iter().map(response).collect()),
        sent: Mutex::default(),
    }
}
fn binding(kind: ReverseVideoKind) -> ReverseVideoBinding {
    ReverseVideoBinding {
        operation_name: "operations/proxy".into(),
        origin: "selected-account".into(),
        model: "sora-test".into(),
        kind,
        api_origin_url: "https://upstream.test".into(),
        create_path: "/v1/videos".into(),
        query_prefix: "/v1/videos/".into(),
    }
}
fn input(instances: usize, samples: usize) -> g::PredictLongRunningRequestBody {
    serde_json::from_value(json!({"instances":(0..instances).map(|i|json!({"prompt":format!("prompt-{i}")})).collect::<Vec<_>>(),"parameters":{"sampleCount":samples,"durationSeconds":4,"resolution":"720p","aspectRatio":"16:9"}})).unwrap()
}
fn native(id: &str, status: &str) -> Value {
    json!({"id":id,"object":"video","model":"sora-test","created_at":100,"progress":if status=="completed"{100}else{20},"seconds":"4","size":"1280x720","status":status})
}
fn router(id: &str, status: &str) -> Value {
    json!({"id":id,"status":status,"polling_url":format!("https://upstream.test/v1/videos/{id}")})
}
fn create(
    host: &Host,
    resources: &Resources,
    store: &Store,
    input: g::PredictLongRunningRequestBody,
    kind: ReverseVideoKind,
    expiry: SystemTime,
    progress: &mut ReverseVideoProgress,
) -> Result<
    JsonInvocation<gproxy_protocol::transform::Converted<g::VideoOperation>>,
    gproxy_protocol::transform::TransformError,
> {
    ready(video::gemini_video_create_composed(
        host,
        &(),
        resources,
        &"source".into(),
        &"target".into(),
        &"publication".into(),
        store,
        &(),
        template("/v1/videos"),
        input,
        binding(kind),
        None,
        None,
        expiry,
        progress,
        limits(),
    ))
}
fn resume(
    host: &Host,
    resources: &Resources,
    store: &Store,
    kind: ReverseVideoKind,
    expiry: SystemTime,
    progress: &mut ReverseVideoProgress,
) -> Result<
    JsonInvocation<gproxy_protocol::transform::Converted<g::VideoOperation>>,
    gproxy_protocol::transform::TransformError,
> {
    ready(video::gemini_video_resume_composed(
        host,
        &(),
        resources,
        &"source".into(),
        &"target".into(),
        &"publication".into(),
        store,
        &(),
        template("/v1/videos"),
        binding(kind),
        None,
        expiry,
        progress,
        limits(),
    ))
}
#[allow(clippy::too_many_arguments)]
fn query(
    host: &Host,
    resources: &Resources,
    store: &Store,
    kind: ReverseVideoKind,
    index: usize,
    id: &str,
    expiry: SystemTime,
    progress: &mut ReverseVideoProgress,
) -> Result<
    JsonInvocation<gproxy_protocol::transform::Converted<g::VideoOperation>>,
    gproxy_protocol::transform::TransformError,
> {
    let mut request = template(&format!("/v1/videos/{id}"));
    request.method = http::Method::GET;
    ready(video::gemini_video_query_composed(
        host,
        &(),
        resources,
        &"target".into(),
        &"publication".into(),
        store,
        &(),
        request,
        binding(kind),
        index,
        expiry,
        progress,
        limits(),
    ))
}
fn body(
    result: JsonInvocation<gproxy_protocol::transform::Converted<g::VideoOperation>>,
) -> g::VideoOperation {
    let JsonInvocation::Success(response) = result else {
        panic!("unexpected rejection")
    };
    response.body.value
}
#[test]
fn native_fanout_preserves_instance_sample_order_and_publishes_real_content() {
    let host = host(
        (0..4)
            .map(|i| native(&format!("video_{i}"), "completed"))
            .collect(),
    );
    let resources = Resources::default();
    let store = store();
    let mut progress = ReverseVideoProgress::default();
    let output = body(
        create(
            &host,
            &resources,
            &store,
            input(2, 2),
            ReverseVideoKind::Native,
            expiry(),
            &mut progress,
        )
        .unwrap(),
    );
    assert_eq!(output.done, Some(true));
    let samples = output
        .response
        .unwrap()
        .generate_video_response
        .unwrap()
        .generated_samples
        .unwrap();
    assert_eq!(samples.len(), 4);
    for (i, sample) in samples.iter().enumerate() {
        assert!(
            sample
                .video
                .as_ref()
                .unwrap()
                .uri
                .as_ref()
                .unwrap()
                .ends_with(&format!("/output/{i}/0"))
        );
    }
    let sent = host.sent.lock().unwrap();
    assert_eq!(sent.len(), 4);
    for (i, r) in sent.iter().enumerate() {
        let HttpBody::Bytes(bytes) = &r.body else {
            panic!()
        };
        let value: Value = serde_json::from_slice(bytes).unwrap();
        assert_eq!(value["prompt"], format!("prompt-{}", i / 2));
        assert_eq!(value["seconds"], "4");
    }
    assert_eq!(
        resources.reads.lock().unwrap()[0],
        (
            "target".into(),
            ResourceReference::Url("https://upstream.test/v1/videos/video_0/content".into())
        )
    );
    assert!(
        resources
            .published
            .lock()
            .unwrap()
            .iter()
            .all(|(s, _)| s == "publication")
    );
    assert!(
        progress
            .state
            .as_ref()
            .unwrap()
            .children
            .iter()
            .all(|c| c.started && c.result.is_some() && c.published.is_some())
    );
}
#[test]
fn router_fanout_keeps_private_polling_and_publishes_output_urls() {
    let mut a = router("job_a", "completed");
    a["unsigned_urls"] = json!(["https://private.test/a"]);
    let mut b = router("job_b", "completed");
    b["unsigned_urls"] = json!(["https://private.test/b"]);
    let host = host(vec![a, b]);
    let resources = Resources::default();
    let store = store();
    let mut p = ReverseVideoProgress::default();
    let out = body(
        create(
            &host,
            &resources,
            &store,
            input(1, 2),
            ReverseVideoKind::OpenRouter,
            expiry(),
            &mut p,
        )
        .unwrap(),
    );
    assert_eq!(out.done, Some(true));
    assert_eq!(host.sent.lock().unwrap().len(), 2);
    assert_eq!(resources.reads.lock().unwrap().len(), 2);
}
#[test]
fn duplicate_group_is_rejected_before_resource_publication_or_post() {
    let host = host(vec![native("a", "queued")]);
    let resources = Resources::default();
    let store = store();
    let exp = expiry();
    create(
        &host,
        &resources,
        &store,
        input(1, 1),
        ReverseVideoKind::Native,
        exp,
        &mut ReverseVideoProgress::default(),
    )
    .unwrap();
    assert!(
        create(
            &host,
            &resources,
            &store,
            input(1, 1),
            ReverseVideoKind::Native,
            exp,
            &mut ReverseVideoProgress::default()
        )
        .is_err()
    );
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}
#[test]
fn dropped_second_post_leaves_uncertain_child_and_never_recreates() {
    let host = host(vec![native("a", "queued")]);
    let resources = Resources::default();
    let store = store();
    let exp = expiry();
    let mut p = ReverseVideoProgress::default();
    let source_scope = "source".to_owned();
    let target_scope = "target".to_owned();
    let publish_scope = "publication".to_owned();
    let mut future = Box::pin(video::gemini_video_create_composed(
        &host,
        &(),
        &resources,
        &source_scope,
        &target_scope,
        &publish_scope,
        &store,
        &(),
        template("/v1/videos"),
        input(1, 2),
        binding(ReverseVideoKind::Native),
        None,
        None,
        exp,
        &mut p,
        limits(),
    ));
    let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(future.as_mut().poll(&mut cx).is_pending());
    drop(future);
    assert_eq!(p.child_index, Some(1));
    assert!(p.send_started);
    assert!(p.state.as_ref().unwrap().children[0].result.is_some());
    assert!(
        resume(
            &host,
            &resources,
            &store,
            ReverseVideoKind::Native,
            exp,
            &mut ReverseVideoProgress::default()
        )
        .is_err()
    );
    assert_eq!(host.sent.lock().unwrap().len(), 2);
}
#[test]
fn post_cas_failure_recovers_actual_result_then_resumes_only_unsent_child() {
    let host = host(vec![native("a", "queued"), native("b", "queued")]);
    let resources = Resources::default();
    let mut store = store();
    store.fail_after = Some(4);
    let exp = expiry();
    let mut p = ReverseVideoProgress::default();
    assert!(
        create(
            &host,
            &resources,
            &store,
            input(1, 2),
            ReverseVideoKind::Native,
            exp,
            &mut p
        )
        .is_err()
    );
    assert!(p.result.is_some());
    assert_eq!(host.sent.lock().unwrap().len(), 1);
    ready(video::recover_reverse_video_result(
        &store,
        &(),
        &binding(ReverseVideoKind::Native),
        &mut p,
        limits(),
    ))
    .unwrap();
    let out = body(
        resume(
            &host,
            &resources,
            &store,
            ReverseVideoKind::Native,
            exp,
            &mut p,
        )
        .unwrap(),
    );
    assert_eq!(out.done, Some(false));
    assert_eq!(host.sent.lock().unwrap().len(), 2);
}
#[test]
fn output_publication_failure_recovers_without_repeating_create() {
    let host = host(vec![native("a", "completed")]);
    let resources = Resources {
        fail_at: Some(1),
        ..Default::default()
    };
    let store = store();
    let exp = expiry();
    let mut p = ReverseVideoProgress::default();
    assert!(
        create(
            &host,
            &resources,
            &store,
            input(1, 1),
            ReverseVideoKind::Native,
            exp,
            &mut p
        )
        .is_err()
    );
    assert!(p.state.as_ref().unwrap().children[0].result.is_some());
    let id = p.publication_ids[0].clone();
    let out = body(
        resume(
            &host,
            &resources,
            &store,
            ReverseVideoKind::Native,
            exp,
            &mut p,
        )
        .unwrap(),
    );
    assert_eq!(out.done, Some(true));
    assert_eq!(host.sent.lock().unwrap().len(), 1);
    assert_eq!(resources.published.lock().unwrap()[1].1, id);
}
#[test]
fn selected_child_query_uses_saved_id_and_keeps_group_pending_until_all_finish() {
    let host = host(vec![
        native("a", "queued"),
        native("b", "queued"),
        native("a", "completed"),
        native("b", "completed"),
    ]);
    let resources = Resources::default();
    let store = store();
    let exp = expiry();
    let mut p = ReverseVideoProgress::default();
    create(
        &host,
        &resources,
        &store,
        input(1, 2),
        ReverseVideoKind::Native,
        exp,
        &mut p,
    )
    .unwrap();
    assert!(
        query(
            &host,
            &resources,
            &store,
            ReverseVideoKind::Native,
            0,
            "wrong",
            exp,
            &mut p
        )
        .is_err()
    );
    assert_eq!(host.sent.lock().unwrap().len(), 2);
    assert_eq!(
        body(
            query(
                &host,
                &resources,
                &store,
                ReverseVideoKind::Native,
                0,
                "a",
                exp,
                &mut p
            )
            .unwrap()
        )
        .done,
        Some(false)
    );
    assert_eq!(
        body(
            query(
                &host,
                &resources,
                &store,
                ReverseVideoKind::Native,
                1,
                "b",
                exp,
                &mut p
            )
            .unwrap()
        )
        .done,
        Some(true)
    );
    assert_eq!(resources.reads.lock().unwrap().len(), 2);
}
#[test]
fn mixed_child_failure_does_not_claim_partial_success_is_complete() {
    let mut failed = native("b", "failed");
    failed["error"] = json!({"code":"provider_error","message":"actual failure"});
    let host = host(vec![native("a", "completed"), failed]);
    let resources = Resources::default();
    let store = store();
    let mut p = ReverseVideoProgress::default();
    let out = body(
        create(
            &host,
            &resources,
            &store,
            input(1, 2),
            ReverseVideoKind::Native,
            expiry(),
            &mut p,
        )
        .unwrap(),
    );
    assert_eq!(out.done, Some(true));
    assert!(out.response.is_none());
    assert!(
        out.error
            .unwrap()
            .message
            .unwrap()
            .contains("actual failure")
    );
    assert!(p.state.unwrap().children[0].published.is_some());
}
#[test]
fn image_is_published_before_create_and_only_declared_fields_are_saved() {
    let host = host(vec![native("a", "queued")]);
    let resources = Resources::default();
    let store = store();
    let mut source = input(1, 1);
    source.instances[0].image = Some(
        serde_json::from_value(
            json!({"bytesBase64Encoded":PNG,"mimeType":"image/png","unknown":"sentinel"}),
        )
        .unwrap(),
    );
    source.rest.insert("unknown".into(), json!("sentinel"));
    let mut p = ReverseVideoProgress::default();
    create(
        &host,
        &resources,
        &store,
        source,
        ReverseVideoKind::Native,
        expiry(),
        &mut p,
    )
    .unwrap();
    assert_eq!(resources.published.lock().unwrap().len(), 1);
    let sent = host.sent.lock().unwrap();
    let HttpBody::Bytes(bytes) = &sent[0].body else {
        panic!()
    };
    let value: Value = serde_json::from_slice(bytes).unwrap();
    assert!(
        value["input_reference"]["image_url"]
            .as_str()
            .unwrap()
            .starts_with("https://cdn.test/")
    );
    assert!(
        !String::from_utf8(
            store
                .entry
                .lock()
                .unwrap()
                .as_ref()
                .unwrap()
                .payload
                .to_vec()
        )
        .unwrap()
        .contains("sentinel")
    );
}
#[test]
fn unsupported_native_control_and_fanout_limit_reject_before_cas_or_publication() {
    let host = host(vec![native("video_0", "queued")]);
    let resources = Resources::default();
    let store = store();
    let mut source = input(1, 1);
    source.parameters.as_mut().unwrap().negative_prompt = Some("avoid".into());
    assert!(
        create(
            &host,
            &resources,
            &store,
            source,
            ReverseVideoKind::Native,
            expiry(),
            &mut ReverseVideoProgress::default()
        )
        .is_ok()
    );
    assert!(
        create(
            &host,
            &resources,
            &store,
            input(3, 3),
            ReverseVideoKind::Native,
            expiry(),
            &mut ReverseVideoProgress::default()
        )
        .is_err()
    );
    assert_eq!(host.sent.lock().unwrap().len(), 1);
    assert!(resources.published.lock().unwrap().is_empty());
}

#[test]
fn malformed_json_retains_raw_and_blocks_recreation() {
    let host = host(vec![]);
    host.responses.lock().unwrap().push(WireResponse {
        status: StatusCode::OK,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from_static(b"not json")),
    });
    let resources = Resources::default();
    let store = store();
    let exp = expiry();
    let mut p = ReverseVideoProgress::default();
    assert!(
        create(
            &host,
            &resources,
            &store,
            input(1, 1),
            ReverseVideoKind::Native,
            exp,
            &mut p
        )
        .is_err()
    );
    assert_eq!(p.raw_response.as_ref().unwrap().body.as_ref(), b"not json");
    assert!(
        resume(
            &host,
            &resources,
            &store,
            ReverseVideoKind::Native,
            exp,
            &mut p
        )
        .is_err()
    );
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}

#[test]
fn missing_sample_defaults_and_changed_saved_expiry_are_not_guessed() {
    let host = host(vec![native("a", "queued")]);
    let resources = Resources::default();
    let store = store();
    let exp = expiry();
    let mut source = input(1, 1);
    source.parameters.as_mut().unwrap().sample_count = None;
    assert!(
        create(
            &host,
            &resources,
            &store,
            source,
            ReverseVideoKind::Native,
            exp,
            &mut ReverseVideoProgress::default()
        )
        .is_err()
    );
    assert_eq!(*store.attempts.lock().unwrap(), 0);
    let mut p = ReverseVideoProgress::default();
    create(
        &host,
        &resources,
        &store,
        input(1, 1),
        ReverseVideoKind::Native,
        exp,
        &mut p,
    )
    .unwrap();
    assert!(
        resume(
            &host,
            &resources,
            &store,
            ReverseVideoKind::Native,
            exp + Duration::from_secs(1),
            &mut p
        )
        .is_err()
    );
    assert!(
        query(
            &host,
            &resources,
            &store,
            ReverseVideoKind::Native,
            0,
            "a",
            exp + Duration::from_secs(1),
            &mut p
        )
        .is_err()
    );
    assert_eq!(host.sent.lock().unwrap().len(), 1);
}
#[test]
fn nontrivial_native_id_is_preserved_and_encoded_as_one_path_segment() {
    let host = host(vec![native("id/a:b%z", "completed")]);
    let resources = Resources::default();
    let store = store();
    let mut p = ReverseVideoProgress::default();
    create(
        &host,
        &resources,
        &store,
        input(1, 1),
        ReverseVideoKind::Native,
        expiry(),
        &mut p,
    )
    .unwrap();
    assert_eq!(
        resources.reads.lock().unwrap()[0].1,
        ResourceReference::Url("https://upstream.test/v1/videos/id%2Fa%3Ab%25z/content".into())
    );
    let video::ReverseVideoResult::Native(v) = p.state.as_ref().unwrap().children[0]
        .result
        .as_ref()
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(v.id, "id/a:b%z");
}
#[test]
fn raw_http_rejection_retains_prior_children_and_prevents_repeating_failed_post() {
    let host = host(vec![native("a", "queued")]);
    host.responses.lock().unwrap().push(WireResponse {
        status: StatusCode::TOO_MANY_REQUESTS,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from_static(b"rate limited")),
    });
    let resources = Resources::default();
    let store = store();
    let exp = expiry();
    let mut p = ReverseVideoProgress::default();
    let result = create(
        &host,
        &resources,
        &store,
        input(1, 2),
        ReverseVideoKind::Native,
        exp,
        &mut p,
    )
    .unwrap();
    let JsonInvocation::Rejected(r) = result else {
        panic!()
    };
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    let HttpBody::Bytes(bytes) = r.body else {
        panic!()
    };
    assert_eq!(bytes.as_ref(), b"rate limited");
    assert!(p.state.as_ref().unwrap().children[0].result.is_some());
    assert!(
        resume(
            &host,
            &resources,
            &store,
            ReverseVideoKind::Native,
            exp,
            &mut p
        )
        .is_err()
    );
    assert_eq!(host.sent.lock().unwrap().len(), 2);
}
#[test]
fn publication_cas_failure_reuses_identical_id_and_does_not_recreate() {
    let host = host(vec![native("a", "completed")]);
    let resources = Resources::default();
    let mut store = store();
    store.fail_after = Some(5);
    let exp = expiry();
    let mut p = ReverseVideoProgress::default();
    assert!(
        create(
            &host,
            &resources,
            &store,
            input(1, 1),
            ReverseVideoKind::Native,
            exp,
            &mut p
        )
        .is_err()
    );
    let id = p.publication_ids[0].clone();
    assert!(p.state.as_ref().unwrap().children[0].published.is_some());
    resume(
        &host,
        &resources,
        &store,
        ReverseVideoKind::Native,
        exp,
        &mut p,
    )
    .unwrap();
    assert_eq!(host.sent.lock().unwrap().len(), 1);
    assert_eq!(resources.published.lock().unwrap()[1].1, id);
}
#[test]
fn resource_bound_counts_inputs_across_all_fanout_children() {
    let host = host(vec![]);
    let resources = Resources::default();
    let store = store();
    let mut source = input(2, 2);
    for instance in &mut source.instances {
        let image = g::VideoImage::builder()
            .bytes_base64_encoded(PNG)
            .mime_type("image/png")
            .build();
        instance.image = Some(image.clone());
        instance.last_frame = Some(image.clone());
        instance.reference_images = Some(vec![
            g::VideoReferenceImage::builder()
                .image(image)
                .reference_type(g::VideoReferenceType::Asset)
                .build(),
        ]);
    }
    assert!(
        create(
            &host,
            &resources,
            &store,
            source,
            ReverseVideoKind::OpenRouter,
            expiry(),
            &mut ReverseVideoProgress::default()
        )
        .is_err()
    );
    assert_eq!(*store.attempts.lock().unwrap(), 0);
    assert!(resources.published.lock().unwrap().is_empty());
}

#[test]
fn shared_fanout_fixtures_preserve_every_requested_instance_and_sample() {
    let cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/video_fanout_parity.json")).unwrap();
    for case in cases {
        let count = case["v4_children"].as_u64().map(|v| v as usize);
        let host = host(
            (0..count.unwrap_or(0))
                .map(|i| native(&format!("video_{i}"), "queued"))
                .collect(),
        );
        let resources = Resources::default();
        let store = store();
        let mut p = ReverseVideoProgress::default();
        let result = create(
            &host,
            &resources,
            &store,
            serde_json::from_value(case["input"].clone()).unwrap(),
            ReverseVideoKind::Native,
            expiry(),
            &mut p,
        );
        assert_eq!(result.is_ok(), count.is_some(), "{}", case["name"]);
        let record = match result {
            Ok(_) => {
                let sent = host.sent.lock().unwrap();
                assert_eq!(sent.len(), count.unwrap());
                let requests: Vec<Value> = sent
                    .iter()
                    .map(|r| {
                        let HttpBody::Bytes(b) = &r.body else {
                            panic!()
                        };
                        serde_json::from_slice(b).unwrap()
                    })
                    .collect();
                json!({"name":case["name"],"accepted":true,"requests":requests})
            }
            Err(error) => {
                assert!(host.sent.lock().unwrap().is_empty());
                json!({"name":case["name"],"accepted":false,"error":error.to_string()})
            }
        };
        println!("FANOUT_PARITY {record}");
    }
}

#[test]
fn terminal_optional_metadata_refresh_keeps_published_content_and_diagnostics() {
    let mut refreshed = native("a", "completed");
    refreshed["prompt"] = json!("prompt-0");
    let host = host(vec![native("a", "completed"), refreshed]);
    let resources = Resources::default();
    let store = store();
    let exp = expiry();
    let mut p = ReverseVideoProgress::default();
    create(
        &host,
        &resources,
        &store,
        input(1, 1),
        ReverseVideoKind::Native,
        exp,
        &mut p,
    )
    .unwrap();
    let result = query(
        &host,
        &resources,
        &store,
        ReverseVideoKind::Native,
        0,
        "a",
        exp,
        &mut p,
    )
    .unwrap();
    let JsonInvocation::Success(response) = result else {
        panic!()
    };
    assert_eq!(response.body.value.done, Some(true));
    assert!(
        response
            .body
            .report
            .diagnostics
            .iter()
            .any(|d| d.field == "prompt")
    );
    assert_eq!(resources.reads.lock().unwrap().len(), 1);
    assert_eq!(resources.published.lock().unwrap().len(), 1);
}
