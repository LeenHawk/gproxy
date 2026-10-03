#![cfg(not(target_arch = "wasm32"))]

//! Gateway ownership of upstream files and video jobs (`gproxy_core::owned`):
//! one upstream credential shared by several scopes must not let one scope
//! read, delete or list what another created. Driven through passthrough to
//! provider `p`, whose two credentials (`a`, `b`) stand for a shared pool.

mod support;
use gproxy_core::{CoreError, RequestContext, owned::FILE_KIND};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest, connection::Bytes};
use http::{HeaderMap, Method, StatusCode};
use serde_json::{Value, json};
use std::sync::Arc;
use support::*;

fn wire(method: Method, path: &str) -> WireRequest<HttpBody> {
    WireRequest {
        method,
        path: path.into(),
        query: None,
        headers: HeaderMap::new(),
        body: HttpBody::Bytes(Bytes::new()),
    }
}

/// A context for `p` under `scope`, with both credentials permitted.
fn context(h: &Harness, operation: Operation, scope: &str) -> Arc<RequestContext> {
    let base = h.context_for(
        "p",
        OperationKey {
            operation,
            dialect: Dialect::OpenAi,
        },
        "r",
        2,
        None,
    );
    let mut context = (*base).clone();
    context.scope = scope.into();
    Arc::new(context)
}

/// Per-operation entry points: `Core::send` matches every HTTP operation and
/// its debug-build poll frame does not fit the test thread's stack.
async fn send(
    h: &Harness,
    operation: Operation,
    scope: &str,
    request: WireRequest<HttpBody>,
) -> gproxy_core::CoreResult<(StatusCode, String)> {
    let ctx = context(h, operation, scope);
    let execution = match operation {
        Operation::CreateFile => h.core.create_file(ctx, request).await,
        Operation::ListFiles => h.core.list_files(ctx, request).await,
        Operation::RetrieveFile => h.core.retrieve_file(ctx, request).await,
        Operation::RetrieveFileContent => h.core.retrieve_file_content(ctx, request).await,
        Operation::DeleteFile => h.core.delete_file(ctx, request).await,
        Operation::CreateVideo => h.core.create_video(ctx, request).await,
        Operation::RetrieveVideo => h.core.retrieve_video(ctx, request).await,
        Operation::ListVideos => h.core.list_videos(ctx, request).await,
        Operation::DownloadVideoContent => h.core.download_video_content(ctx, request).await,
        other => panic!("not an owned-resource operation: {other:?}"),
    }?;
    let (response, completion) = execution.into_parts();
    let body = read(response.body).await;
    let _ = completion.await;
    Ok((response.status, body))
}

fn assert_not_found(result: gproxy_core::CoreResult<(StatusCode, String)>, id: &str) {
    match result {
        Err(CoreError::ResourceNotFound { id: seen, .. }) => assert_eq!(seen, id),
        Err(other) => panic!("expected not-found for {id}, got {other}"),
        Ok((status, body)) => panic!("expected not-found for {id}, got {status}: {body}"),
    }
}

#[tokio::test]
async fn a_file_uploaded_by_one_scope_is_not_found_for_another() {
    let h = harness(full(), "round_robin").await;
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({"id": "file-1", "object": "file"}),
    )]);
    let (status, _) = send(
        &h,
        Operation::CreateFile,
        "user:alice",
        wire(Method::POST, "/v1/files"),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(h.client.seen.lines().len(), 1);

    // Another caller on the same credential pool: refused before any upstream
    // call, for metadata, content and delete alike.
    for (operation, method, path) in [
        (Operation::RetrieveFile, Method::GET, "/v1/files/file-1"),
        (
            Operation::RetrieveFileContent,
            Method::GET,
            "/v1/files/file-1/content",
        ),
        (Operation::DeleteFile, Method::DELETE, "/v1/files/file-1"),
    ] {
        assert_not_found(
            send(&h, operation, "user:mallory", wire(method, path)).await,
            "file-1",
        );
    }
    assert_eq!(h.client.seen.lines().len(), 1);

    // The uploader still reaches it.
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({"id": "file-1", "object": "file"}),
    )]);
    let (status, _) = send(
        &h,
        Operation::RetrieveFile,
        "user:alice",
        wire(Method::GET, "/v1/files/file-1"),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_eq!(h.client.seen.lines().len(), 2);
}

#[tokio::test]
async fn an_id_never_created_through_the_gateway_is_not_found() {
    let h = harness(full(), "round_robin").await;
    assert_not_found(
        send(
            &h,
            Operation::RetrieveFile,
            "tenant",
            wire(Method::GET, "/v1/files/file-elsewhere"),
        )
        .await,
        "file-elsewhere",
    );
    assert!(h.client.seen.lines().is_empty());
}

#[tokio::test]
async fn a_read_is_pinned_to_the_credential_that_holds_the_file() {
    let h = harness(full(), "round_robin").await;
    h.own("p", "b", FILE_KIND, "file-b").await;
    for _ in 0..3 {
        h.script(vec![json_reply(StatusCode::OK, json!({"id": "file-b"}))]);
        let (status, _) = send(
            &h,
            Operation::RetrieveFile,
            "tenant",
            wire(Method::GET, "/v1/files/file-b"),
        )
        .await
        .unwrap();
        assert_eq!(status, StatusCode::OK);
    }
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 3);
    assert!(
        seen.iter().all(|line| line.contains("auth=Bearer kb")),
        "{seen:?}"
    );
}

#[tokio::test]
async fn a_successful_delete_forgets_the_owner() {
    let h = harness(full(), "round_robin").await;
    h.own("p", "a", FILE_KIND, "file-gone").await;

    // A rejected delete keeps the row: the file may still exist upstream.
    h.script(vec![json_reply(
        StatusCode::BAD_REQUEST,
        json!({"error": {"message": "busy"}}),
    )]);
    let (status, _) = send(
        &h,
        Operation::DeleteFile,
        "tenant",
        wire(Method::DELETE, "/v1/files/file-gone"),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::BAD_REQUEST);

    h.script(vec![json_reply(
        StatusCode::OK,
        json!({"id": "file-gone", "object": "file", "deleted": true}),
    )]);
    let (status, _) = send(
        &h,
        Operation::DeleteFile,
        "tenant",
        wire(Method::DELETE, "/v1/files/file-gone"),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);
    assert_not_found(
        send(
            &h,
            Operation::RetrieveFile,
            "tenant",
            wire(Method::GET, "/v1/files/file-gone"),
        )
        .await,
        "file-gone",
    );
    assert_eq!(h.client.seen.lines().len(), 2);
}

#[tokio::test]
async fn a_file_list_shows_only_the_callers_own_files() {
    let h = harness(full(), "round_robin").await;
    h.own("p", "a", FILE_KIND, "mine").await;
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({
            "object": "list",
            "data": [{"id": "mine"}, {"id": "theirs"}],
            "has_more": true,
            "first_id": "mine",
            "last_id": "theirs"
        }),
    )]);
    let (status, body) = send(
        &h,
        Operation::ListFiles,
        "tenant",
        wire(Method::GET, "/v1/files"),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["data"], json!([{"id": "mine"}]));
    // Cursors are the upstream's so paging still advances.
    assert_eq!(body["has_more"], true);
    assert_eq!(body["last_id"], "theirs");

    // A list that cannot be read is refused, never passed through unfiltered.
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({"unexpected": true}),
    )]);
    assert!(
        send(
            &h,
            Operation::ListFiles,
            "tenant",
            wire(Method::GET, "/v1/files")
        )
        .await
        .is_err()
    );
}

#[tokio::test]
async fn a_video_job_is_reachable_only_by_the_scope_that_created_it() {
    let h = harness(full(), "round_robin").await;
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({"id": "video_1", "object": "video", "status": "queued"}),
    )]);
    let (status, _) = send(
        &h,
        Operation::CreateVideo,
        "user:alice",
        WireRequest {
            body: HttpBody::Bytes(Bytes::from_static(br#"{"prompt":"a cat"}"#)),
            ..wire(Method::POST, "/v1/videos")
        },
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);

    for (operation, path) in [
        (Operation::RetrieveVideo, "/v1/videos/video_1"),
        (
            Operation::DownloadVideoContent,
            "/v1/videos/video_1/content",
        ),
    ] {
        assert_not_found(
            send(&h, operation, "user:mallory", wire(Method::GET, path)).await,
            "video_1",
        );
    }

    h.script(vec![json_reply(
        StatusCode::OK,
        json!({"object": "list", "data": [{"id": "video_1"}, {"id": "video_other"}]}),
    )]);
    let (_, body) = send(
        &h,
        Operation::ListVideos,
        "user:mallory",
        wire(Method::GET, "/v1/videos"),
    )
    .await
    .unwrap();
    let body: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(body["data"], json!([]));

    h.script(vec![json_reply(
        StatusCode::OK,
        json!({"id": "video_1", "object": "video", "status": "completed"}),
    )]);
    let (status, _) = send(
        &h,
        Operation::RetrieveVideo,
        "user:alice",
        wire(Method::GET, "/v1/videos/video_1"),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);
    // Only the create, the list and alice's own poll went upstream.
    assert_eq!(h.client.seen.lines().len(), 3);
}

/// A passthrough generation on `p` in `dialect` under scope `tenant`.
async fn generate(
    h: &Harness,
    dialect: Dialect,
    path: &str,
    body: Value,
) -> gproxy_core::CoreResult<StatusCode> {
    let ctx = h.context_for(
        "p",
        OperationKey {
            operation: Operation::GenerateContent,
            dialect,
        },
        "g",
        2,
        None,
    );
    let request = WireRequest {
        body: HttpBody::Bytes(Bytes::from(serde_json::to_vec(&body).unwrap())),
        ..wire(Method::POST, path)
    };
    let execution = h.core.generate_content(ctx, request).await?;
    let (response, completion) = execution.into_parts();
    let _ = read(response.body).await;
    let _ = completion.await;
    Ok(response.status)
}

fn responses_with_file(id: &str) -> Value {
    json!({"model": "m", "input": [{"role": "user", "content": [
        {"type": "input_file", "file_id": id}
    ]}]})
}

#[tokio::test]
async fn a_generation_referencing_another_callers_file_is_refused_before_sending() {
    let h = harness(full(), "round_robin").await;
    let result = generate(
        &h,
        Dialect::OpenAi,
        "/v1/responses",
        responses_with_file("file-theirs"),
    )
    .await;
    match result {
        Err(CoreError::ResourceNotFound { id, .. }) => assert_eq!(id, "file-theirs"),
        other => panic!(
            "expected not-found, got {:?}",
            other.map_err(|e| e.to_string())
        ),
    }
    assert!(h.client.seen.lines().is_empty());
}

#[tokio::test]
async fn a_generation_referencing_owned_files_runs_on_the_credential_holding_them() {
    let h = harness(full(), "round_robin").await;
    h.own("p", "b", FILE_KIND, "file-b").await;
    for _ in 0..2 {
        h.script(vec![json_reply(StatusCode::OK, json!({"ok": true}))]);
        let status = generate(
            &h,
            Dialect::OpenAi,
            "/v1/responses",
            responses_with_file("file-b"),
        )
        .await
        .unwrap();
        assert_eq!(status, StatusCode::OK);
    }
    // Gemini names the same kind of file by its Files API URI.
    h.own("p", "b", FILE_KIND, "gem").await;
    h.script(vec![json_reply(StatusCode::OK, json!({"ok": true}))]);
    let status = generate(
        &h,
        Dialect::Gemini,
        "/v1beta/models/m:generateContent",
        json!({"contents": [{"role": "user", "parts": [
            {"fileData": {"mimeType": "text/plain", "fileUri": "https://generativelanguage.googleapis.com/v1beta/files/gem"}}
        ]}]}),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 3);
    assert!(
        seen.iter().all(|line| line.contains("auth=Bearer kb")),
        "{seen:?}"
    );
}

#[tokio::test]
async fn files_on_two_credentials_cannot_share_one_request() {
    let h = harness(full(), "round_robin").await;
    h.own("p", "a", FILE_KIND, "file-a").await;
    h.own("p", "b", FILE_KIND, "file-b").await;
    let result = generate(
        &h,
        Dialect::Claude,
        "/v1/messages",
        json!({"model": "m", "max_tokens": 8, "messages": [{"role": "user", "content": [
            {"type": "document", "source": {"type": "file", "file_id": "file-a"}},
            {"type": "document", "source": {"type": "file", "file_id": "file-b"}}
        ]}]}),
    )
    .await;
    match result {
        Err(CoreError::InvalidTarget(message)) => {
            assert!(
                message.contains("different upstream credentials"),
                "{message}"
            )
        }
        other => panic!(
            "expected a refusal, got {:?}",
            other.map_err(|e| e.to_string())
        ),
    }
    assert!(h.client.seen.lines().is_empty());
}

#[tokio::test]
async fn a_tool_argument_named_file_id_is_not_a_reference() {
    let h = harness(full(), "round_robin").await;
    h.script(vec![json_reply(StatusCode::OK, json!({"ok": true}))]);
    let status = generate(
        &h,
        Dialect::Claude,
        "/v1/messages",
        json!({"model": "m", "max_tokens": 8, "messages": [
            {"role": "assistant", "content": [
                {"type": "tool_use", "id": "t1", "name": "open", "input": {"file_id": "not-a-file"}}
            ]},
            {"role": "user", "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "ok"}]}
        ]}),
    )
    .await
    .unwrap();
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn a_multipart_form_naming_a_file_is_checked_like_json() {
    let h = harness(full(), "round_robin").await;
    let form = |id: &str| {
        let body = format!(
            "--B\r\ncontent-disposition: form-data; name=\"prompt\"\r\n\r\na cat\r\n\
             --B\r\ncontent-disposition: form-data; name=\"input_reference[file_id]\"\r\n\r\n{id}\r\n--B--\r\n"
        );
        let mut request = WireRequest {
            body: HttpBody::Bytes(Bytes::from(body)),
            ..wire(Method::POST, "/v1/videos")
        };
        request.headers.insert(
            "content-type",
            http::HeaderValue::from_static("multipart/form-data; boundary=B"),
        );
        request
    };
    assert_not_found(
        send(&h, Operation::CreateVideo, "tenant", form("file-theirs")).await,
        "file-theirs",
    );
    assert!(h.client.seen.lines().is_empty());

    h.own("p", "b", FILE_KIND, "file-mine").await;
    h.script(vec![json_reply(StatusCode::OK, json!({"id": "video_9"}))]);
    let (status, _) = send(&h, Operation::CreateVideo, "tenant", form("file-mine"))
        .await
        .unwrap();
    assert_eq!(status, StatusCode::OK);
    let seen = h.client.seen.lines();
    assert!(seen[0].contains("auth=Bearer kb"), "{seen:?}");
}
