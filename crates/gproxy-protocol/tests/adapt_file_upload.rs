use bytes::Bytes;
use futures_util::StreamExt;
use gproxy_protocol::{
    HttpBody, WireRequest, WireResponse,
    adapt::{JsonInvocation, files},
    capability::{CapabilityError, CapabilityFuture, CapabilityLimits, Upstream},
    codec::CodecLimits,
    connection::{HeaderMap, Method, MultipartPart, StatusCode},
    openai::files::FileObject,
};
use std::{
    collections::VecDeque,
    future::Future,
    sync::{Arc, Mutex},
    task::{Context, Poll},
};

fn ready<F: Future>(future: F) -> F::Output {
    let mut future = Box::pin(future);
    let mut cx = Context::from_waker(std::task::Waker::noop());
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
fn limits() -> CodecLimits {
    CodecLimits {
        max_buffer_bytes: 4096,
        max_value_bytes: 4096,
        max_body_bytes: 4096,
        max_line_bytes: 4096,
        max_part_bytes: 4096,
        max_parts: 8,
    }
}

#[derive(Clone)]
struct Fake {
    responses: Arc<Mutex<VecDeque<WireResponse<HttpBody>>>>,
    seen: Arc<Mutex<Vec<(String, HeaderMap, Bytes)>>>,
}
impl Upstream for Fake {
    type Target = ();
    fn send<'a>(
        &'a self,
        _: &'a (),
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        let seen = self.seen.clone();
        let response = self.responses.lock().unwrap().pop_front().unwrap();
        Box::pin(async move {
            let mut body = request.body;
            let mut bytes = Bytes::new();
            if let HttpBody::Bytes(value) = body {
                bytes = value;
            } else if let HttpBody::Stream(ref mut stream) = body {
                while let Some(chunk) = stream.next().await {
                    bytes = Bytes::from([bytes.as_ref(), chunk.unwrap().as_ref()].concat());
                }
            }
            seen.lock().unwrap().push((
                match request.query {
                    Some(query) => format!("{}?{query}", request.path),
                    None => request.path,
                },
                request.headers,
                bytes,
            ));
            Ok(response)
        })
    }
    fn connect<'a>(
        &'a self,
        _: &'a (),
        _: WireRequest<()>,
    ) -> CapabilityFuture<
        'a,
        Result<gproxy_protocol::capability::UpstreamConnection, CapabilityError>,
    > {
        Box::pin(async {
            Err(CapabilityError::new(
                gproxy_protocol::capability::CapabilityErrorKind::Unsupported,
                gproxy_protocol::capability::CapabilityErrorStage::Start,
                "unused",
            ))
        })
    }
    fn limits(&self) -> CapabilityLimits {
        CapabilityLimits {
            operation_total: std::time::Duration::from_secs(5),
            stream_idle: std::time::Duration::from_secs(1),
            read_bytes: 4096,
            write_bytes: 4096,
            ws_frame_bytes: 1024,
        }
    }
}

fn json_ok() -> WireResponse<HttpBody> {
    WireResponse {
        status: StatusCode::OK,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from_static(b"{}")),
    }
}

#[test]
fn multipart_upload_stream_contains_mime_filename_and_bytes() {
    let fake = Fake {
        responses: Arc::new(Mutex::new(VecDeque::from([json_ok()]))),
        seen: Arc::new(Mutex::new(Vec::new())),
    };
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-disposition",
        "form-data; name=\"file\"; filename=\"a.txt\""
            .parse()
            .unwrap(),
    );
    headers.insert("content-type", "text/plain".parse().unwrap());
    let result = ready(files::upload_multipart_json::<_, serde_json::Value>(
        &fake,
        &(),
        template("/v1/files"),
        "B".into(),
        vec![MultipartPart {
            headers,
            body: HttpBody::Bytes(Bytes::from_static(b"abc")),
        }],
        limits(),
    ))
    .unwrap();
    assert!(matches!(result, JsonInvocation::Success(_)));
    let (path, headers, body) = &fake.seen.lock().unwrap()[0];
    assert_eq!(path, "/v1/files");
    assert!(
        headers
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("boundary=B")
    );
    assert!(body.windows(3).any(|window| window == b"abc"));
    assert!(body.windows(5).any(|window| window == b"a.txt"));
}

#[test]
fn multipart_upload_limit_and_response_decode_failure_are_reported_after_send() {
    let fake = Fake {
        responses: Arc::new(Mutex::new(VecDeque::from([WireResponse {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            body: HttpBody::Bytes(Bytes::from_static(b"not-json")),
        }]))),
        seen: Arc::new(Mutex::new(Vec::new())),
    };
    let error = ready(files::upload_multipart_json::<_, FileObject>(
        &fake,
        &(),
        template("/v1/files"),
        "B".into(),
        vec![],
        limits(),
    ))
    .unwrap_err();
    assert_eq!(error.attempted_calls, 1);
    assert_eq!(fake.seen.lock().unwrap().len(), 1);
}

#[test]
fn gemini_start_finalize_origin_and_no_retry_are_enforced() {
    let start = WireResponse {
        status: StatusCode::OK,
        headers: HeaderMap::from_iter([(
            "X-Goog-Upload-URL".parse().unwrap(),
            "https://files.example/upload/session?upload_id=a%2Fb"
                .parse()
                .unwrap(),
        )]),
        body: HttpBody::Bytes(Bytes::new()),
    };
    let done = WireResponse {
        status: StatusCode::OK,
        headers: HeaderMap::from_iter([(
            "X-Goog-Upload-Status".parse().unwrap(),
            "final".parse().unwrap(),
        )]),
        body: HttpBody::Bytes(Bytes::from_static(
            b"{\"file\":{\"name\":\"files/new\"},\"foreign\":1}",
        )),
    };
    let fake = Fake {
        responses: Arc::new(Mutex::new(VecDeque::from([start, done]))),
        seen: Arc::new(Mutex::new(Vec::new())),
    };
    let metadata = gproxy_protocol::gemini::files::UploadFileMetadata::builder().build();
    let mut session = ready(files::gemini_start_resumable(
        &fake,
        &(),
        template("/upload/v1beta/files"),
        metadata,
        3,
        "text/plain",
        "https://files.example",
        limits(),
    ))
    .unwrap();
    assert!(matches!(
        ready(files::gemini_upload_chunk(
            &fake,
            &(),
            &mut session,
            HeaderMap::new(),
            "https://files.example",
            Bytes::from_static(b"abc"),
            true,
            limits()
        ))
        .unwrap(),
        files::GeminiUploadProgress::Finalized(_)
    ));
    let seen = fake.seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].1["x-goog-upload-header-content-length"], "3");
    assert_eq!(seen[0].1["x-goog-upload-header-content-type"], "text/plain");
    assert_eq!(seen[1].0, "/upload/session?upload_id=a%2Fb");
    assert!(session.is_closed());
    assert_eq!(session.offset(), 3);
    assert_eq!(seen[1].1.get("X-Goog-Upload-Offset").unwrap(), "0");
    assert_eq!(
        seen[1].1.get("X-Goog-Upload-Command").unwrap(),
        "upload, finalize"
    );
}

#[test]
fn gemini_cross_origin_session_fails_without_upload_call() {
    let start = WireResponse {
        status: StatusCode::OK,
        headers: HeaderMap::from_iter([(
            "x-goog-upload-url".parse().unwrap(),
            "https://other.example/upload".parse().unwrap(),
        )]),
        body: HttpBody::Bytes(Bytes::new()),
    };
    let fake = Fake {
        responses: Arc::new(Mutex::new(VecDeque::from([start]))),
        seen: Arc::new(Mutex::new(Vec::new())),
    };
    let metadata = gproxy_protocol::gemini::files::UploadFileMetadata::builder().build();
    assert!(
        ready(files::gemini_start_resumable(
            &fake,
            &(),
            template("/upload/v1beta/files"),
            metadata,
            1,
            "text/plain",
            "https://files.example",
            limits()
        ))
        .is_err()
    );
    assert_eq!(fake.seen.lock().unwrap().len(), 1);
}

fn template(path: &str) -> WireRequest<()> {
    let mut headers = HeaderMap::new();
    headers.insert("x-target-header", "preserved".parse().unwrap());
    WireRequest {
        method: Method::POST,
        path: path.into(),
        query: None,
        headers,
        body: (),
    }
}

#[test]
fn finalize_size_and_origin_are_checked_before_send_and_uncertain_session_cannot_retry() {
    let start = WireResponse {
        status: StatusCode::OK,
        headers: HeaderMap::from_iter([(
            "x-goog-upload-url".parse().unwrap(),
            "https://files.example/upload/session?token=x"
                .parse()
                .unwrap(),
        )]),
        body: HttpBody::Bytes(Bytes::new()),
    };
    let failed = WireResponse {
        status: StatusCode::SERVICE_UNAVAILABLE,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from_static(b"upstream error")),
    };
    let fake = Fake {
        responses: Arc::new(Mutex::new(VecDeque::from([start, failed]))),
        seen: Arc::new(Mutex::new(Vec::new())),
    };
    let mut session = ready(files::gemini_start_resumable(
        &fake,
        &(),
        template("/upload/v1beta/files"),
        gproxy_protocol::gemini::files::UploadFileMetadata::builder().build(),
        3,
        "text/plain",
        "https://files.example",
        limits(),
    ))
    .unwrap();
    assert!(
        ready(files::gemini_upload_chunk(
            &fake,
            &(),
            &mut session,
            HeaderMap::new(),
            "https://files.example",
            Bytes::from_static(b"a"),
            true,
            limits()
        ))
        .is_err()
    );
    assert!(!session.is_closed());
    assert_eq!(fake.seen.lock().unwrap().len(), 1);
    assert!(
        ready(files::gemini_upload_chunk(
            &fake,
            &(),
            &mut session,
            HeaderMap::new(),
            "https://files.example.evil",
            Bytes::from_static(b"abc"),
            true,
            limits()
        ))
        .is_err()
    );
    assert_eq!(fake.seen.lock().unwrap().len(), 1);
    let error = ready(files::gemini_upload_chunk(
        &fake,
        &(),
        &mut session,
        HeaderMap::new(),
        "https://files.example",
        Bytes::from_static(b"abc"),
        true,
        limits(),
    ))
    .unwrap_err();
    assert_eq!(error.attempted_calls, 1);
    assert!(matches!(error.failure, files::UploadFailure::Rejected(_)));
    assert!(session.is_closed());
    assert!(
        ready(files::gemini_upload_chunk(
            &fake,
            &(),
            &mut session,
            HeaderMap::new(),
            "https://files.example",
            Bytes::from_static(b"abc"),
            true,
            limits()
        ))
        .is_err()
    );
    assert_eq!(fake.seen.lock().unwrap().len(), 2);
}

struct SourceAccess;
impl gproxy_protocol::capability::ResourceAccess for SourceAccess {
    type Scope = ();
    type PublishedHandle = ();
    fn resolve<'a>(
        &'a self,
        _: &'a (),
        _: &'a gproxy_protocol::capability::ResourceReference,
    ) -> CapabilityFuture<'a, Result<gproxy_protocol::capability::ResourceMetadata, CapabilityError>>
    {
        Box::pin(async { Ok(resource_metadata()) })
    }
    fn read<'a>(
        &'a self,
        _: &'a (),
        _: &'a gproxy_protocol::capability::ResourceReference,
    ) -> CapabilityFuture<'a, Result<gproxy_protocol::capability::ResourceRead, CapabilityError>>
    {
        Box::pin(async {
            Ok(gproxy_protocol::capability::ResourceRead {
                metadata: resource_metadata(),
                body: HttpBody::Bytes(Bytes::from_static(b"actual-source-bytes")),
            })
        })
    }
    fn publish<'a>(
        &'a self,
        _: &'a (),
        _: &'a str,
        _: gproxy_protocol::capability::PublicationKind,
        _: gproxy_protocol::capability::ResourceMetadata,
        _: HttpBody,
    ) -> CapabilityFuture<
        'a,
        Result<gproxy_protocol::capability::PublishedResource<()>, CapabilityError>,
    > {
        panic!("copy must execute Files upload, not publish")
    }
    fn publication_status<'a>(
        &'a self,
        _: &'a (),
        _: &'a str,
    ) -> CapabilityFuture<
        'a,
        Result<gproxy_protocol::capability::PublicationStatus<()>, CapabilityError>,
    > {
        panic!()
    }
    fn release<'a>(
        &'a self,
        _: &'a (),
        _: &'a (),
    ) -> CapabilityFuture<'a, Result<(), CapabilityError>> {
        panic!()
    }
    fn limits(&self) -> CapabilityLimits {
        CapabilityLimits {
            operation_total: std::time::Duration::from_secs(5),
            stream_idle: std::time::Duration::from_secs(1),
            read_bytes: 4096,
            write_bytes: 4096,
            ws_frame_bytes: 1024,
        }
    }
}
fn resource_metadata() -> gproxy_protocol::capability::ResourceMetadata {
    gproxy_protocol::capability::ResourceMetadata {
        mime: Some("text/plain".into()),
        length: Some(19),
        filename: Some("copied.txt".into()),
        expires_at: None,
    }
}
#[test]
fn copy_downloads_source_and_executes_real_multipart_upload() {
    let fake=Fake{responses:Arc::new(Mutex::new(VecDeque::from([WireResponse{status:StatusCode::OK,headers:HeaderMap::new(),body:HttpBody::Bytes(Bytes::from_static(b"{\"id\":\"new-id\",\"bytes\":19,\"created_at\":1,\"filename\":\"copied.txt\",\"object\":\"file\",\"purpose\":\"user_data\",\"status\":\"processed\",\"foreign\":1}"))}]))),seen:Arc::new(Mutex::new(Vec::new()))};
    let source = gproxy_protocol::capability::ResourceReference::Id("source".into());
    let result = ready(files::copy_to_multipart::<_, _, FileObject>(
        &SourceAccess,
        &(),
        &source,
        &fake,
        &(),
        template("/v1/files"),
        "BOUND".into(),
        Vec::new(),
        None,
        limits(),
    ))
    .unwrap();
    let JsonInvocation::Success(response) = result else {
        panic!()
    };
    assert_eq!(response.body.id, "new-id");
    assert!(response.body.rest.is_empty());
    let seen = fake.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert!(seen[0].2.windows(19).any(|v| v == b"actual-source-bytes"));
    assert_eq!(seen[0].1["x-target-header"], "preserved");
}

#[test]
fn cancellation_marks_resumable_outcome_uncertain_before_await() {
    struct Pending;
    impl Upstream for Pending {
        type Target = ();
        fn send<'a>(
            &'a self,
            _: &'a (),
            _: WireRequest<HttpBody>,
        ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
            Box::pin(std::future::pending())
        }
        fn connect<'a>(
            &'a self,
            _: &'a (),
            _: WireRequest<()>,
        ) -> CapabilityFuture<
            'a,
            Result<gproxy_protocol::capability::UpstreamConnection, CapabilityError>,
        > {
            panic!()
        }
        fn limits(&self) -> CapabilityLimits {
            gproxy_protocol::capability::ResourceAccess::limits(&SourceAccess)
        }
    }
    let start = WireResponse {
        status: StatusCode::OK,
        headers: HeaderMap::from_iter([(
            "x-goog-upload-url".parse().unwrap(),
            "https://files.example/upload/s".parse().unwrap(),
        )]),
        body: HttpBody::Bytes(Bytes::new()),
    };
    let fake = Fake {
        responses: Arc::new(Mutex::new(VecDeque::from([start]))),
        seen: Arc::new(Mutex::new(Vec::new())),
    };
    let mut session = ready(files::gemini_start_resumable(
        &fake,
        &(),
        template("/upload/v1beta/files"),
        gproxy_protocol::gemini::files::UploadFileMetadata::builder().build(),
        3,
        "text/plain",
        "https://files.example",
        limits(),
    ))
    .unwrap();
    let mut pending = Box::pin(files::gemini_upload_chunk(
        &Pending,
        &(),
        &mut session,
        HeaderMap::new(),
        "https://files.example",
        Bytes::from_static(b"abc"),
        true,
        limits(),
    ));
    let mut context = Context::from_waker(std::task::Waker::noop());
    assert!(pending.as_mut().poll(&mut context).is_pending());
    drop(pending);
    assert!(session.is_closed());
    assert_eq!(session.offset(), 0);
}
