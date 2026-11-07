//! A backend that answers with scripted bodies and records what it was sent.
use gproxy_client_sdk::*;
use gproxy_protocol::{
    HttpBody, OperationKey, WireRequest, WireResponse, capability::CapabilityFuture,
    connection::Bytes,
};
use http::{Method, StatusCode};
use serde_json::Value;
use std::{collections::VecDeque, sync::Mutex};

#[derive(Debug)]
pub struct Recorded {
    pub key: OperationKey,
    pub model: Option<String>,
    pub method: Method,
    pub path: String,
    pub query: Option<String>,
    pub headers: http::HeaderMap,
    pub body: Bytes,
}
pub struct BackendScript {
    pub replies: Mutex<VecDeque<Bytes>>,
    pub requests: Mutex<Vec<Recorded>>,
}
impl BackendScript {
    pub fn new(replies: Vec<Value>) -> Self {
        Self {
            replies: Mutex::new(replies.into_iter().map(|v| v.to_string().into()).collect()),
            requests: Mutex::new(vec![]),
        }
    }
    pub fn take(&self) -> Recorded {
        self.requests.lock().unwrap().remove(0)
    }
}
impl Backend for BackendScript {
    fn send(
        &self,
        key: OperationKey,
        model: Option<&str>,
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'_, Result<WireResponse<HttpBody>>> {
        let model = model.map(str::to_owned);
        Box::pin(async move {
            let body = gproxy_protocol::codec::read_http_body(request.body, DEFAULT_LIMITS)
                .await
                .unwrap();
            self.requests.lock().unwrap().push(Recorded {
                key,
                model,
                method: request.method,
                path: request.path,
                query: request.query,
                headers: request.headers,
                body,
            });
            Ok(WireResponse {
                status: StatusCode::OK,
                headers: http::HeaderMap::new(),
                body: HttpBody::Bytes(self.replies.lock().unwrap().pop_front().unwrap()),
            })
        })
    }
}
