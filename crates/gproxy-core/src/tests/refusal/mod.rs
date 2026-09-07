mod billing;
mod credit;
mod retry;
mod streaming;

use std::sync::{Arc, Mutex};

use bytes::Bytes;
use futures_util::StreamExt;
use http::{HeaderMap, StatusCode};
use serde_json::{Value, json};

use super::memory::{MemoryHost, State};
use super::{block_on, request};
use crate::control::{FailoverBudget, Plan, ProviderRef, Target};
use crate::{Core, ResponseBody};

pub(super) fn scripted(
    state: &Arc<Mutex<State>>,
    request: &http::Request<Bytes>,
) -> Option<http::Response<crate::ByteStream>> {
    let mut state = state.lock().expect("state");
    let (status, chunks) = state.scripted.pop_front()?;
    state
        .upstream_requests
        .push((request.headers().clone(), request.uri().to_string()));
    state.upstream_bodies.push(request.body().clone());
    let mut stream =
        Box::pin(futures_util::stream::iter(chunks.into_iter().map(Ok))) as crate::ByteStream;
    if state.scripted_pending_at == Some(state.upstream_requests.len()) {
        stream = Box::pin(stream.chain(futures_util::stream::pending()));
    }
    let mut response = http::Response::new(stream);
    *response.status_mut() = status;
    Some(response)
}

fn setup(
    channel: impl gproxy_channel_api::Channel + 'static,
    settings: Value,
) -> (MemoryHost, Core<MemoryHost>) {
    let channel_id = channel.descriptor().id;
    let host = MemoryHost::new(false);
    let mut state = host.state.lock().expect("state");
    state.credential.channel = channel_id.into();
    state.credential.secret["expires_at"] = json!(i64::MAX);
    state.credential.secret["api_key"] = state.credential.secret["access_token"].clone();
    let target = Target {
        provider: ProviderRef {
            id: 1,
            name: "fallback".into(),
            channel: channel_id.into(),
            settings,
            fingerprint: None,
            proxy_url: None,
            traffic_blacklist: Default::default(),
        },
        credential: state.credential.id,
        upstream_model: "claude-fable-5".into(),
        tier: 0,
        rules: Default::default(),
    };
    state.plan = Some(Plan {
        targets: vec![target],
        budget: FailoverBudget { max_attempts: 6 },
    });
    drop(state);
    let channel = Box::new(channel) as Box<dyn gproxy_channel_api::Channel>;
    let core = Core::new(
        host.clone(),
        gproxy_channel_api::ChannelRegistry::new([channel]).unwrap(),
    )
    .unwrap();
    (host, core)
}

fn message(model: &str, reason: &str, text: &str, input: u64, output: u64) -> Value {
    json!({"id":"msg_test","type":"message","role":"assistant","model":model,
        "content":if text.is_empty() { json!([]) } else { json!([{"type":"text","text":text}]) },
        "stop_reason":reason,"stop_sequence":null,
        "stop_details":if reason == "refusal" { json!({"type":"refusal","category":null,"explanation":null}) } else { Value::Null },
        "usage":{"input_tokens":input,"output_tokens":output}})
}

fn enqueue(host: &MemoryHost, body: Value, streaming: bool) {
    let bytes = Bytes::from(body.to_string());
    let frames = if streaming {
        gproxy_transform::synthesize_response(
            gproxy_protocol::ContentGenerationKind::ClaudeMessages,
            bytes,
            gproxy_protocol::StreamFraming::Sse,
        )
        .unwrap()
    } else {
        vec![bytes]
    };
    let chunks = frames
        .into_iter()
        .flat_map(|frame| {
            frame
                .chunks(11)
                .map(Bytes::copy_from_slice)
                .collect::<Vec<_>>()
        })
        .collect();
    host.state
        .lock()
        .unwrap()
        .scripted
        .push_back((StatusCode::OK, chunks));
}

fn execute(host: &MemoryHost, core: &Core<MemoryHost>, streaming: bool) -> (StatusCode, Value) {
    let mut input = request(streaming, "fallback-test");
    input.path = "/v1/messages".into();
    input.headers = HeaderMap::new();
    input.body = Bytes::from(json!({"model":"claude-fable-5","max_tokens":128,"stream":streaming,"messages":[{"role":"user","content":"hello"}]}).to_string());
    let result = block_on(core.execute(host, input)).unwrap();
    let bytes = match result.body {
        ResponseBody::Full(body) => body,
        ResponseBody::Stream(mut stream) => block_on(async {
            let mut bytes = Vec::new();
            while let Some(chunk) = stream.next().await {
                bytes.extend_from_slice(&chunk.unwrap());
            }
            Bytes::from(bytes)
        }),
        ResponseBody::WebSocket(_) => panic!("unexpected websocket"),
    };
    let body = if streaming && result.status.is_success() {
        let mut collector = gproxy_transform::ResponseCollector::new(
            gproxy_protocol::ContentGenerationKind::ClaudeMessages,
        )
        .unwrap();
        collector.push(bytes).unwrap();
        serde_json::from_slice(&collector.finish().unwrap().into_bytes().unwrap()).unwrap()
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (result.status, body)
}
