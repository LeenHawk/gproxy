use bytes::Bytes;
use gproxy_protocol::{
    WireRequest, WireResponse,
    adapt::files,
    capability::{CapabilityError, CapabilityFuture, CapabilityLimits, Upstream},
    connection::{HeaderMap, HttpBody, StatusCode},
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

type Requests = Vec<(String, Option<String>)>;
#[derive(Clone)]
struct Fake {
    responses: Arc<Mutex<VecDeque<WireResponse<HttpBody>>>>,
    requests: Arc<Mutex<Requests>>,
}
impl Upstream for Fake {
    type Target = ();
    fn send<'a>(
        &'a self,
        _: &'a (),
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'a, Result<WireResponse<HttpBody>, CapabilityError>> {
        self.requests
            .lock()
            .unwrap()
            .push((request.path, request.query));
        let response = self.responses.lock().unwrap().pop_front().unwrap();
        Box::pin(async move { Ok(response) })
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

fn json_response<T: serde::Serialize>(value: &T) -> WireResponse<HttpBody> {
    WireResponse {
        status: StatusCode::OK,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(value).unwrap())),
    }
}

#[test]
fn openai_list_paginates_and_delete_accepts_empty_204() {
    let pages = VecDeque::from([
        json_response(
            &serde_json::json!({"object":"list","data":[{"id":"f1","bytes":1,"created_at":1,"filename":"a","object":"file","purpose":"user_data","status":"processed"}],"has_more":true,"first_id":"f1","last_id":"f1"}),
        ),
        json_response(
            &serde_json::json!({"object":"list","data":[],"has_more":false,"first_id":"f1","last_id":"f1"}),
        ),
        WireResponse {
            status: StatusCode::NO_CONTENT,
            headers: HeaderMap::new(),
            body: HttpBody::Bytes(Bytes::new()),
        },
    ]);
    let fake = Fake {
        responses: Arc::new(Mutex::new(pages)),
        requests: Arc::new(Mutex::new(Vec::new())),
    };
    let files = ready(files::openai_list(
        &fake,
        &(),
        template("/v1/files", http::Method::GET),
        gproxy_protocol::openai::files::ListFilesQuery::builder().build(),
        files::FileCrudLimits {
            codec: fake_limits(),
            max_pages: 3,
            max_files: 100,
            max_declared_bytes: 16384,
        },
    ))
    .unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(
        ready(files::delete_empty(
            &fake,
            &(),
            template("/v1/files/f1", http::Method::DELETE)
        ))
        .unwrap()
        .status,
        StatusCode::NO_CONTENT
    );
    assert_eq!(fake.requests.lock().unwrap().len(), 3);
}

fn fake_limits() -> gproxy_protocol::codec::CodecLimits {
    gproxy_protocol::codec::CodecLimits {
        max_buffer_bytes: 4096,
        max_value_bytes: 4096,
        max_body_bytes: 4096,
        max_line_bytes: 4096,
        max_part_bytes: 4096,
        max_parts: 8,
    }
}

#[test]
fn cursor_and_file_ids_are_percent_encoded() {
    let page = json_response(
        &serde_json::json!({"data":[],"has_more":false,"first_id":"x","last_id":"x"}),
    );
    let fake = Fake {
        responses: Arc::new(Mutex::new(VecDeque::from([page]))),
        requests: Arc::new(Mutex::new(Vec::new())),
    };
    let query = gproxy_protocol::claude::files::ListFilesQuery::builder()
        .after_id("a&b/#?")
        .build();
    ready(files::claude_list(
        &fake,
        &(),
        template("/v1/files", http::Method::GET),
        query,
        files::FileCrudLimits {
            codec: fake_limits(),
            max_pages: 1,
            max_files: 100,
            max_declared_bytes: 16384,
        },
    ))
    .unwrap();
    assert_eq!(
        fake.requests.lock().unwrap()[0].1.as_deref(),
        Some("after_id=a%26b%2F%23%3F")
    );
}

fn template(path: &str, method: http::Method) -> WireRequest<()> {
    let mut headers = HeaderMap::new();
    headers.insert("anthropic-beta", "files-api-2025-04-14".parse().unwrap());
    WireRequest {
        method,
        path: path.into(),
        query: None,
        headers,
        body: (),
    }
}
fn crud_limits() -> files::FileCrudLimits {
    files::FileCrudLimits {
        codec: fake_limits(),
        max_pages: 5,
        max_files: 10,
        max_declared_bytes: 16384,
    }
}

#[test]
fn claude_and_gemini_really_collect_all_pages_and_preserve_query_filters() {
    let fake = Fake {
        responses: Arc::new(Mutex::new(VecDeque::from([
            json_response(
                &serde_json::json!({"data":[{"id":"a","created_at":"2024-01-01T00:00:00Z","filename":"a","mime_type":"text/plain","size_bytes":1,"type":"file"}],"has_more":true,"first_id":"a","last_id":"a"}),
            ),
            json_response(&serde_json::json!({"data":[],"has_more":false})),
        ]))),
        requests: Arc::new(Mutex::new(Vec::new())),
    };
    let query = gproxy_protocol::claude::files::ListFilesQuery::builder()
        .limit(2)
        .scope_id("scope/one")
        .build();
    let result = ready(files::claude_list(
        &fake,
        &(),
        template("/selected/files", http::Method::GET),
        query,
        crud_limits(),
    ))
    .unwrap();
    assert_eq!(result.len(), 1);
    let seen = fake.requests.lock().unwrap();
    assert_eq!(seen.len(), 2);
    assert!(seen[0].1.as_ref().unwrap().contains("scope_id=scope%2Fone"));
    assert!(seen[1].1.as_ref().unwrap().contains("after_id=a"));
    drop(seen);
    let fake = Fake {
        responses: Arc::new(Mutex::new(VecDeque::from([
            json_response(&serde_json::json!({"files":[{"name":"files/a"}],"nextPageToken":"a&b"})),
            json_response(&serde_json::json!({"files":[{"name":"files/b"}],"nextPageToken":""})),
        ]))),
        requests: Arc::new(Mutex::new(Vec::new())),
    };
    let query = gproxy_protocol::gemini::files::ListFilesQuery::builder()
        .page_size(Some(2))
        .build();
    assert_eq!(
        ready(files::gemini_list(
            &fake,
            &(),
            template("/v1beta/files", http::Method::GET),
            query,
            crud_limits()
        ))
        .unwrap()
        .len(),
        2
    );
    assert!(
        fake.requests.lock().unwrap()[1]
            .1
            .as_ref()
            .unwrap()
            .contains("pageToken=a%26b")
    );
}

#[test]
fn gemini_resource_path_has_one_real_separator_and_list_errors_keep_raw_body() {
    let fake = Fake {
        responses: Arc::new(Mutex::new(VecDeque::from([json_response(
            &serde_json::json!({"name":"files/a/b"}),
        )]))),
        requests: Arc::new(Mutex::new(Vec::new())),
    };
    ready(files::gemini_get(
        &fake,
        &(),
        template("/v1beta/files", http::Method::GET),
        "files/a/b",
        fake_limits(),
    ))
    .unwrap();
    assert_eq!(fake.requests.lock().unwrap()[0].0, "/v1beta/files/a%2Fb");
    let body = HttpBody::Stream(Box::pin(futures_util::stream::poll_fn(
        |_| -> Poll<Option<Result<Bytes, gproxy_protocol::connection::TransportError>>> {
            panic!("error body polled")
        },
    )));
    let fake = Fake {
        responses: Arc::new(Mutex::new(VecDeque::from([WireResponse {
            status: StatusCode::TOO_MANY_REQUESTS,
            headers: HeaderMap::new(),
            body,
        }]))),
        requests: Arc::new(Mutex::new(Vec::new())),
    };
    let error = ready(files::gemini_list(
        &fake,
        &(),
        template("/v1beta/files", http::Method::GET),
        gproxy_protocol::gemini::files::ListFilesQuery::builder().build(),
        crud_limits(),
    ))
    .unwrap_err();
    assert_eq!(error.completed_calls, 0);
    let files::FileFailure::Rejected(response) = error.failure else {
        panic!()
    };
    assert_eq!(response.status, StatusCode::TOO_MANY_REQUESTS);
}

#[test]
fn pagination_cycles_and_total_retention_are_bounded() {
    let page = serde_json::json!({"files":[],"nextPageToken":"repeat"});
    let fake = Fake {
        responses: Arc::new(Mutex::new(VecDeque::from([
            json_response(&page),
            json_response(&page),
        ]))),
        requests: Arc::new(Mutex::new(Vec::new())),
    };
    let error = ready(files::gemini_list(
        &fake,
        &(),
        template("/v1beta/files", http::Method::GET),
        gproxy_protocol::gemini::files::ListFilesQuery::builder().build(),
        crud_limits(),
    ))
    .unwrap_err();
    assert_eq!(error.completed_calls, 2);
    let fake = Fake {
        responses: Arc::new(Mutex::new(VecDeque::from([json_response(
            &serde_json::json!({"files":[{"name":"files/a"}]}),
        )]))),
        requests: Arc::new(Mutex::new(Vec::new())),
    };
    let mut limits = crud_limits();
    limits.max_files = 0;
    assert!(
        ready(files::gemini_list(
            &fake,
            &(),
            template("/v1beta/files", http::Method::GET),
            gproxy_protocol::gemini::files::ListFilesQuery::builder().build(),
            limits
        ))
        .is_err()
    );
}

#[test]
fn all_six_metadata_mappings_use_actual_fields_and_never_relabel_processing_as_processed() {
    use gproxy_protocol::{
        transform::files as mapping,
        wire::{claude::files as c, gemini::files as g, openai::files as o},
    };
    let source:o::FileObject=serde_json::from_value(serde_json::json!({"id":"source","bytes":3,"created_at":1704067200,"filename":"a.txt","object":"file","purpose":"user_data","status":"processed"})).unwrap();
    let target = mapping::FileFacts {
        id: Some("target".into()),
        mime: Some("text/plain".into()),
        purpose: Some(mapping::FilePurpose::UserData),
        status: Some(mapping::FileStatusFacts::Active),
        ..Default::default()
    };
    let claude = mapping::openai_to_claude(&source, &target).unwrap().value;
    assert_eq!(claude.created_at, "2024-01-01T00:00:00Z");
    assert_eq!(
        mapping::claude_to_openai(&claude, &target)
            .unwrap()
            .value
            .created_at,
        1704067200
    );
    let mut gemini_target = target.clone();
    gemini_target.id = Some("files/target".into());
    let gemini = mapping::openai_to_gemini(&source, &gemini_target)
        .unwrap()
        .value;
    assert_eq!(
        gemini.size_bytes.as_ref().unwrap().as_ref().unwrap(),
        &g::FileSize::String("3".into())
    );
    assert!(mapping::claude_to_gemini(&claude, &gemini_target).is_ok());
    assert!(mapping::gemini_to_claude(&gemini, &target).is_ok());
    assert!(mapping::gemini_to_openai(&gemini, &target).is_ok());
    let mut bad = target;
    bad.status = Some(mapping::FileStatusFacts::Processing);
    assert!(mapping::claude_to_openai(&claude, &bad).is_err());
    bad.status = Some(mapping::FileStatusFacts::Active);
    bad.expires_at = Some("invalid-time".into());
    assert!(mapping::claude_to_openai(&claude, &bad).is_err());
    let empty = mapping::FileFacts::default();
    assert!(mapping::gemini_to_claude(&gemini, &empty).is_err());
    let _: c::FileMetadata = claude;
}
