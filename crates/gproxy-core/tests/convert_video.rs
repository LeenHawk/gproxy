#![cfg(not(target_arch = "wasm32"))]

//! video family: the OpenAI native video API converted to Veo long-running
//! operations on a Gemini provider, with job state in the protocol state store.

mod support;
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest, connection::Bytes};
use http::{HeaderMap, HeaderValue, Method, StatusCode};
use serde_json::{Value, json};
use support::*;

fn mp4() -> &'static [u8] {
    b"\0\0\0\x10ftypisom\0\0\0\0"
}

fn mp4_base64() -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(mp4())
}

fn json_request(method: Method, path: &str, body: Option<Value>) -> WireRequest<HttpBody> {
    let mut headers = HeaderMap::new();
    let body = match body {
        Some(body) => {
            headers.insert("content-type", HeaderValue::from_static("application/json"));
            Bytes::from(serde_json::to_vec(&body).unwrap())
        }
        None => Bytes::new(),
    };
    WireRequest {
        method,
        path: path.into(),
        query: None,
        headers,
        body: HttpBody::Bytes(body),
    }
}

async fn send(
    h: &Harness,
    operation: Operation,
    request: WireRequest<HttpBody>,
) -> gproxy_core::CoreResult<gproxy_core::HttpExecution> {
    let ctx = h.context_for(
        "gemini",
        OperationKey {
            operation,
            dialect: Dialect::OpenAi,
        },
        "r",
        1,
        None,
    );
    match operation {
        Operation::CreateVideo => h.core.create_video(ctx, request).await,
        Operation::RetrieveVideo => h.core.retrieve_video(ctx, request).await,
        Operation::DownloadVideoContent => h.core.download_video_content(ctx, request).await,
        Operation::ListVideos => h.core.list_videos(ctx, request).await,
        Operation::DeleteVideo => h.core.delete_video(ctx, request).await,
        other => panic!("not a video operation: {other:?}"),
    }
}

async fn run(
    h: &Harness,
    operation: Operation,
    request: WireRequest<HttpBody>,
) -> (StatusCode, HeaderMap, Vec<u8>) {
    let execution = send(h, operation, request).await.unwrap();
    let (response, completion) = execution.into_parts();
    let body = read(response.body).await;
    let _ = completion.await;
    (response.status, response.headers, body.into_bytes())
}

fn value(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap()
}

#[tokio::test]
async fn create_poll_and_download_a_veo_job_through_the_native_video_api() {
    let h = harness(full(), "round_robin").await;
    seed_provider(&h, "gemini", "https://gemini.example", "gemini").await;
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({"name": "operations/abc", "done": false}),
    )]);
    let (status, _, body) = run(
        &h,
        Operation::CreateVideo,
        json_request(
            Method::POST,
            "/v1/videos",
            Some(
                json!({"model": "sora-2", "prompt": "sunset", "seconds": "4", "size": "1280x720"}),
            ),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let created = value(&body);
    let id = created["id"].as_str().unwrap().to_owned();
    assert!(id.starts_with("video_"), "{created}");
    assert_eq!(created["object"], "video");
    assert_eq!(created["status"], "queued");
    assert_eq!(created["model"], "sora-2");
    assert_eq!(created["seconds"], "4");
    assert_eq!(created["size"], "1280x720");
    assert_eq!(created["progress"], 0);
    assert_eq!(created["prompt"], "sunset");
    assert!(created["created_at"].as_i64().unwrap() > 0);
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 1);
    assert!(
        seen[0].starts_with(
            "POST https://gemini.example/v1beta/models/gpt-x:predictLongRunning auth=Bearer k-gemini"
        ),
        "{}",
        seen[0]
    );
    assert!(
        seen[0].contains("\"prompt\":\"sunset\"") && seen[0].contains("\"durationSeconds\":4"),
        "{}",
        seen[0]
    );

    // Not finished: no content yet.
    let Err(error) = send(
        &h,
        Operation::DownloadVideoContent,
        json_request(Method::GET, &format!("/v1/videos/{id}/content"), None),
    )
    .await
    else {
        panic!("content before completion must fail")
    };
    assert!(error.to_string().contains("not completed"), "{error}");

    // Poll: the upstream operation completes with inline bytes.
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({
            "name": "operations/abc", "done": true,
            "response": {"generateVideoResponse": {"generatedSamples": [
                {"video": {"encodedVideo": mp4_base64(), "encoding": "video/mp4"}}
            ]}}
        }),
    )]);
    let (status, _, body) = run(
        &h,
        Operation::RetrieveVideo,
        json_request(Method::GET, &format!("/v1/videos/{id}"), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let done = value(&body);
    assert_eq!(done["id"], id);
    assert_eq!(done["status"], "completed");
    assert_eq!(done["progress"], 100);
    assert_eq!(done["created_at"], created["created_at"]);
    assert!(done["completed_at"].as_i64().unwrap() >= created["created_at"].as_i64().unwrap());
    let seen = h.client.seen.lines();
    assert_eq!(seen.len(), 2);
    assert!(
        seen[1]
            .starts_with("GET https://gemini.example/v1beta/operations/abc auth=Bearer k-gemini"),
        "{}",
        seen[1]
    );

    // Download serves the saved bytes without another upstream call.
    let (status, headers, body) = run(
        &h,
        Operation::DownloadVideoContent,
        json_request(Method::GET, &format!("/v1/videos/{id}/content"), None),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(headers["content-type"], "video/mp4");
    assert_eq!(body, mp4());
    assert_eq!(h.client.seen.lines().len(), 2);

    // Polling again after completion is a plain re-read of the operation.
    h.script(vec![json_reply(
        StatusCode::OK,
        json!({
            "name": "operations/abc", "done": true,
            "response": {"generateVideoResponse": {"generatedSamples": [
                {"video": {"encodedVideo": mp4_base64(), "encoding": "video/mp4"}}
            ]}}
        }),
    )]);
    let (_, _, body) = run(
        &h,
        Operation::RetrieveVideo,
        json_request(Method::GET, &format!("/v1/videos/{id}"), None),
    )
    .await;
    let again = value(&body);
    assert_eq!(again["completed_at"], done["completed_at"]);
}

#[tokio::test]
async fn unknown_jobs_multipart_creates_and_list_delete_are_refused() {
    let h = harness(full(), "round_robin").await;
    seed_provider(&h, "gemini", "https://gemini.example", "gemini").await;
    let Err(error) = send(
        &h,
        Operation::RetrieveVideo,
        json_request(Method::GET, "/v1/videos/video_nope", None),
    )
    .await
    else {
        panic!("unknown job must fail")
    };
    assert!(error.to_string().contains("job_state"), "{error}");

    let mut headers = HeaderMap::new();
    headers.insert(
        "content-type",
        HeaderValue::from_static("multipart/form-data; boundary=XYZ"),
    );
    let Err(error) = send(
        &h,
        Operation::CreateVideo,
        WireRequest {
            method: Method::POST,
            path: "/v1/videos".into(),
            query: None,
            headers,
            body: HttpBody::Bytes(Bytes::from_static(b"--XYZ--\r\n")),
        },
    )
    .await
    else {
        panic!("multipart create must be refused")
    };
    assert!(error.to_string().contains("multipart"), "{error}");

    for operation in [Operation::ListVideos, Operation::DeleteVideo] {
        let Err(error) = send(&h, operation, json_request(Method::GET, "/v1/videos", None)).await
        else {
            panic!("{operation:?} must be refused")
        };
        assert!(error.to_string().contains("not supported"), "{error}");
    }
    assert!(h.client.seen.lines().is_empty());
}

#[tokio::test]
async fn upstream_rejection_of_the_create_call_is_passed_through() {
    let h = harness(full(), "round_robin").await;
    seed_provider(&h, "gemini", "https://gemini.example", "gemini").await;
    h.script(vec![json_reply(
        StatusCode::BAD_REQUEST,
        json!({"error": {"code": 400, "message": "bad prompt"}}),
    )]);
    let (status, _, body) = run(
        &h,
        Operation::CreateVideo,
        json_request(
            Method::POST,
            "/v1/videos",
            Some(json!({"prompt": "sunset"})),
        ),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(value(&body)["error"]["message"], "bad prompt");
}
