//! files family: OpenAI/Claude/Gemini file CRUD converted to a provider that
//! speaks another dialect, driven through `Core::send`.

mod support;
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest, connection::Bytes};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use serde_json::{Value, json};
use support::*;

fn key(operation: Operation, dialect: Dialect) -> OperationKey {
    OperationKey { operation, dialect }
}

fn wire(method: Method, path: &str, query: Option<&str>) -> WireRequest<HttpBody> {
    WireRequest {
        method,
        path: path.into(),
        query: query.map(str::to_owned),
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::new()),
    }
}

fn multipart(parts: &[(&str, Option<&str>, Option<&str>, &str)]) -> WireRequest<HttpBody> {
    let mut body = String::new();
    for (name, filename, mime, content) in parts {
        body.push_str("--XYZ\r\n");
        body.push_str(&format!("content-disposition: form-data; name=\"{name}\""));
        if let Some(filename) = filename {
            body.push_str(&format!("; filename=\"{filename}\""));
        }
        body.push_str("\r\n");
        if let Some(mime) = mime {
            body.push_str(&format!("content-type: {mime}\r\n"));
        }
        body.push_str("\r\n");
        body.push_str(content);
        body.push_str("\r\n");
    }
    body.push_str("--XYZ--\r\n");
    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        HeaderValue::from_static("multipart/form-data; boundary=XYZ"),
    );
    WireRequest {
        method: Method::POST,
        path: "/v1/files".into(),
        query: None,
        headers,
        body: HttpBody::Bytes(Bytes::from(body)),
    }
}

async fn run(
    h: &Harness,
    provider: &str,
    operation: Operation,
    dialect: Dialect,
    request: WireRequest<HttpBody>,
) -> (StatusCode, HeaderMap, String) {
    let ctx = h.context_for(provider, key(operation, dialect), "r", 1, None);
    let execution = send(h, ctx, request).await.unwrap();
    let (response, completion) = execution.into_parts();
    let body = read(response.body).await;
    let _ = completion.await;
    (response.status, response.headers, body)
}

/// Per-operation entry points: `Core::send` matches every HTTP operation and
/// its debug-build poll frame does not fit the test thread's stack.
async fn send(
    h: &Harness,
    ctx: std::sync::Arc<gproxy_core::RequestContext>,
    request: WireRequest<HttpBody>,
) -> gproxy_core::CoreResult<gproxy_core::HttpExecution> {
    match ctx.operation.operation {
        Operation::CreateFile => h.core.create_file(ctx, request).await,
        Operation::ListFiles => h.core.list_files(ctx, request).await,
        Operation::RetrieveFile => h.core.retrieve_file(ctx, request).await,
        Operation::RetrieveFileContent => h.core.retrieve_file_content(ctx, request).await,
        Operation::DeleteFile => h.core.delete_file(ctx, request).await,
        other => panic!("not a files operation: {other:?}"),
    }
}

fn claude_file(id: &str) -> Value {
    json!({
        "id": id, "type": "file", "created_at": "2025-01-01T00:00:00Z",
        "filename": "a.txt", "mime_type": "text/plain", "size_bytes": 3
    })
}

#[tokio::test]
async fn openai_client_retrieves_and_deletes_files_on_a_claude_upstream() {
    let h = harness(full(), "round_robin").await;
    h.script(vec![json_reply(StatusCode::OK, claude_file("file_abc"))]);
    let (status, headers, body) = run(
        &h,
        "claude",
        Operation::RetrieveFile,
        Dialect::OpenAi,
        wire(Method::GET, "/v1/files/file_abc", None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "application/json");
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        body,
        json!({
            "id": "file_abc", "bytes": 3, "created_at": 1735689600, "filename": "a.txt",
            "object": "file", "purpose": "user_data", "status": "processed"
        })
    );
    let seen = h.client.seen.lines();
    assert!(
        seen[0].starts_with("GET https://claude.example/v1/files/file_abc auth=Bearer k1"),
        "{}",
        seen[0]
    );

    h.script(vec![json_reply(
        StatusCode::OK,
        json!({"id": "file_abc", "type": "file_deleted"}),
    )]);
    let (status, _, body) = run(
        &h,
        "claude",
        Operation::DeleteFile,
        Dialect::OpenAi,
        wire(Method::DELETE, "/v1/files/file_abc", None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        json!({"id": "file_abc", "object": "file", "deleted": true})
    );
    let seen = h.client.seen.lines();
    assert!(
        seen[1].starts_with("DELETE https://claude.example/v1/files/file_abc "),
        "{}",
        seen[1]
    );
}

#[tokio::test]
async fn list_follows_pages_and_applies_the_client_limit() {
    let h = harness(full(), "round_robin").await;
    h.script(vec![
        json_reply(
            StatusCode::OK,
            json!({
                "data": [claude_file("f1")], "first_id": "f1", "last_id": "f1", "has_more": true
            }),
        ),
        json_reply(
            StatusCode::OK,
            json!({
                "data": [claude_file("f2")], "first_id": "f2", "last_id": "f2", "has_more": false
            }),
        ),
    ]);
    let (status, _, body) = run(
        &h,
        "claude",
        Operation::ListFiles,
        Dialect::OpenAi,
        wire(Method::GET, "/v1/files", Some("limit=1")),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["object"], "list");
    assert_eq!(body["data"].as_array().unwrap().len(), 1);
    assert_eq!(body["data"][0]["id"], "f1");
    assert_eq!(body["has_more"], true);
    assert_eq!(body["first_id"], "f1");
    assert_eq!(body["last_id"], "f1");
    assert_eq!(h.client.seen.lines().len(), 2);

    // A filter Claude cannot express is refused before any upstream call.
    let ctx = h.context_for(
        "claude",
        key(Operation::ListFiles, Dialect::OpenAi),
        "r2",
        1,
        None,
    );
    let Err(error) = send(
        &h,
        ctx,
        wire(Method::GET, "/v1/files", Some("purpose=batch")),
    )
    .await
    else {
        panic!("unsupported filter must be refused")
    };
    assert!(error.to_string().contains("purpose"), "{error}");
    assert_eq!(h.client.seen.lines().len(), 2);
}

#[tokio::test]
async fn gemini_client_sees_claude_files_under_the_files_prefix() {
    let h = harness(full(), "round_robin").await;
    h.script(vec![json_reply(StatusCode::OK, claude_file("file_abc"))]);
    let (status, _, body) = run(
        &h,
        "claude",
        Operation::RetrieveFile,
        Dialect::Gemini,
        wire(Method::GET, "/v1beta/files/file_abc", None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["name"], "files/file_abc");
    assert_eq!(body["displayName"], "a.txt");
    assert_eq!(body["mimeType"], "text/plain");
    assert_eq!(body["sizeBytes"], "3");
    assert_eq!(body["state"], "ACTIVE");
    assert!(h.client.seen.lines()[0].starts_with("GET https://claude.example/v1/files/file_abc "));

    h.script(vec![json_reply(
        StatusCode::OK,
        json!({"data": [claude_file("f1"), claude_file("f2")], "has_more": false}),
    )]);
    let (_, _, body) = run(
        &h,
        "claude",
        Operation::ListFiles,
        Dialect::Gemini,
        wire(Method::GET, "/v1beta/files", Some("pageSize=1")),
    )
    .await;
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["files"].as_array().unwrap().len(), 1);
    assert_eq!(body["files"][0]["name"], "files/f1");
    assert_eq!(body["nextPageToken"], "files/f1");
}

#[tokio::test]
async fn content_download_passes_the_upstream_body_through_and_rejections_keep_their_body() {
    let h = harness(full(), "round_robin").await;
    h.script(vec![(
        StatusCode::OK,
        vec![("content-type", "text/plain")],
        vec![Bytes::from_static(b"abc")],
    )]);
    let (status, headers, body) = run(
        &h,
        "claude",
        Operation::RetrieveFileContent,
        Dialect::OpenAi,
        wire(Method::GET, "/v1/files/file_abc/content", None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "text/plain");
    assert_eq!(body, "abc");
    assert!(
        h.client.seen.lines()[0]
            .starts_with("GET https://claude.example/v1/files/file_abc/content ")
    );

    h.script(vec![json_reply(
        StatusCode::NOT_FOUND,
        json!({"type": "error", "error": {"type": "not_found_error", "message": "gone"}}),
    )]);
    let (status, _, body) = run(
        &h,
        "claude",
        Operation::RetrieveFile,
        Dialect::OpenAi,
        wire(Method::GET, "/v1/files/file_missing", None),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(body.contains("not_found_error"), "{body}");
}

#[tokio::test]
async fn openai_upload_becomes_a_claude_multipart_and_maps_the_reply() {
    let h = harness(full(), "round_robin").await;
    h.script(vec![json_reply(StatusCode::OK, claude_file("file_new"))]);
    let request = multipart(&[
        ("purpose", None, None, "user_data"),
        ("file", Some("a.txt"), Some("text/plain"), "abc"),
    ]);
    let (status, _, body) = run(
        &h,
        "claude",
        Operation::CreateFile,
        Dialect::OpenAi,
        request,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["id"], "file_new");
    assert_eq!(body["purpose"], "user_data");
    assert_eq!(body["status"], "processed");
    assert_eq!(body["bytes"], 3);
    assert_eq!(body["filename"], "a.txt");
    let seen = h.client.seen.lines();
    assert!(
        seen[0].starts_with("POST https://claude.example/v1/files auth=Bearer k1"),
        "{}",
        seen[0]
    );
    assert!(
        seen[0].contains("name=\"file\"; filename=\"a.txt\"") && seen[0].contains("abc"),
        "{}",
        seen[0]
    );
    assert!(
        !seen[0].contains("name=\"purpose\""),
        "Claude has no purpose field: {}",
        seen[0]
    );
}

#[tokio::test]
async fn openai_upload_to_a_gemini_upstream_uses_the_resumable_protocol() {
    let h = harness(full(), "round_robin").await;
    seed_provider(&h, "gemini", "https://gemini.example", "gemini").await;
    h.script(vec![
        (
            StatusCode::OK,
            vec![(
                "x-goog-upload-url",
                "https://gemini.example/upload/v1beta/files?upload_id=u1",
            )],
            vec![],
        ),
        json_reply(
            StatusCode::OK,
            json!({"file": {
                "name": "files/g1", "displayName": "a.txt", "mimeType": "text/plain",
                "sizeBytes": "3", "createTime": "2025-01-01T00:00:00Z", "state": "ACTIVE"
            }}),
        ),
    ]);
    let request = multipart(&[
        ("purpose", None, None, "user_data"),
        ("file", Some("a.txt"), Some("text/plain"), "abc"),
    ]);
    let (status, _, body) = run(
        &h,
        "gemini",
        Operation::CreateFile,
        Dialect::OpenAi,
        request,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["id"], "g1");
    assert_eq!(body["bytes"], 3);
    assert_eq!(body["filename"], "a.txt");
    assert_eq!(body["status"], "processed");
    assert_eq!(body["created_at"], 1735689600);
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 2);
    assert!(
        seen[0].starts_with("POST https://gemini.example/upload/v1beta/files auth=Bearer k-gemini"),
        "{}",
        seen[0]
    );
    assert!(seen[0].contains("\"displayName\":\"a.txt\""), "{}", seen[0]);
    assert!(
        seen[1].starts_with("POST https://gemini.example/upload/v1beta/files")
            && seen[1].ends_with("body=abc"),
        "{}",
        seen[1]
    );

    // Retrieve maps the Gemini shape back, with the bare id on the OpenAI side.
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({
            "name": "files/g1", "displayName": "a.txt", "mimeType": "text/plain",
            "sizeBytes": "3", "createTime": "2025-01-01T00:00:00Z", "state": "ACTIVE"
        }),
    )]);
    let (status, _, body) = run(
        &h,
        "gemini",
        Operation::RetrieveFile,
        Dialect::OpenAi,
        wire(Method::GET, "/v1/files/g1", None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["id"], "g1");
    assert!(h.client.seen.lines()[2].starts_with("GET https://gemini.example/v1beta/files/g1 "));

    // Gemini files cannot be downloaded: refused before any call.
    let ctx = h.context_for(
        "gemini",
        key(Operation::RetrieveFileContent, Dialect::OpenAi),
        "r3",
        1,
        None,
    );
    let Err(error) = send(&h, ctx, wire(Method::GET, "/v1/files/g1/content", None)).await else {
        panic!("Gemini download must be refused")
    };
    assert!(error.to_string().contains("download"), "{error}");
    assert_eq!(h.client.seen.lines().len(), 3);
}
