#![cfg(not(target_arch = "wasm32"))]
//! Gemini resumable uploads through the whole router
//! (`gproxy_core::owned::upload`): the start's upstream upload URL is replaced
//! by a gateway session bound to the caller, the follow-up is proxied to the
//! upstream session on the credential that started it, and the finalized file
//! is the caller's — reachable by them, not by anyone else.

mod support;

use axum::body::Body;
use http::{Method, Request, StatusCode};
use serde_json::json;
use support::{Host, Reply, keyed};

const UPSTREAM_SESSION: &str =
    "https://p1.example/upload/v1beta/files?upload_id=UPSTREAM-1&upload_protocol=resumable";

async fn instance() -> Host {
    let host = Host::new().await;
    let handle = host.handle();
    for (user, key) in [("alice", "k-alice"), ("bob", "k-bob")] {
        support::person(&handle, user, "user").await;
        support::api_key(&handle, key, user, None, None).await;
        support::allow(&handle, &format!("perm-{user}"), user, None).await;
    }
    support::provider(&handle, "p1", &["m1"]).await;
    support::credential(&handle, "c1", "p1", None, None, None).await;
    host.publish().await;
    host
}

fn upload(uri: &str, command: &str, body: Vec<u8>) -> Request<Body> {
    Request::builder()
        .method(Method::POST)
        .uri(uri)
        .header("host", support::HOST)
        .header("x-goog-upload-protocol", "resumable")
        .header("x-goog-upload-command", command)
        .header("content-type", "application/octet-stream")
        .body(Body::from(body))
        .unwrap()
}

/// The gateway URL a start hands back, as a path and query on this host.
async fn start(host: &Host, key: &str) -> String {
    host.client.script(vec![Reply::Headed(
        StatusCode::OK,
        vec![
            ("x-goog-upload-url", UPSTREAM_SESSION.into()),
            ("x-goog-upload-status", "active".into()),
        ],
        json!({}),
    )]);
    let answer = host
        .send(keyed(
            upload(
                "/upload/v1beta/files",
                "start",
                serde_json::to_vec(&json!({"file": {"display_name": "a.txt"}})).unwrap(),
            ),
            key,
        ))
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    let url = answer.header("x-goog-upload-url").unwrap().to_owned();
    assert!(
        !url.contains("UPSTREAM-1"),
        "the upstream session leaked: {url}"
    );
    let path = url
        .strip_prefix(&format!("http://{}", support::HOST))
        .unwrap_or_else(|| panic!("an absolute gateway URL: {url}"));
    assert!(
        path.starts_with("/upload/v1beta/files?upload_id=gproxy-upload-"),
        "{path}"
    );
    path.to_owned()
}

#[tokio::test]
async fn a_resumable_upload_is_proxied_and_the_file_belongs_to_the_uploader() {
    let host = instance().await;
    let session = start(&host, "k-alice").await;

    // Someone else cannot ride the session.
    let answer = host
        .send(keyed(
            upload(&session, "upload, finalize", b"abc".to_vec()),
            "k-bob",
        ))
        .await;
    assert_eq!(answer.status, StatusCode::NOT_FOUND, "{}", answer.text());
    assert_eq!(host.client.urls().len(), 1);

    // The uploader's bytes go to the upstream session; the finalized file is
    // registered for them.
    host.client.script(vec![Reply::Headed(
        StatusCode::OK,
        vec![("x-goog-upload-status", "final".into())],
        json!({"file": {"name": "files/abc", "uri": "https://p1.example/v1beta/files/abc"}}),
    )]);
    let answer = host
        .send(keyed(
            upload(&session, "upload, finalize", b"abc".to_vec()),
            "k-alice",
        ))
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());
    assert_eq!(answer.json()["file"]["name"], "files/abc");
    assert_eq!(host.client.urls()[1], UPSTREAM_SESSION);

    host.client.script(vec![Reply::Http(
        StatusCode::OK,
        json!({"name": "files/abc"}),
    )]);
    let answer = host
        .send(keyed(support::get("/v1beta/files/abc"), "k-alice"))
        .await;
    assert_eq!(answer.status, StatusCode::OK, "{}", answer.text());

    let answer = host
        .send(keyed(support::get("/v1beta/files/abc"), "k-bob"))
        .await;
    assert_eq!(answer.status, StatusCode::NOT_FOUND, "{}", answer.text());
    assert_eq!(answer.json()["error"]["code"], "not_found");
    assert_eq!(host.client.urls().len(), 3);
}

#[tokio::test]
async fn an_unknown_upload_session_is_not_found() {
    let host = instance().await;
    let answer = host
        .send(keyed(
            upload(
                "/upload/v1beta/files?upload_id=gproxy-upload-forged&upload_protocol=resumable",
                "upload, finalize",
                b"abc".to_vec(),
            ),
            "k-alice",
        ))
        .await;
    assert_eq!(answer.status, StatusCode::NOT_FOUND, "{}", answer.text());
    assert!(host.client.urls().is_empty());
}
