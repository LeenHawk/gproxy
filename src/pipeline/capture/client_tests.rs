use std::sync::Arc;

use arc_swap::ArcSwap;
use bytes::Bytes;

use super::*;
use crate::app::snapshot::{ControlPlaneSnapshot, LogSettings};
use crate::config::{CacheConfig, PersistenceConfig, RuntimeConfig, UpstreamConfig};
use crate::http::client::ConduitSocket;

struct WebSocketUpstream;

#[async_trait::async_trait]
impl UpstreamClient for WebSocketUpstream {
    async fn send(&self, _req: http::Request<Bytes>) -> Result<http::Response<Bytes>, ClientError> {
        Ok(http::Response::builder()
            .status(StatusCode::OK)
            .body(Bytes::from_static(b"buffered response"))
            .unwrap())
    }

    async fn send_streaming(
        &self,
        req: http::Request<Bytes>,
    ) -> Result<(StatusCode, http::HeaderMap, crate::http::client::RespStream), ClientError> {
        let is_error = req.uri().path().ends_with("/error");
        let chunks = if is_error {
            vec![Ok(Bytes::from_static(b"stream error"))]
        } else {
            vec![
                Ok(Bytes::from_static(b"stream ")),
                Ok(Bytes::from_static(b"response")),
            ]
        };
        Ok((
            if is_error {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::OK
            },
            http::HeaderMap::new(),
            Box::pin(futures_util::stream::iter(chunks)),
        ))
    }

    async fn send_websocket(
        &self,
        _req: http::Request<Bytes>,
    ) -> Result<http::Response<Bytes>, ClientError> {
        Ok(http::Response::builder()
            .status(StatusCode::OK)
            .header(http::header::CONTENT_TYPE, "text/event-stream")
            .body(Bytes::from_static(
                b"event: first\ndata: {\"type\":\"first\",\"access_token\":\"secret-1\"}\n\n\
                  event: second\ndata: {\"type\":\"second\",\"client_secret\":\"secret-2\"}\n\n",
            ))
            .unwrap())
    }

    async fn open_websocket(
        &self,
        _req: http::Request<Bytes>,
    ) -> Result<Box<dyn ConduitSocket>, ClientError> {
        Ok(Box::new(OpenSocket { received: 0 }))
    }
}

struct OpenSocket {
    received: usize,
}

#[async_trait::async_trait]
impl ConduitSocket for OpenSocket {
    async fn send_text(&mut self, _text: String) -> Result<(), ClientError> {
        Ok(())
    }

    async fn recv_text(&mut self) -> Option<Result<String, ClientError>> {
        let frames = [
            r#"{"type":"first","token":"secret-1"}"#,
            r#"{"type":"second","nested":{"api_key":"secret-2"}}"#,
        ];
        let frame = frames.get(self.received)?;
        self.received += 1;
        Some(Ok((*frame).into()))
    }
}

async fn state() -> (AppState, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let persistence: Arc<dyn crate::store::persistence::PersistenceBackend> = Arc::new(
        crate::store::persistence::DbPersistence::connect("sqlite::memory:")
            .await
            .unwrap(),
    );
    let upstream: Arc<dyn UpstreamClient> = Arc::new(WebSocketUpstream);
    let mut snapshot = ControlPlaneSnapshot::empty(1);
    snapshot.log_settings = LogSettings {
        enable_usage: false,
        enable_upstream_log: true,
        enable_upstream_log_body: true,
        enable_downstream_log: false,
        enable_downstream_log_body: false,
        disable_log_redaction: false,
    };
    let config = Arc::new(RuntimeConfig {
        host: "127.0.0.1".into(),
        port: 0,
        cache: CacheConfig::Memory,
        persistence: PersistenceConfig::Db {
            dsn: "sqlite::memory:".into(),
        },
        upstream: UpstreamConfig::from_proxy_url(None),
        instance_id: 0,
        max_attempts: crate::config::DEFAULT_MAX_ATTEMPTS,
        max_in_flight: crate::config::DEFAULT_MAX_IN_FLIGHT,
        trusted_proxies: Vec::new(),
        update_channel: "releases".into(),
        update_data_dir: dir.path().to_path_buf(),
        cors_origins: Vec::new(),
    });
    (
        AppState::new(
            config,
            Arc::new(crate::store::cache::MemoryCache::new()),
            persistence,
            upstream,
            Arc::new(ArcSwap::from_pointee(snapshot)),
            Arc::new(crate::channel::registry::ChannelRegistry::with_builtin()),
            Arc::new(crate::crypto::NoopCipher),
        ),
        dir,
    )
}

#[tokio::test]
async fn open_websocket_redacts_secrets_and_preserves_received_frame_boundaries() {
    let (state, _dir) = state().await;
    let client = CapturingClient::new(
        Arc::new(WebSocketUpstream),
        state.clone(),
        "ws-open".into(),
        7,
        9,
    );
    let request = http::Request::get("wss://upstream.test/v1/responses")
        .header("authorization", "Bearer secret")
        .body(Bytes::from_static(b"request frame"))
        .unwrap();

    let mut socket = client.open_websocket(request).await.unwrap();
    assert_eq!(
        socket.recv_text().await.transpose().unwrap().as_deref(),
        Some(r#"{"type":"first","token":"secret-1"}"#)
    );
    assert_eq!(
        socket.recv_text().await.transpose().unwrap().as_deref(),
        Some(r#"{"type":"second","nested":{"api_key":"secret-2"}}"#)
    );
    drop(socket);

    let mut captured = None;
    for _ in 0..100 {
        let rows = state
            .persistence
            .list_upstream_requests("ws-open")
            .await
            .unwrap();
        if rows.first().is_some_and(|row| row.response_body.is_some()) {
            captured = Some(rows);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let rows = captured.expect("WebSocket frame backfill did not finish");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].url, "wss://upstream.test/v1/responses");
    assert_eq!(rows[0].method, "GET");
    assert_eq!(rows[0].status, 101);
    assert_eq!(rows[0].provider_id, Some(7));
    assert_eq!(rows[0].credential_id, Some(9));
    assert_eq!(rows[0].body.as_deref(), Some("request frame"));
    let response = rows[0].response_body.as_deref().unwrap();
    let frames = response
        .lines()
        .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
        .collect::<Vec<_>>();
    assert_eq!(frames.len(), 2);
    assert_eq!(frames[0]["type"], "first");
    assert_eq!(frames[0]["token"], "[REDACTED]");
    assert_eq!(frames[1]["type"], "second");
    assert_eq!(frames[1]["nested"]["api_key"], "[REDACTED]");
    assert!(!response.contains("secret-1") && !response.contains("secret-2"));
}

#[tokio::test]
async fn send_websocket_records_returned_status_and_body() {
    let (state, _dir) = state().await;
    let client = CapturingClient::new(
        Arc::new(WebSocketUpstream),
        state.clone(),
        "ws-send".into(),
        7,
        9,
    );
    let request = http::Request::post("wss://upstream.test/v1/responses")
        .body(Bytes::from_static(b"request frame"))
        .unwrap();

    let response = client.send_websocket(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);

    let rows = state
        .persistence
        .list_upstream_requests("ws-send")
        .await
        .unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, 200);
    assert_eq!(rows[0].body.as_deref(), Some("request frame"));
    assert_eq!(
        rows[0].response_body.as_deref(),
        Some(
            "{\"access_token\":\"[REDACTED]\",\"type\":\"first\"}\n\
             {\"client_secret\":\"[REDACTED]\",\"type\":\"second\"}\n"
        )
    );
}

#[tokio::test]
async fn streaming_call_backfills_only_its_ordered_row() {
    use futures_util::StreamExt as _;

    let (state, _dir) = state().await;
    let client = CapturingClient::new(
        Arc::new(WebSocketUpstream),
        state.clone(),
        "multi-step".into(),
        7,
        9,
    );
    for path in ["step-1", "step-2"] {
        client
            .send(
                http::Request::post(format!("https://upstream.test/{path}"))
                    .body(Bytes::new())
                    .unwrap(),
            )
            .await
            .unwrap();
    }
    let (_, _, stream) = client
        .send_streaming(
            http::Request::post("https://upstream.test/stream")
                .body(Bytes::new())
                .unwrap(),
        )
        .await
        .unwrap();
    let pending = state
        .persistence
        .list_upstream_requests("multi-step")
        .await
        .unwrap();
    assert_eq!(pending.len(), 3, "stream row exists before first poll");
    assert_eq!(pending[2].response_body, None);
    let _: Vec<_> = stream.collect().await;

    let mut captured = None;
    for _ in 0..100 {
        let rows = state
            .persistence
            .list_upstream_requests("multi-step")
            .await
            .unwrap();
        if rows.get(2).is_some_and(|row| row.response_body.is_some()) {
            captured = Some(rows);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let rows = captured.expect("stream body backfill did not finish");
    assert_eq!(
        rows.iter().map(|row| row.url.as_str()).collect::<Vec<_>>(),
        vec![
            "https://upstream.test/step-1",
            "https://upstream.test/step-2",
            "https://upstream.test/stream",
        ]
    );
    assert!(
        rows.windows(2).all(|pair| pair[0].id < pair[1].id),
        "capture ids preserve custom-call order"
    );
    assert_eq!(rows[0].response_body.as_deref(), Some("buffered response"));
    assert_eq!(rows[1].response_body.as_deref(), Some("buffered response"));
    assert_eq!(rows[2].response_body.as_deref(), Some("stream response"));
}

#[tokio::test]
async fn non_success_stream_backfills_its_response_body() {
    use futures_util::StreamExt as _;

    let (state, _dir) = state().await;
    let client = CapturingClient::new(
        Arc::new(WebSocketUpstream),
        state.clone(),
        "stream-error".into(),
        7,
        9,
    );
    let (status, _, stream) = client
        .send_streaming(
            http::Request::post("https://upstream.test/error")
                .body(Bytes::new())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let _: Vec<_> = stream.collect().await;

    let mut captured = None;
    for _ in 0..100 {
        let rows = state
            .persistence
            .list_upstream_requests("stream-error")
            .await
            .unwrap();
        if rows.first().is_some_and(|row| row.response_body.is_some()) {
            captured = Some(rows);
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    }
    let rows = captured.expect("error stream body backfill did not finish");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, 400);
    assert_eq!(rows[0].response_body.as_deref(), Some("stream error"));
}
