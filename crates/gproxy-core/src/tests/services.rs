use std::time::Duration;

use bytes::Bytes;
use gproxy_channel_api::{
    BoxFuture, StateError, TransportError, UsageView, UsageWindow, WsDuplex, WsFrame,
};
use serde_json::json;

use super::memory::MemoryHost;
use crate::error::StoreError;
use crate::host::{
    CacheBackend, Capture, CaptureSink, CredentialId, CredentialRecord, CredentialStore,
    UpstreamTransport, UsageSink,
};
use crate::usage::Settlement;

impl CredentialStore for MemoryHost {
    fn load<'a>(&'a self, _: CredentialId) -> BoxFuture<'a, Result<CredentialRecord, StoreError>> {
        let record = self.state.lock().expect("state lock").credential.clone();
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
        self.state.lock().expect("state lock").lease_calls += 1;
        Box::pin(async { Ok(true) })
    }
}

impl CacheBackend for MemoryHost {
    fn get<'a>(&'a self, key: &'a str) -> BoxFuture<'a, Option<Vec<u8>>> {
        let value = self
            .state
            .lock()
            .expect("state lock")
            .cache
            .get(key)
            .cloned();
        Box::pin(async move { value })
    }

    fn set<'a>(&'a self, key: &'a str, value: Vec<u8>, ttl: Option<Duration>) -> BoxFuture<'a, ()> {
        let mut state = self.state.lock().expect("state lock");
        state.cache.insert(key.into(), value);
        if let Some(ttl) = ttl {
            state.cache_ttls.insert(key.into(), ttl.as_secs());
        }
        Box::pin(async {})
    }

    fn incr<'a>(&'a self, _: &'a str, _: i64, _: Option<Duration>) -> BoxFuture<'a, i64> {
        Box::pin(async { 0 })
    }
}

impl UpstreamTransport for MemoryHost {
    fn send<'a>(
        &'a self,
        request: http::Request<Bytes>,
    ) -> BoxFuture<'a, Result<http::Response<crate::ByteStream>, TransportError>> {
        let state = self.state.clone();
        Box::pin(async move {
            let (status, body) = if request.uri().path() == "/refresh" {
                (
                    http::StatusCode::OK,
                    Bytes::from_static(
                        br#"{"access_token":"fresh","expires_at":9223372036854775807}"#,
                    ),
                )
            } else {
                let authorization = request
                    .headers()
                    .get(http::header::AUTHORIZATION)
                    .expect("authorization header")
                    .to_str()
                    .expect("text authorization")
                    .to_owned();
                let mut state = state.lock().expect("state lock");
                state.authorizations.push(authorization);
                (
                    state.statuses.pop_front().unwrap_or(http::StatusCode::OK),
                    Bytes::from_static(br#"{"usage":true,"result":"ok"}"#),
                )
            };
            let stream: crate::ByteStream =
                Box::pin(futures_util::stream::once(async move { Ok(body) }));
            let mut response = http::Response::new(stream);
            *response.status_mut() = status;
            Ok(response)
        })
    }

    fn open_websocket<'a>(
        &'a self,
        _: http::Request<Bytes>,
    ) -> BoxFuture<'a, Result<Box<dyn WsDuplex>, TransportError>> {
        self.state.lock().expect("state lock").socket_opens += 1;
        let socket: Box<dyn WsDuplex> = Box::new(self.clone());
        Box::pin(async move { Ok(socket) })
    }
}

impl WsDuplex for MemoryHost {
    fn send<'a>(&'a mut self, frame: WsFrame) -> BoxFuture<'a, Result<(), TransportError>> {
        if matches!(frame, WsFrame::Close(_)) {
            self.state.lock().expect("state lock").socket_closed = true;
        }
        Box::pin(async { Ok(()) })
    }

    fn recv<'a>(&'a mut self) -> BoxFuture<'a, Result<Option<WsFrame>, TransportError>> {
        let mut state = self.state.lock().expect("state lock");
        let frame = if state.socket_closed {
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
            .push((capture.response_status, capture.response_body.clone()));
        Box::pin(async {})
    }
}

impl UsageView for MemoryHost {
    fn window<'a>(&'a self, _: i64) -> BoxFuture<'a, Result<UsageWindow, StateError>> {
        Box::pin(async { Ok(UsageWindow::default()) })
    }
}
