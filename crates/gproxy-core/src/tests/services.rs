use std::time::Duration;

use bytes::Bytes;
use gproxy_channel_api::{BoxFuture, TransportError, WsDuplex};
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
    fn get<'a>(&'a self, _: &'a str) -> BoxFuture<'a, Option<Vec<u8>>> {
        Box::pin(async { None })
    }

    fn set<'a>(&'a self, _: &'a str, _: Vec<u8>, _: Option<Duration>) -> BoxFuture<'a, ()> {
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
            let body = if request.uri().path() == "/refresh" {
                Bytes::from_static(br#"{"access_token":"fresh","expires_at":9223372036854775807}"#)
            } else {
                let authorization = request
                    .headers()
                    .get(http::header::AUTHORIZATION)
                    .expect("authorization header")
                    .to_str()
                    .expect("text authorization")
                    .to_owned();
                state
                    .lock()
                    .expect("state lock")
                    .authorizations
                    .push(authorization);
                Bytes::from_static(br#"{"usage":true,"result":"ok"}"#)
            };
            let stream: crate::ByteStream =
                Box::pin(futures_util::stream::once(async move { Ok(body) }));
            Ok(http::Response::new(stream))
        })
    }

    fn open_websocket<'a>(
        &'a self,
        _: http::Request<Bytes>,
    ) -> BoxFuture<'a, Result<Box<dyn WsDuplex>, TransportError>> {
        Box::pin(async { Err(TransportError::Connect("unused".into())) })
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
