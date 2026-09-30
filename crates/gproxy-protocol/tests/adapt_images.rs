use base64::Engine;
use gproxy_protocol::{
    HttpBody, WireRequest, WireResponse,
    adapt::images::{self, ImageError, ImageLimits, ImageProgress},
    capability::{
        CapabilityError, CapabilityErrorKind, CapabilityErrorStage, CapabilityFuture,
        CapabilityLimits, PublicationKind, PublicationStatus, PublishedResource, ResourceAccess,
        ResourceMetadata, ResourceRead, ResourceReference, Upstream, UpstreamConnection,
    },
    codec::CodecLimits,
    connection::{Bytes, HeaderMap, StatusCode},
    transform::{
        TransformErrorKind,
        images::{
            self as mapping, ImageDialect, ImageInput, ImageResponseFacts, ImageTargetModels,
        },
    },
};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    future::Future,
    sync::Mutex,
    task::{Context, Poll},
    time::{Duration, SystemTime},
};
const PNG: &str =
    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVR4nGP4z8AAAAMBAQDJ/pLvAAAAAElFTkSuQmCC";
const MASK: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgYGBgAAAABQABpfZFQAAAAABJRU5ErkJggg==";
const WEBP: &str =
    "UklGRjwAAABXRUJQVlA4IDAAAADQAQCdASoBAAEAAUAmJaACdLoB+AADsAD+8ut//NgVzXPv9//S4P0uD9Lg/9KQAAA=";
fn bytes(s: &str) -> Bytes {
    base64::engine::general_purpose::STANDARD
        .decode(s)
        .unwrap()
        .into()
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
struct Host {
    sent: Mutex<Vec<WireRequest<HttpBody>>>,
    responses: Mutex<VecDeque<WireResponse<HttpBody>>>,
    pending_at: Option<usize>,
}
impl Host {
    fn new(responses: Vec<WireResponse<HttpBody>>) -> Self {
        Self {
            sent: Mutex::default(),
            responses: Mutex::new(responses.into()),
            pending_at: None,
        }
    }
    fn sent_json(&self) -> Vec<Value> {
        self.sent
            .lock()
            .unwrap()
            .iter()
            .map(|r| {
                let HttpBody::Bytes(b) = &r.body else {
                    panic!("expected JSON bytes")
                };
                serde_json::from_slice(b).unwrap()
            })
            .collect()
    }
}
impl Upstream for Host {
    type Target = ();
    fn send<'a>(
        &'a self,
        _: &'a (),
        r: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        Box::pin(async move {
            let n = {
                let mut sent = self.sent.lock().unwrap();
                sent.push(r);
                sent.len()
            };
            if self.pending_at == Some(n) {
                std::future::pending().await
            } else {
                Ok(self
                    .responses
                    .lock()
                    .unwrap()
                    .pop_front()
                    .expect("unexpected extra upstream call"))
            }
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
        caps()
    }
}
#[derive(Default)]
struct Resources {
    reads: Mutex<Vec<ResourceReference>>,
    published: Mutex<Vec<(String, ResourceMetadata, Bytes)>>,
    metadata_length: Option<u64>,
    mime: Option<String>,
    pending_publish: bool,
    fail_publish_at: Option<usize>,
    bad_publication: bool,
}
impl ResourceAccess for Resources {
    type Scope = ();
    type PublishedHandle = String;
    fn resolve<'a>(
        &'a self,
        _: &'a (),
        _: &'a ResourceReference,
    ) -> CapabilityFuture<'a, Result<ResourceMetadata, CapabilityError>> {
        panic!("read returns metadata")
    }
    fn read<'a>(
        &'a self,
        _: &'a (),
        r: &'a ResourceReference,
    ) -> CapabilityFuture<'a, Result<ResourceRead, CapabilityError>> {
        Box::pin(async move {
            self.reads.lock().unwrap().push(r.clone());
            let b = bytes(PNG);
            Ok(ResourceRead {
                metadata: ResourceMetadata {
                    mime: Some(self.mime.clone().unwrap_or("image/png".into())),
                    length: Some(self.metadata_length.unwrap_or(b.len() as u64)),
                    filename: None,
                    expires_at: None,
                },
                body: HttpBody::Bytes(b),
            })
        })
    }
    fn publish<'a>(
        &'a self,
        _: &'a (),
        id: &'a str,
        kind: PublicationKind,
        metadata: ResourceMetadata,
        body: HttpBody,
    ) -> CapabilityFuture<'a, Result<PublishedResource<String>, CapabilityError>> {
        Box::pin(async move {
            assert_eq!(kind, PublicationKind::Url);
            let HttpBody::Bytes(b) = body else {
                panic!("expected bytes")
            };
            let n = {
                let mut p = self.published.lock().unwrap();
                p.push((id.into(), metadata.clone(), b));
                p.len()
            };
            if self.pending_publish {
                return std::future::pending().await;
            }
            if self.fail_publish_at == Some(n) {
                return Err(CapabilityError::new(
                    CapabilityErrorKind::Storage,
                    CapabilityErrorStage::Start,
                    "publication failed",
                ));
            }
            Ok(PublishedResource {
                handle: id.into(),
                reference: if self.bad_publication {
                    ResourceReference::Id("wrong".into())
                } else {
                    ResourceReference::Url(format!("https://cdn.test/{n}.png"))
                },
                metadata,
            })
        })
    }
    fn publication_status<'a>(
        &'a self,
        _: &'a (),
        _: &'a str,
    ) -> CapabilityFuture<'a, Result<PublicationStatus<String>, CapabilityError>> {
        Box::pin(async { Ok(PublicationStatus::Missing) })
    }
    fn release<'a>(
        &'a self,
        _: &'a (),
        _: &'a String,
    ) -> CapabilityFuture<'a, Result<(), CapabilityError>> {
        panic!("adapter must retain handles for explicit compensation")
    }
    fn limits(&self) -> CapabilityLimits {
        caps()
    }
}
fn limits() -> ImageLimits {
    ImageLimits {
        codec: CodecLimits {
            max_buffer_bytes: 65536,
            max_value_bytes: 65536,
            max_body_bytes: 65536,
            max_line_bytes: 65536,
            max_part_bytes: 65536,
            max_parts: 16,
        },

        max_input_bytes: 65536,
        max_total_input_bytes: 65536,
        max_output_bytes: 65536,
        max_total_output_bytes: 65536,
    }
}
fn models() -> ImageTargetModels {
    ImageTargetModels {
        generation_model: "selected-generation-model".into(),
        image_tool_model: None,
    }
}
fn facts() -> ImageResponseFacts {
    ImageResponseFacts {
        created: 42,
        observed_at: SystemTime::now(),
        default_response_format: gproxy_protocol::openai::images::ImageResponseFormat::B64Json,
        publish_expires_at: Some(SystemTime::now() + Duration::from_secs(3600)),
        operation_id: "operation:with:separator".into(),
    }
}
fn create(value: Value) -> ImageInput {
    ImageInput::Create(serde_json::from_value(value).unwrap())
}
fn edit(value: Value) -> ImageInput {
    ImageInput::Edit(serde_json::from_value(value).unwrap())
}
fn basic() -> ImageInput {
    create(json!({"prompt":"draw a red square","model":"source-image-model"}))
}
fn value(d: ImageDialect) -> Value {
    match d {
        ImageDialect::Gemini => {
            json!({"candidates":[{"index":0,"finishReason":"STOP","content":{"role":"model","parts":[{"inlineData":{"mimeType":"image/png","data":PNG}}]}}],"responseId":"gemini-r","modelVersion":"gemini-actual","usageMetadata":{"promptTokenCount":7,"candidatesTokenCount":11,"totalTokenCount":18}})
        }
        ImageDialect::Responses => {
            json!({"id":"r","created_at":1,"error":null,"incomplete_details":null,"instructions":null,"metadata":{},"model":"generation-actual","object":"response","status":"completed","output":[{"type":"image_generation_call","id":"img","result":PNG,"status":"completed"}],"parallel_tool_calls":false,"temperature":null,"tool_choice":"auto","tools":[],"top_p":null,"usage":{"input_tokens":7,"output_tokens":11,"total_tokens":18,"input_tokens_details":{"cached_tokens":0,"cache_write_tokens":0},"output_tokens_details":{"reasoning_tokens":0}}})
        }
    }
}
fn response(v: Value) -> WireResponse<HttpBody> {
    WireResponse {
        status: StatusCode::OK,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(serde_json::to_vec(&v).unwrap().into()),
    }
}
fn ready<F: Future>(f: F) -> F::Output {
    let mut f = Box::pin(f);
    match f
        .as_mut()
        .poll(&mut Context::from_waker(std::task::Waker::noop()))
    {
        Poll::Ready(v) => v,
        Poll::Pending => panic!("unexpected pending"),
    }
}
fn run(
    resources: &Resources,
    host: &Host,
    input: ImageInput,
    d: ImageDialect,
    limits: ImageLimits,
    progress: &mut ImageProgress<String>,
) -> Result<images::ImageOutcome, ImageError> {
    ready(images::generate(
        resources,
        &(),
        host,
        &(),
        input,
        d,
        &models(),
        &facts(),
        limits,
        progress,
    ))
}
fn kind(e: ImageError) -> TransformErrorKind {
    let (ImageError::Transform(e)
    | ImageError::InvalidResponses { error: e, .. }
    | ImageError::InvalidGemini { error: e, .. }) = e
    else {
        panic!("expected transform error")
    };
    e.kind()
}
#[test]
fn all_four_directions_split_n_and_retain_real_usage() {
    for d in [ImageDialect::Responses, ImageDialect::Gemini] {
        for is_edit in [false, true] {
            let resources = Resources::default();
            let host = Host::new(vec![response(value(d)), response(value(d))]);
            let input = if is_edit {
                edit(
                    json!({"prompt":"edit it","images":[{"file_id":"opaque-no-file-prefix"}],"model":"source-image-model","n":2,"unknown_source_key":"must-not-leak"}),
                )
            } else {
                create(
                    json!({"prompt":"draw","model":"source-image-model","n":2,"unknown_source_key":"must-not-leak"}),
                )
            };
            let mut progress = ImageProgress::default();
            let result = run(&resources, &host, input, d, limits(), &mut progress).unwrap();
            assert_eq!(result.response.created, 42);
            assert_eq!(result.response.data.as_ref().unwrap().len(), 2);
            assert!(result.response.usage.is_none());
            assert!(result.response.background.is_none());
            assert!(result.response.quality.is_none());
            assert_eq!(
                serde_json::to_value(result.response.size.as_ref().unwrap()).unwrap(),
                json!("1x1")
            );
            assert_eq!(progress.attempted_calls(), 2);
            assert!(progress.calls().iter().all(|c| c.usage.is_some()));
            for v in host.sent_json() {
                assert!(v.get("unknown_source_key").is_none());
                match d {
                    ImageDialect::Responses => {
                        assert_eq!(v["model"], "selected-generation-model");
                        assert_eq!(v["tools"][0]["model"], "source-image-model");
                        assert_eq!(
                            v["tools"][0]["action"],
                            if is_edit { "edit" } else { "generate" }
                        );
                        assert!(v["tools"][0].get("n").is_none());
                        assert_eq!(v["tool_choice"]["type"], "image_generation");
                    }
                    ImageDialect::Gemini => {
                        assert!(v["generationConfig"].get("candidateCount").is_none());
                        assert!(v["generationConfig"].get("responseMimeType").is_none());
                        assert_eq!(
                            v["generationConfig"]["responseModalities"],
                            json!(["IMAGE"])
                        );
                    }
                }
            }
            assert_eq!(
                resources.reads.lock().unwrap().as_slice(),
                if is_edit {
                    vec![ResourceReference::Id("opaque-no-file-prefix".into())]
                } else {
                    vec![]
                }
            );
        }
    }
}
#[test]
fn responses_nondefault_fields_are_typed_and_auto_is_not_high() {
    let data = format!("data:image/png;base64,{MASK}");
    let source = edit(
        json!({"prompt":"edit","images":[{"image_url":data}],"mask":{"image_url":data},"model":"source-image","background":"transparent","quality":"auto","size":"auto","output_format":"webp","output_compression":37,"input_fidelity":"high","moderation":"low","user":"account"}),
    );
    let reference = ResourceReference::Url(data);
    let resolved = vec![mapping::ResolvedImageInput {
        reference,
        bytes_base64: MASK.into(),
        mime_type: "image/png".into(),
    }];
    let mut selected = models();
    selected.image_tool_model = Some("selected-image-tool".into());
    let prepared =
        mapping::build_request(source, ImageDialect::Responses, &selected, &resolved).unwrap();
    let mapping::ImageDialectBody::Responses(r) = prepared.value.body else {
        panic!()
    };
    let v = serde_json::to_value(r.body).unwrap();
    assert_eq!(v["model"], "selected-generation-model");
    assert_eq!(v["user"], "account");
    let tool = &v["tools"][0];
    for (k, val) in [
        ("model", json!("selected-image-tool")),
        ("background", json!("transparent")),
        ("quality", json!("auto")),
        ("size", json!("auto")),
        ("output_format", json!("webp")),
        ("output_compression", json!(37)),
        ("input_fidelity", json!("high")),
        ("moderation", json!("low")),
    ] {
        assert_eq!(tool[k], val, "{k}");
    }
    assert!(
        tool["input_image_mask"]["image_url"]
            .as_str()
            .unwrap()
            .starts_with("data:image/png;base64,")
    );
}
#[test]
fn unmapped_gemini_controls_do_not_block_resource_reads_or_generation() {
    for field in [
        json!({"mask":{"file_id":"mask"}}),
        json!({"quality":"high"}),
        json!({"input_fidelity":"high"}),
        json!({"background":"transparent"}),
        json!({"moderation":"low"}),
        json!({"output_format":"png"}),
        json!({"output_compression":30}),
        json!({"size":"1536x1024"}),
        json!({"stream":true}),
        json!({"partial_images":1}),
    ] {
        let mut v = json!({"prompt":"edit","images":[{"file_id":"input"}]});
        v.as_object_mut()
            .unwrap()
            .extend(field.as_object().unwrap().clone());
        let resources = Resources::default();
        let host = Host::new(vec![response(value(ImageDialect::Gemini))]);
        let result = run(
            &resources,
            &host,
            edit(v),
            ImageDialect::Gemini,
            limits(),
            &mut ImageProgress::default(),
        )
        .unwrap();
        assert_eq!(result.response.data.unwrap().len(), 1);
        assert_eq!(resources.reads.lock().unwrap().len(), 1);
        assert_eq!(host.sent.lock().unwrap().len(), 1);
    }
}

#[test]
fn reference_kind_is_native_and_metadata_is_verified() {
    for bad in [false, true] {
        let resources = Resources {
            metadata_length: bad.then_some(1),
            ..Default::default()
        };
        let host = Host::new(vec![response(value(ImageDialect::Responses))]);
        let input = edit(json!({"prompt":"edit","images":[{"image_url":"file_not_an_id"}]}));
        let result = run(
            &resources,
            &host,
            input,
            ImageDialect::Responses,
            limits(),
            &mut ImageProgress::default(),
        );
        assert_eq!(
            resources.reads.lock().unwrap()[0],
            ResourceReference::Url("file_not_an_id".into())
        );
        assert_eq!(result.is_err(), bad);
        assert_eq!(host.sent.lock().unwrap().len(), usize::from(!bad));
    }
}
#[test]
fn ambiguous_reference_and_aggregate_input_cap_prevent_send() {
    for v in [
        json!({"prompt":"edit","images":[{"file_id":"one","image_url":"two"}]}),
        json!({"prompt":"edit","images":[{"file_id":"one"},{"file_id":"two"}]}),
    ] {
        let resources = Resources::default();
        let host = Host::new(vec![]);
        let mut cap = limits();
        cap.max_total_input_bytes = bytes(PNG).len() as u64;
        assert!(
            run(
                &resources,
                &host,
                edit(v),
                ImageDialect::Responses,
                cap,
                &mut ImageProgress::default()
            )
            .is_err()
        );
        assert!(host.sent.lock().unwrap().is_empty());
    }
}
#[test]
fn actual_webp_format_is_reported_without_request_guess() {
    let d = ImageDialect::Responses;
    let mut v = value(d);
    v["output"][0]["result"] = json!(WEBP);
    let host = Host::new(vec![response(v)]);
    let result = run(
        &Resources::default(),
        &host,
        basic(),
        d,
        limits(),
        &mut ImageProgress::default(),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(result.response.output_format).unwrap(),
        json!("webp")
    );
}
#[test]
fn invalid_outputs_and_completion_are_rejected_before_publication() {
    for d in [ImageDialect::Responses, ImageDialect::Gemini] {
        for issue in 0..4 {
            let mut v = value(d);
            match (d, issue) {
                (ImageDialect::Responses, 0) => v["status"] = json!("incomplete"),
                (ImageDialect::Responses, 1) => v["output"][0]["result"] = json!("AQI="),
                (ImageDialect::Responses, 2) => v["output"][0]["status"] = json!("in_progress"),
                (ImageDialect::Responses, _) => {
                    let item = v["output"][0].clone();
                    v["output"].as_array_mut().unwrap().push(item)
                }
                (ImageDialect::Gemini, 0) => {
                    v["candidates"][0]["finishReason"] = json!("MAX_TOKENS")
                }
                (ImageDialect::Gemini, 1) => {
                    v["candidates"][0]["content"]["parts"][0]["inlineData"]["data"] = json!("AQI=")
                }
                (ImageDialect::Gemini, 2) => {
                    v["candidates"][0]["content"]["parts"][0]["inlineData"]["mimeType"] =
                        json!("image/jpeg")
                }
                (ImageDialect::Gemini, _) => {
                    let c = v["candidates"][0].clone();
                    v["candidates"].as_array_mut().unwrap().push(c)
                }
            }
            let resources = Resources::default();
            let host = Host::new(vec![response(v)]);
            assert!(
                run(
                    &resources,
                    &host,
                    create(json!({"prompt":"draw","response_format":"url"})),
                    d,
                    limits(),
                    &mut ImageProgress::default()
                )
                .is_err()
            );
            assert!(resources.published.lock().unwrap().is_empty());
        }
    }
}
#[test]
fn zero_images_is_refused_before_send() {
    for d in [ImageDialect::Responses, ImageDialect::Gemini] {
        let host = Host::new(Vec::new());
        let error = run(
            &Resources::default(),
            &host,
            create(json!({"prompt":"draw","n":0})),
            d,
            limits(),
            &mut ImageProgress::default(),
        )
        .unwrap_err();
        assert_eq!(kind(error), TransformErrorKind::InvalidInput);
        assert!(host.sent_json().is_empty());
    }
}
#[test]
fn publication_facts_preflight_before_send() {
    let host = Host::new(vec![]);
    let mut facts = facts();
    facts.publish_expires_at = None;
    let result = ready(images::generate(
        &Resources::default(),
        &(),
        &host,
        &(),
        create(json!({"prompt":"draw","n":2,"response_format":"url"})),
        ImageDialect::Responses,
        &models(),
        &facts,
        limits(),
        &mut ImageProgress::default(),
    ));
    assert!(result.is_err());
    assert!(host.sent.lock().unwrap().is_empty());
}
#[test]
fn all_outputs_validated_before_any_publication_and_aggregate_cap_retains_completed_call() {
    let resources = Resources::default();
    let host = Host::new(vec![
        response(value(ImageDialect::Responses)),
        response(value(ImageDialect::Responses)),
    ]);
    let mut cap = limits();
    cap.max_total_output_bytes = bytes(PNG).len() as u64 + 1;
    let mut progress = ImageProgress::default();
    assert_eq!(
        kind(
            run(
                &resources,
                &host,
                create(json!({"prompt":"draw","n":2,"response_format":"url"})),
                ImageDialect::Responses,
                cap,
                &mut progress
            )
            .unwrap_err()
        ),
        TransformErrorKind::Limit
    );
    assert_eq!(progress.calls().len(), 1);
    assert_eq!(progress.attempted_calls(), 2);
    assert!(resources.published.lock().unwrap().is_empty());
}
#[test]
fn raw_http_rejection_is_retained_with_prior_progress() {
    let mut rejection = WireResponse {
        status: StatusCode::TOO_MANY_REQUESTS,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from_static(b"opaque vendor error")),
    };
    rejection
        .headers
        .insert("retry-after", "12".parse().unwrap());
    let host = Host::new(vec![response(value(ImageDialect::Gemini)), rejection]);
    let mut progress = ImageProgress::default();
    let error = run(
        &Resources::default(),
        &host,
        create(json!({"prompt":"draw","n":3})),
        ImageDialect::Gemini,
        limits(),
        &mut progress,
    )
    .unwrap_err();
    let ImageError::Rejected(r) = error else {
        panic!()
    };
    assert_eq!(r.status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(r.headers["retry-after"], "12");
    let HttpBody::Bytes(b) = r.body else { panic!() };
    assert_eq!(&b[..], b"opaque vendor error");
    assert_eq!(progress.calls().len(), 1);
    assert_eq!(host.sent.lock().unwrap().len(), 2);
}
#[test]
fn cancellation_closes_generation_and_retains_uncertain_call() {
    let mut host = Host::new(vec![response(value(ImageDialect::Responses))]);
    host.pending_at = Some(2);
    let resources = Resources::default();
    let mut progress = ImageProgress::default();
    let input = create(json!({"prompt":"draw","n":3}));
    let model = models();
    let facts = facts();
    let mut f = Box::pin(images::generate(
        &resources,
        &(),
        &host,
        &(),
        input,
        ImageDialect::Responses,
        &model,
        &facts,
        limits(),
        &mut progress,
    ));
    assert!(
        f.as_mut()
            .poll(&mut Context::from_waker(std::task::Waker::noop()))
            .is_pending()
    );
    drop(f);
    assert_eq!(progress.attempted_calls(), 2);
    assert_eq!(progress.calls().len(), 1);
    assert_eq!(
        kind(
            run(
                &resources,
                &host,
                basic(),
                ImageDialect::Responses,
                limits(),
                &mut progress
            )
            .unwrap_err()
        ),
        TransformErrorKind::MissingState
    );
    assert_eq!(host.sent.lock().unwrap().len(), 2);
}
#[test]
fn publication_cancellation_exposes_operation_for_status_recovery() {
    let resources = Resources {
        pending_publish: true,
        ..Default::default()
    };
    let host = Host::new(vec![response(value(ImageDialect::Responses))]);
    let mut progress = ImageProgress::default();
    let model = models();
    let facts = facts();
    let mut f = Box::pin(images::generate(
        &resources,
        &(),
        &host,
        &(),
        create(json!({"prompt":"draw","response_format":"url"})),
        ImageDialect::Responses,
        &model,
        &facts,
        limits(),
        &mut progress,
    ));
    assert!(
        f.as_mut()
            .poll(&mut Context::from_waker(std::task::Waker::noop()))
            .is_pending()
    );
    drop(f);
    assert_eq!(progress.calls().len(), 1);
    assert_eq!(progress.publication_ids().len(), 1);
    assert!(progress.published().is_empty());
    let p = resources.published.lock().unwrap();
    assert_eq!(&p[0].0, &progress.publication_ids()[0]);
    assert_eq!(p[0].1.length, Some(bytes(PNG).len() as u64));
    assert_eq!(p[0].2, bytes(PNG));
}
#[test]
fn partial_publication_and_bad_host_handle_remain_recoverable() {
    for bad in [false, true] {
        let resources = Resources {
            fail_publish_at: (!bad).then_some(2),
            bad_publication: bad,
            ..Default::default()
        };
        let host = Host::new(vec![
            response(value(ImageDialect::Responses)),
            response(value(ImageDialect::Responses)),
        ]);
        let mut progress = ImageProgress::default();
        assert!(
            run(
                &resources,
                &host,
                create(json!({"prompt":"draw","n":2,"response_format":"url"})),
                ImageDialect::Responses,
                limits(),
                &mut progress
            )
            .is_err()
        );
        assert_eq!(progress.calls().len(), 2);
        assert_eq!(progress.published().len(), 1);
        assert_eq!(progress.publication_ids().len(), if bad { 1 } else { 2 });
    }
}
#[test]
fn png_integrity_and_declared_mime_are_checked() {
    let png = bytes(PNG);
    let info = mapping::inspect_image(&png).unwrap();
    assert_eq!((info.width, info.height), (1, 1));
    let mut bad = png.to_vec();
    bad[30] ^= 1;
    assert!(mapping::inspect_image(&bad).is_err());
    assert!(mapping::inspect_image(&png[..png.len() - 1]).is_err());
    assert!(mapping::decode_image(PNG, Some("image/jpeg"), 65536).is_err());
    assert!(mapping::decode_image(PNG, None, 1).is_err());
}

const JPEG: &str = "/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBQNDAsLDBkSEw8UHRofHh0aHBwgJC4nICIsIxwcKDcpLDAxNDQ0Hyc5PTgyPC4zNDL/2wBDAQkJCQwLDBgNDRgyIRwhMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjIyMjL/wAARCAABAAEDASIAAhEBAxEB/8QAHwAAAQUBAQEBAQEAAAAAAAAAAAECAwQFBgcICQoL/8QAtRAAAgEDAwIEAwUFBAQAAAF9AQIDAAQRBRIhMUEGE1FhByJxFDKBkaEII0KxwRVS0fAkM2JyggkKFhcYGRolJicoKSo0NTY3ODk6Q0RFRkdISUpTVFVWV1hZWmNkZWZnaGlqc3R1dnd4eXqDhIWGh4iJipKTlJWWl5iZmqKjpKWmp6ipqrKztLW2t7i5usLDxMXGx8jJytLT1NXW19jZ2uHi4+Tl5ufo6erx8vP09fb3+Pn6/8QAHwEAAwEBAQEBAQEBAQAAAAAAAAECAwQFBgcICQoL/8QAtREAAgECBAQDBAcFBAQAAQJ3AAECAxEEBSExBhJBUQdhcRMiMoEIFEKRobHBCSMzUvAVYnLRChYkNOEl8RcYGRomJygpKjU2Nzg5OkNERUZHSElKU1RVVldYWVpjZGVmZ2hpanN0dXZ3eHl6goOEhYaHiImKkpOUlZaXmJmaoqOkpaanqKmqsrO0tba3uLm6wsPExcbHyMnK0tPU1dbX2Nna4uPk5ebn6Onq8vP09fb3+Pn6/9oADAMBAAIRAxEAPwDi6KKK+ZP3E//Z";

#[test]
fn response_reports_actual_format_without_enforcing_requested_format() {
    let meta = mapping::inspect_image(&bytes(JPEG)).unwrap();
    assert_eq!(meta.mime(), "image/jpeg");
    assert_eq!((meta.width, meta.height), (1, 1));
    let host = Host::new(vec![response(value(ImageDialect::Responses))]);
    let output = run(
        &Resources::default(),
        &host,
        create(json!({"prompt":"draw","output_format":"jpeg"})),
        ImageDialect::Responses,
        limits(),
        &mut ImageProgress::default(),
    )
    .unwrap();
    assert_eq!(output.response.data.unwrap().len(), 1);
}

#[test]
fn explicit_and_host_default_delivery_are_respected() {
    for explicit in [false, true] {
        let host = Host::new(vec![response(value(ImageDialect::Responses))]);
        let resources = Resources::default();
        let mut progress = ImageProgress::default();
        let mut f = facts();
        f.default_response_format = gproxy_protocol::openai::images::ImageResponseFormat::Url;
        let input = if explicit {
            create(json!({"prompt":"draw","response_format":"b64_json"}))
        } else {
            basic()
        };
        let out = ready(images::generate(
            &resources,
            &(),
            &host,
            &(),
            input,
            ImageDialect::Responses,
            &models(),
            &f,
            limits(),
            &mut progress,
        ))
        .unwrap();
        let image = &out.response.data.unwrap()[0];
        assert_eq!(image.url.is_some(), !explicit);
        assert_eq!(image.b64_json.is_some(), explicit);
        assert_eq!(progress.published().len(), usize::from(!explicit));
    }
}
#[test]
fn mask_is_forwarded_and_ancillary_text_is_diagnosed() {
    let data = format!("data:image/png;base64,{PNG}");
    let input =
        edit(json!({"prompt":"edit","images":[{"image_url":data}],"mask":{"image_url":data}}));
    let host = Host::new(vec![response(value(ImageDialect::Responses))]);
    assert!(
        run(
            &Resources::default(),
            &host,
            input,
            ImageDialect::Responses,
            limits(),
            &mut ImageProgress::default()
        )
        .is_ok()
    );
    assert_eq!(host.sent.lock().unwrap().len(), 1);
    let mut v = value(ImageDialect::Gemini);
    v["candidates"][0]["content"]["parts"]
        .as_array_mut()
        .unwrap()
        .push(json!({"text":"Here is your picture."}));
    let host = Host::new(vec![response(v)]);
    let out = run(
        &Resources::default(),
        &host,
        basic(),
        ImageDialect::Gemini,
        limits(),
        &mut ImageProgress::default(),
    )
    .unwrap();
    assert!(out.response.data.unwrap()[0].revised_prompt.is_none());
    assert!(
        out.report
            .diagnostics
            .iter()
            .any(|d| d.field == "candidate.text/thought")
    );
}
