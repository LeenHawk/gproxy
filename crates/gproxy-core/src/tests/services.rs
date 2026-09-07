use std::time::Duration;

use bytes::Bytes;
use gproxy_channel_api::{
    BoxFuture, QuotaWindow, StateError, TransportError, UsageView, UsageWindow, WsDuplex, WsFrame,
};
use serde_json::json;

use super::memory::{Captured, MemoryHost};
use crate::error::StoreError;
use crate::host::{
    CacheBackend, Capture, CaptureSink, CredentialId, CredentialRecord, CredentialStore,
    UpstreamTransport, UsageSink,
};
use crate::usage::Settlement;
use crate::{Continuation, ContinuationKey, ContinuationMeta, ContinuationStore, Spawner};

impl CredentialStore for MemoryHost {
    fn load<'a>(&'a self, id: CredentialId) -> BoxFuture<'a, Result<CredentialRecord, StoreError>> {
        let mut state = self.state.lock().expect("state lock");
        state.loaded_credentials.push(id);
        let mut record = state.credential.clone();
        record.id = id;
        Box::pin(async move { Ok(record) })
    }

    fn persist_rotation<'a>(
        &'a self,
        _: CredentialId,
        secret: serde_json::Value,
        version: u64,
    ) -> BoxFuture<'a, Result<(), StoreError>> {
        let state = self.state.clone();
        Box::pin(async move {
            let mut state = state.lock().expect("state lock");
            state.rotations.push(version);
            if state.conflict {
                state.conflict = false;
                state.credential.secret = json!({
                    "access_token": "peer",
                    "expires_at": i64::MAX
                });
                state.credential.version += 1;
                return Err(StoreError("version conflict".into()));
            }
            if state.credential.version != version {
                return Err(StoreError("version conflict".into()));
            }
            state.credential.secret = secret;
            state.credential.version += 1;
            Ok(())
        })
    }

    fn lease_refresh<'a>(
        &'a self,
        _: CredentialId,
        _: Duration,
    ) -> BoxFuture<'a, Result<bool, StoreError>> {
        let mut state = self.state.lock().expect("state lock");
        state.lease_calls += 1;
        let acquired = !state.peer_refresh_on_wait;
        Box::pin(async move { Ok(acquired) })
    }
}

impl CacheBackend for MemoryHost {
    fn get<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<Option<Vec<u8>>, StoreError>> {
        let value = self
            .state
            .lock()
            .expect("state lock")
            .cache
            .get(key)
            .cloned();
        Box::pin(async move { Ok(value) })
    }

    fn set<'a>(
        &'a self,
        key: &'a str,
        value: Vec<u8>,
        ttl: Option<Duration>,
    ) -> BoxFuture<'a, Result<(), StoreError>> {
        let mut state = self.state.lock().expect("state lock");
        state.cache.insert(key.into(), value);
        if let Some(ttl) = ttl {
            state.cache_ttls.insert(key.into(), ttl.as_secs());
        }
        Box::pin(async { Ok(()) })
    }

    fn delete<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Result<(), StoreError>> {
        let mut state = self.state.lock().expect("state lock");
        state.cache.remove(key);
        state.cache_ttls.remove(key);
        Box::pin(async { Ok(()) })
    }

    fn incr<'a>(
        &'a self,
        key: &'a str,
        by: i64,
        ttl: Option<Duration>,
    ) -> BoxFuture<'a, Result<i64, StoreError>> {
        let result = (|| {
            let mut state = self.state.lock().expect("state lock");
            let absent = !state.cache.contains_key(key);
            let current = match state.cache.get(key) {
                Some(value) => i64::from_be_bytes(
                    value
                        .as_slice()
                        .try_into()
                        .map_err(|_| StoreError("cache counter is not an i64".into()))?,
                ),
                None => 0,
            };
            let next = current
                .checked_add(by)
                .ok_or_else(|| StoreError("cache counter overflow".into()))?;
            state.cache.insert(key.into(), next.to_be_bytes().to_vec());
            if absent && let Some(ttl) = ttl {
                state.cache_ttls.insert(key.into(), ttl.as_secs());
            }
            Ok(next)
        })();
        Box::pin(async move { result })
    }

    fn compare_incr_and_set<'a>(
        &'a self,
        counter_key: &'a str,
        by: i64,
        state_key: &'a str,
        expected_state: Vec<u8>,
        state_value: Vec<u8>,
    ) -> BoxFuture<'a, Result<Option<i64>, StoreError>> {
        let result = (|| {
            let mut state = self.state.lock().expect("state lock");
            if state.cache.get(state_key) != Some(&expected_state) {
                return Ok(None);
            }
            let current = match state.cache.get(counter_key) {
                Some(value) => i64::from_be_bytes(
                    value
                        .as_slice()
                        .try_into()
                        .map_err(|_| StoreError("cache counter is not an i64".into()))?,
                ),
                None => 0,
            };
            let next = current
                .checked_add(by)
                .ok_or_else(|| StoreError("cache counter overflow".into()))?;
            state
                .cache
                .insert(counter_key.into(), next.to_be_bytes().to_vec());
            state.cache.insert(state_key.into(), state_value);
            state.cache_ttls.remove(counter_key);
            state.cache_ttls.remove(state_key);
            Ok(Some(next))
        })();
        Box::pin(async move { result })
    }

    fn compare_and_swap<'a>(
        &'a self,
        key: &'a str,
        expected: Option<Vec<u8>>,
        value: Option<Vec<u8>>,
        ttl: Option<Duration>,
    ) -> BoxFuture<'a, Result<bool, StoreError>> {
        let mut state = self.state.lock().expect("state lock");
        if state.cache.get(key) != expected.as_ref() {
            return Box::pin(async { Ok(false) });
        }
        match value {
            Some(value) => {
                state.cache.insert(key.into(), value);
                if let Some(ttl) = ttl {
                    state.cache_ttls.insert(key.into(), ttl.as_secs());
                }
            }
            None => {
                state.cache.remove(key);
                state.cache_ttls.remove(key);
            }
        }
        Box::pin(async { Ok(true) })
    }
}

impl UpstreamTransport for MemoryHost {
    fn send<'a>(
        &'a self,
        request: http::Request<Bytes>,
    ) -> BoxFuture<'a, Result<http::Response<crate::ByteStream>, TransportError>> {
        let state = self.state.clone();
        Box::pin(async move {
            if let Some(response) = super::refusal::scripted(&state, &request) {
                return Ok(response);
            }
            let realtime_call = request.uri().path() == "/v1/realtime/calls";
            let (status, bodies) = if request.uri().path() == "/refresh" {
                (
                    http::StatusCode::OK,
                    vec![Bytes::from_static(
                        br#"{"access_token":"fresh","expires_at":9223372036854775807}"#,
                    )],
                )
            } else {
                let path = request.uri().path().to_owned();
                let method = request.method().clone();
                let request_body = request.body().clone();
                let authorization = request
                    .headers()
                    .get(http::header::AUTHORIZATION)
                    .or_else(|| request.headers().get("x-api-key"))
                    .or_else(|| request.headers().get(http::header::COOKIE))
                    .expect("upstream authentication header")
                    .to_str()
                    .expect("text authorization")
                    .to_owned();
                let mut state = state.lock().expect("state lock");
                state
                    .upstream_requests
                    .push((request.headers().clone(), request.uri().to_string()));
                state.upstream_bodies.push(request_body.clone());
                state.upstream_bodies.push(request_body.clone());
                state.authorizations.push(authorization);
                if let Some(value) = request.headers().get("originator") {
                    state
                        .fingerprint_headers
                        .push(value.to_str().expect("fingerprint header text").to_owned());
                }
                let body = match (method, path.as_str()) {
                    (http::Method::POST, "/copilot-compact/chat/completions") => Bytes::from_static(
                        br#"{"id":"chat_1","object":"chat.completion","created":0,"model":"upstream-model","choices":[{"index":0,"message":{"role":"assistant","content":"conversation summary"},"finish_reason":"stop"}],"usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}"#,
                    ),
                    (http::Method::POST, "/v1/realtime/calls") => {
                        Bytes::from_static(b"v=answer")
                    }
                    (http::Method::POST, "/v1/files") => {
                        Bytes::from_static(br#"{"id":"file-1","object":"file"}"#)
                    }
                    (http::Method::GET, "/v1/files/file-1") => {
                        Bytes::from_static(br#"{"id":"file-1","object":"file"}"#)
                    }
                    (http::Method::DELETE, "/v1/files/file-1") => {
                        Bytes::from_static(br#"{"id":"file-1","deleted":true}"#)
                    }
                    (http::Method::GET, "/v1/models") => Bytes::from_static(
                        br#"{"object":"list","data":[{"id":"fresh-model","object":"model","display_name":"Fresh model","context_window":200000,"max_output_tokens":32000,"thinking_supported":true}]}"#,
                    ),
                    (http::Method::GET, "/api/v1/ai/cline/recommended-models") => {
                        Bytes::from_static(
                            br#"{"free":[{"id":"fresh-model","display_name":"Fresh model","context_window":200000,"max_output_tokens":32000}],"clinePass":[]}"#,
                        )
                    }
                    (http::Method::POST, "/v1/messages")
                        if serde_json::from_slice::<serde_json::Value>(&request_body)
                            .ok()
                            .and_then(|body| body.get("stream")?.as_bool())
                            .unwrap_or(false) =>
                    {
                        Bytes::from_static(
                            b"event: message_start\ndata: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"type\":\"message\",\"model\":\"claude-test\",\"role\":\"assistant\",\"content\":[],\"stop_reason\":null,\"stop_sequence\":null,\"usage\":{\"input_tokens\":10,\"output_tokens\":0}}}\n\nevent: content_block_start\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\nevent: content_block_delta\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"ok\"}}\n\nevent: content_block_stop\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\nevent: message_delta\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":5}}\n\nevent: message_stop\ndata: {\"type\":\"message_stop\"}\n\n",
                        )
                    }
                    (http::Method::POST, "/v1/messages") => Bytes::from_static(
                        br#"{"id":"msg_1","type":"message","role":"assistant","model":"claude-test","content":[{"type":"text","text":"ok"}],"stop_reason":"end_turn","usage":{"input_tokens":10,"output_tokens":5}}"#,
                    ),
                    (http::Method::POST, path)
                        if path.ends_with("/codex/responses")
                            && String::from_utf8_lossy(&request_body).contains("sparse_test") =>
                    {
                        Bytes::from_static(
                            b"event: response.created\ndata: {\"type\":\"response.created\",\"response\":{\"id\":\"resp_sparse\",\"created_at\":1,\"object\":\"response\",\"model\":\"gpt-test\",\"output\":[]}}\n\nevent: response.output_text.delta\ndata: {\"type\":\"response.output_text.delta\",\"output_index\":0,\"item_id\":\"msg_sparse\",\"content_index\":0,\"delta\":\"sparse_test\"}\n\nevent: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_sparse\",\"created_at\":1,\"object\":\"response\",\"model\":\"gpt-test\",\"output\":[]}}\n\n",
                        )
                    }
                    (http::Method::POST, path) if path.ends_with("/codex/responses") => {
                        Bytes::from_static(
                            b"event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"id\":\"resp_1\",\"created_at\":0,\"object\":\"response\",\"model\":\"gpt-test\",\"output\":[{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"assistant\",\"content\":[{\"type\":\"output_text\",\"text\":\"ok\",\"annotations\":[]}],\"status\":\"completed\"}],\"output_text\":\"ok\",\"status\":\"completed\",\"usage\":{\"input_tokens\":10,\"output_tokens\":5,\"total_tokens\":15,\"output_tokens_details\":{\"reasoning_tokens\":2}}}}\n\n",
                        )
                    }
                    (http::Method::GET, path) if path.ends_with("/codex/models") => {
                        Bytes::from_static(
                            br#"{"models":[{"slug":"gpt-test","display_name":"GPT Test","context_window":128000,"max_context_window":256000,"future_catalog":"kept"}],"future_list":"kept"}"#,
                        )
                    }
                    _ => Bytes::from_static(br#"{"usage":true,"result":"ok"}"#),
                };
                let bodies = if path.ends_with("/completion") {
                    vec![
                        Bytes::from_static(
                            b"data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg-web\",\"content\":[]}}\n\ndata: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"tool_use\",\"id\":\"toolu-web\",\"name\":\"weather\"}}\n\ndata: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{}\"}}\n\ndata: {\"type\":\"content_block_stop\",\"index\":0}\n\n",
                        ),
                        Bytes::from_static(
                            b"data: {\"type\":\"content_block_start\",\"index\":1,\"content_block\":{\"type\":\"tool_result\",\"tool_use_id\":\"toolu-web\"}}\n\ndata: {\"type\":\"content_block_stop\",\"index\":1}\n\ndata: {\"type\":\"content_block_start\",\"index\":2,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}\n\ndata: {\"type\":\"content_block_delta\",\"index\":2,\"delta\":{\"type\":\"text_delta\",\"text\":\"Sunny\"}}\n\ndata: {\"type\":\"content_block_stop\",\"index\":2}\n\ndata: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"},\"usage\":{\"output_tokens\":2}}\n\ndata: {\"type\":\"message_stop\"}\n\n",
                        ),
                    ]
                } else {
                    vec![body]
                };
                (
                    state.statuses.pop_front().unwrap_or(http::StatusCode::OK),
                    bodies,
                )
            };
            let stream: crate::ByteStream =
                Box::pin(futures_util::stream::iter(bodies.into_iter().map(Ok)));
            let mut response = http::Response::new(stream);
            *response.status_mut() = status;
            if realtime_call {
                response.headers_mut().insert(
                    http::header::LOCATION,
                    "/v1/realtime/calls/rtc_test"
                        .parse()
                        .expect("test Location"),
                );
            }
            response
                .headers_mut()
                .insert("x-test-visible", "kept".parse().unwrap());
            response
                .headers_mut()
                .insert("x-test-hidden", "dropped".parse().unwrap());
            response
                .headers_mut()
                .insert("set-cookie", "session=secret".parse().unwrap());
            Ok(response)
        })
    }

    fn open_websocket<'a>(
        &'a self,
        _: http::Request<Bytes>,
    ) -> BoxFuture<'a, Result<Box<dyn WsDuplex>, TransportError>> {
        let status = {
            let mut state = self.state.lock().expect("state lock");
            state.socket_opens += 1;
            state.socket_statuses.pop_front().unwrap_or(101)
        };
        if status != 101 {
            return Box::pin(async move { Err(TransportError::Status(status)) });
        }
        let socket: Box<dyn WsDuplex> = Box::new(self.clone());
        Box::pin(async move { Ok(socket) })
    }
}

impl WsDuplex for MemoryHost {
    fn send<'a>(&'a mut self, frame: WsFrame) -> BoxFuture<'a, Result<(), TransportError>> {
        if let WsFrame::Text(text) = &frame {
            self.state
                .lock()
                .expect("state lock")
                .socket_sent
                .push(text.clone());
        }
        if matches!(frame, WsFrame::Close(_)) {
            self.state.lock().expect("state lock").socket_closed = true;
        }
        Box::pin(async { Ok(()) })
    }

    fn recv<'a>(&'a mut self) -> BoxFuture<'a, Result<Option<WsFrame>, TransportError>> {
        let mut state = self.state.lock().expect("state lock");
        let frame = if let Some(frame) = state.socket_frames.pop_front() {
            Some(frame)
        } else if state.socket_closed {
            None
        } else {
            state.socket_closed = true;
            Some(WsFrame::Close(Some(1000)))
        };
        Box::pin(async move { Ok(frame) })
    }
}

impl UsageSink for MemoryHost {
    fn record<'a>(&'a self, settlement: &'a Settlement) -> BoxFuture<'a, ()> {
        self.state
            .lock()
            .expect("state lock")
            .settlements
            .push(settlement.clone());
        Box::pin(async {})
    }
}

impl CaptureSink for MemoryHost {
    fn record<'a>(&'a self, capture: &'a Capture) -> BoxFuture<'a, ()> {
        self.state
            .lock()
            .expect("state lock")
            .captures
            .push(Captured {
                status: capture.response_status,
                body: capture.response_body.clone(),
                provider_id: capture.provider_id,
                credential_id: capture.credential_id,
            });
        Box::pin(async {})
    }
}

impl UsageView for MemoryHost {
    fn window<'a>(&'a self, _: i64) -> BoxFuture<'a, Result<UsageWindow, StateError>> {
        Box::pin(async { Ok(UsageWindow::default()) })
    }

    fn quota_windows<'a>(&'a self) -> BoxFuture<'a, Result<Vec<QuotaWindow>, StateError>> {
        let windows = self.state.lock().expect("state lock").quota_windows.clone();
        Box::pin(async move { Ok(windows) })
    }
}

impl ContinuationStore for MemoryHost {
    fn peek(&self, key: &ContinuationKey) -> Result<Option<ContinuationMeta>, StoreError> {
        Ok(self
            .state
            .lock()
            .expect("state lock")
            .continuations
            .get(key)
            .map(Continuation::meta))
    }

    fn put(
        &self,
        value: Continuation,
    ) -> Result<Option<Continuation>, (StoreError, Box<Continuation>)> {
        let key = value.key().clone();
        Ok(self
            .state
            .lock()
            .expect("state lock")
            .continuations
            .insert(key, value))
    }

    fn take(&self, key: &ContinuationKey) -> Result<Option<Continuation>, StoreError> {
        Ok(self
            .state
            .lock()
            .expect("state lock")
            .continuations
            .remove(key))
    }

    fn take_generation(
        &self,
        key: &ContinuationKey,
        generation: &str,
    ) -> Result<Option<Continuation>, StoreError> {
        let mut state = self.state.lock().expect("state lock");
        if state
            .continuations
            .get(key)
            .is_some_and(|value| value.meta().generation == generation)
        {
            Ok(state.continuations.remove(key))
        } else {
            Ok(None)
        }
    }
}

impl Spawner for MemoryHost {
    fn reserve_settlement(&self) -> BoxFuture<'_, crate::SettlementPermit> {
        Box::pin(async { Box::new(()) as crate::SettlementPermit })
    }

    fn spawn(&self, task: std::pin::Pin<Box<dyn Future<Output = ()> + Send>>) {
        let run = {
            let mut state = self.state.lock().expect("state lock");
            if state.drop_spawn_once {
                state.drop_spawn_once = false;
                state.run_spawned = true;
                false
            } else {
                state.run_spawned
            }
        };
        if run {
            crate::tests::block_on(task);
        }
    }
}
