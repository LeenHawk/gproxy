use gproxy_protocol::{claude::files as c, gemini::files as g, openai::files as o};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
fn round<T: Serialize + DeserializeOwned>(v: Value) -> T {
    let p: T = serde_json::from_value(v.clone()).unwrap();
    assert_eq!(serde_json::to_value(&p).unwrap(), v);
    p
}
fn openai() -> Value {
    json!({"id":"f","bytes":12,"created_at":123,"filename":"x.txt","object":"file","purpose":"assistants","status":"processed","expires_at":456,"status_details":null})
}
fn claude() -> Value {
    json!({"id":"f","created_at":"2026-09-13T00:00:00Z","filename":"x.txt","mime_type":"text/plain","size_bytes":12,"type":"file","downloadable":false,"scope":{"id":"s","type":"session"}})
}
fn gemini() -> Value {
    json!({"name":"files/f","displayName":"x","mimeType":"video/mp4","sizeBytes":"9223372036854775807","createTime":"2026-09-13T00:00:00Z","updateTime":"2026-09-13T00:00:00Z","expirationTime":"2026-09-14T00:00:00Z","sha256Hash":"YWJj","uri":"https://files/f","downloadUri":"https://files/f:download","state":"FAILED","source":"GENERATED","videoMetadata":{"videoDuration":"3.500s"},"error":{"code":3,"message":"invalid","details":[{"@type":"type.googleapis.com/google.rpc.DebugInfo","stackEntries":["frame"]}]}})
}
#[test]
fn all_three_resources_have_typed_known_fields_and_exact_envelopes() {
    let p = round::<o::FileObject>(openai());
    assert!(p.rest.is_empty());
    assert_eq!(p.status, o::FileStatus::Processed);
    let p = round::<o::ListFilesResponseBody>(
        json!({"object":"list","data":[openai()],"has_more":false,"first_id":"f","last_id":"f"}),
    );
    assert!(p.rest.is_empty());
    assert!(p.data[0].rest.is_empty());
    let p = round::<o::DeleteFileResponseBody>(json!({"id":"f","object":"file","deleted":true}));
    assert!(p.rest.is_empty());
    let p = round::<c::FileMetadata>(claude());
    assert!(p.rest.is_empty());
    assert!(p.scope.unwrap().rest.is_empty());
    let p = round::<c::ListFilesResponseBody>(
        json!({"data":[claude()],"first_id":"f","last_id":"f","has_more":false}),
    );
    assert!(p.rest.is_empty());
    let p = round::<c::DeleteFileResponseBody>(json!({"id":"f","type":"file_deleted"}));
    assert!(p.rest.is_empty());
    let p = round::<g::File>(gemini());
    assert!(p.rest.is_empty());
    assert!(p.video_metadata.unwrap().unwrap().rest.is_empty());
    let status = p.error.unwrap().unwrap();
    assert!(status.rest.is_empty());
    assert_eq!(
        status.details.unwrap().unwrap()[0].rest["stackEntries"],
        json!(["frame"])
    );
    let p = round::<g::ListFilesResponseBody>(json!({"files":[gemini()],"nextPageToken":"next"}));
    assert!(p.rest.is_empty());
    let p = round::<g::UploadFileResponseBody>(json!({"file":gemini()}));
    assert!(p.rest.is_empty());
    let p = round::<g::DeleteFileResponseBody>(json!({}));
    assert!(p.rest.is_empty());
}
#[test]
fn required_optional_and_nullable_contracts_do_not_drop_values() {
    for field in [
        "id",
        "bytes",
        "created_at",
        "filename",
        "object",
        "purpose",
        "status",
    ] {
        let mut v = openai();
        v.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<o::FileObject>(v).is_err(),
            "OpenAI missing {field}"
        );
    }
    for field in ["expires_at", "status_details"] {
        for null in [false, true] {
            let mut v = openai();
            if null {
                v[field] = Value::Null;
            } else {
                v.as_object_mut().unwrap().remove(field);
            }
            round::<o::FileObject>(v);
        }
    }
    for field in ["first_id", "last_id", "has_more"] {
        let mut v = json!({"object":"list","data":[],"has_more":false,"first_id":"","last_id":""});
        v.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<o::ListFilesResponseBody>(v).is_err());
    }
    for field in [
        "id",
        "created_at",
        "filename",
        "mime_type",
        "size_bytes",
        "type",
    ] {
        let mut v = claude();
        v.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<c::FileMetadata>(v).is_err());
    }
    for field in ["downloadable", "scope"] {
        let mut v = claude();
        v.as_object_mut().unwrap().remove(field);
        round::<c::FileMetadata>(v.clone());
        v[field] = Value::Null;
        assert!(serde_json::from_value::<c::FileMetadata>(v).is_err());
    }
    round::<c::ListFilesResponseBody>(json!({"data":[]}));
    round::<c::DeleteFileResponseBody>(json!({"id":"f"}));
    assert!(
        serde_json::from_value::<c::DeleteFileResponseBody>(json!({"id":"f","type":null})).is_err()
    );
    let base = gemini();
    for field in base.as_object().unwrap().keys() {
        let mut v = base.clone();
        v.as_object_mut().unwrap().remove(field);
        round::<g::File>(v.clone());
        v[field] = Value::Null;
        round::<g::File>(v);
    }
    round::<g::File>(json!({}));
    round::<g::Status>(json!({}));
    round::<g::ListFilesResponseBody>(json!({}));
}
fn closed<T: DeserializeOwned + Serialize>(values: &[&str]) {
    for value in values {
        round::<T>(json!(value));
    }
    assert!(serde_json::from_value::<T>(json!("unknown-value")).is_err());
}
#[test]
fn purpose_request_output_and_source_enums_are_distinct() {
    closed::<o::UploadFilePurpose>(&[
        "assistants",
        "batch",
        "fine-tune",
        "vision",
        "user_data",
        "evals",
    ]);
    closed::<o::FilePurpose>(&[
        "assistants",
        "assistants_output",
        "batch",
        "batch_output",
        "fine-tune",
        "fine-tune-results",
        "vision",
        "user_data",
    ]);
    assert!(serde_json::from_value::<o::UploadFilePurpose>(json!("batch_output")).is_err());
    assert!(serde_json::from_value::<o::FilePurpose>(json!("evals")).is_err());
    closed::<o::FileStatus>(&["uploaded", "processed", "error"]);
    closed::<o::FileExpiryAnchor>(&["created_at"]);
    closed::<o::FileOrder>(&["asc", "desc"]);
    closed::<c::FileScopeType>(&["session"]);
    closed::<c::DeletedFileType>(&["file_deleted"]);
    closed::<c::FileType>(&["file"]);
    closed::<g::FileState>(&["STATE_UNSPECIFIED", "PROCESSING", "ACTIVE", "FAILED"]);
    closed::<g::FileSource>(&["SOURCE_UNSPECIFIED", "UPLOADED", "GENERATED", "REGISTERED"]);
}
#[test]
fn expiry_query_and_gemini_metadata_shapes_follow_native_names() {
    let e = round::<o::FileExpiry>(json!({"anchor":"created_at","seconds":3600}));
    assert!(e.rest.is_empty());
    assert!(serde_json::from_value::<o::FileExpiry>(json!({"anchor":"created_at"})).is_err());
    let q = round::<o::ListFilesQuery>(
        json!({"after":"f","limit":100,"order":"desc","purpose":"future-purpose"}),
    );
    assert!(q.rest.is_empty());
    let q = round::<c::ListFilesQuery>(
        json!({"after_id":"f","before_id":"g","limit":10,"scope_id":"s"}),
    );
    assert!(q.rest.is_empty());
    let q: g::ListFilesQuery =
        serde_json::from_value(json!({"page_size":10,"page_token":"next","filter":"extension"}))
            .unwrap();
    assert_eq!(q.page_size, Some(Some(10)));
    assert_eq!(
        serde_json::to_value(q).unwrap(),
        json!({"pageSize":10,"pageToken":"next","filter":"extension"})
    );
    let f: g::File = serde_json::from_value(
        json!({"display_name":"x","size_bytes":"42","video_metadata":{"video_duration":"2s"}}),
    )
    .unwrap();
    assert!(f.rest.is_empty());
    assert_eq!(
        serde_json::to_value(f).unwrap(),
        json!({"displayName":"x","sizeBytes":"42","videoMetadata":{"videoDuration":"2s"}})
    );
    round::<g::FileSize>(json!(42));
    round::<g::FileSize>(json!("42"));
    assert!(serde_json::from_value::<g::FileSize>(json!("not-a-size")).is_err());
    assert!(serde_json::from_value::<g::FileSize>(json!("9223372036854775808")).is_err());
    let m = round::<g::UploadFileMetadata>(
        json!({"file":{"name":"files/f","displayName":"x","mimeType":"text/plain"}}),
    );
    assert!(m.rest.is_empty());
}
#[test]
fn unknown_fields_survive_without_becoming_known_aliases() {
    let mut v = openai();
    v["future"] = json!([true]);
    let p = round::<o::FileObject>(v);
    assert_eq!(p.rest.len(), 1);
    let mut v = claude();
    v["future"] = json!([true]);
    let p = round::<c::FileMetadata>(v);
    assert_eq!(p.rest.len(), 1);
    let mut v = gemini();
    v["future"] = json!([true]);
    let p = round::<g::File>(v);
    assert_eq!(p.rest.len(), 1);
    let q = round::<o::ListFilesQuery>(json!({"afterId":"x"}));
    assert!(q.after.is_none());
    assert!(q.rest.contains_key("afterId"));
    let p = round::<g::DeleteFileResponseBody>(json!({"future":true}));
    assert!(p.rest.contains_key("future"));
}
#[test]
fn multipart_stream_fields_and_http_five_three_parts_are_preserved() {
    use gproxy_protocol::connection::{HttpBody, MultipartPart};
    struct Bytes;
    impl futures_core::Stream for Bytes {
        type Item = Result<bytes::Bytes, gproxy_protocol::connection::TransportError>;
        fn poll_next(
            self: std::pin::Pin<&mut Self>,
            _: &mut std::task::Context<'_>,
        ) -> std::task::Poll<Option<Self::Item>> {
            std::task::Poll::Ready(None)
        }
    }
    fn part() -> MultipartPart {
        let mut headers = http::HeaderMap::new();
        headers.insert(
            "content-disposition",
            http::HeaderValue::from_static("form-data; name=\"file\"; filename=\"x.bin\""),
        );
        MultipartPart {
            headers,
            body: HttpBody::Stream(Box::pin(Bytes)),
        }
    }
    let form = o::UploadFileForm::builder(part(), o::UploadFilePurpose::Evals)
        .expires_after(o::FileExpiry::builder(o::FileExpiryAnchor::CreatedAt, 3600).build())
        .build();
    let request: o::UploadFileRequest = gproxy_protocol::WireRequest {
        method: http::Method::POST,
        path: "/v1/files".into(),
        query: Some("x=1".into()),
        headers: http::HeaderMap::new(),
        body: form,
    };
    assert_eq!(request.method, http::Method::POST);
    assert_eq!(request.path, "/v1/files");
    assert_eq!(request.query.as_deref(), Some("x=1"));
    assert!(request.headers.is_empty());
    assert_eq!(request.body.expires_after.unwrap().seconds, 3600);
    assert!(matches!(request.body.file.body, HttpBody::Stream(_)));
    let _ = c::UploadFileForm::builder(part()).build();
    let metadata = g::UploadFileMetadata::builder()
        .file(Some(
            g::File::builder().display_name(Some("x".into())).build(),
        ))
        .build();
    let _ = g::UploadFileForm::builder(part(), metadata).build();
    let get: o::ListFilesRequest = gproxy_protocol::WireRequest {
        method: http::Method::GET,
        path: "/v1/files".into(),
        query: None,
        headers: http::HeaderMap::new(),
        body: (),
    };
    assert_eq!(get.body, ());
    let response: g::DeleteFileResponse = gproxy_protocol::WireResponse {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: g::DeleteFileResponseBody::builder().build(),
    };
    assert_eq!(response.status, http::StatusCode::OK);
    assert!(response.headers.is_empty());
    assert_eq!(serde_json::to_value(response.body).unwrap(), json!({}));
    let download: c::RetrieveFileContentResponse = gproxy_protocol::WireResponse {
        status: http::StatusCode::OK,
        headers: http::HeaderMap::new(),
        body: HttpBody::Stream(Box::pin(Bytes)),
    };
    assert!(matches!(download.body, HttpBody::Stream(_)));
}
