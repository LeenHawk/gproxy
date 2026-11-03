use crate::support;
use gproxy_client_sdk::*;
use protocol::{HttpBody, OperationKey, WireRequest, WireResponse, capability::CapabilityFuture};
use serde_json::{Value, json};
use std::{collections::VecDeque, sync::Mutex};
pub(crate) struct Reply {
    pub(crate) status: http::StatusCode,
    pub(crate) headers: http::HeaderMap,
    pub(crate) body: Vec<u8>,
}

impl Reply {
    pub(crate) fn json(body: Value) -> Self {
        Self {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: body.to_string().into_bytes(),
        }
    }
    pub(crate) fn sse(events: Vec<Value>) -> Self {
        Self {
            status: http::StatusCode::OK,
            headers: http::HeaderMap::new(),
            body: events
                .into_iter()
                .map(|event| format!("data: {event}\n\n"))
                .collect::<String>()
                .into_bytes(),
        }
    }
}

pub(crate) struct Script {
    pub(crate) replies: Mutex<VecDeque<Reply>>,
    pub(crate) requests: Mutex<Vec<Value>>,
}

impl Script {
    pub(crate) fn new(replies: Vec<Reply>) -> Self {
        Self {
            replies: Mutex::new(replies.into()),
            requests: Mutex::new(Vec::new()),
        }
    }
}

impl Backend for Script {
    fn send(
        &self,
        _: OperationKey,
        _: Option<&str>,
        request: WireRequest<HttpBody>,
    ) -> CapabilityFuture<'_, Result<WireResponse<HttpBody>>> {
        Box::pin(async move {
            let bytes = protocol::codec::read_http_body(request.body, DEFAULT_LIMITS)
                .await
                .unwrap();
            self.requests
                .lock()
                .unwrap()
                .push(serde_json::from_slice(&bytes).unwrap());
            let reply = self
                .replies
                .lock()
                .unwrap()
                .pop_front()
                .expect("unexpected retry or request");
            Ok(WireResponse {
                status: reply.status,
                headers: reply.headers,
                body: HttpBody::Bytes(reply.body.into()),
            })
        })
    }
}

pub(crate) fn answer() -> Value {
    let mut native = support::openai();
    native["id"] = json!("response_served");
    native["model"] = json!("served_model");
    native["created_at"] = json!(1770000000);
    native["output"] = json!([{"type":"message","id":"message_served","role":"assistant","status":"completed","content":[{"type":"output_text","text":"answer","annotations":[],"logprobs":[]}]}]);
    native
}

pub(crate) fn early_events(native: &Value) -> Vec<Value> {
    let mut created = native.clone();
    created["status"] = json!("in_progress");
    created["output"] = json!([]);
    created["usage"] = Value::Null;
    let mut events =
        vec![json!({"type":"response.created","sequence_number":0,"response":created})];
    events.extend(support::openai_events(native));
    events
}
