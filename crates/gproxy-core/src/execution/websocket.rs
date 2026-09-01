use std::collections::VecDeque;

use bytes::Bytes;
use futures_util::StreamExt as _;
use gproxy_channel_api::{BoxFuture, TransportError, WsDuplex, WsFrame};
use gproxy_protocol::{ContentGenerationKind, OperationKind};

use crate::api::Core;
use crate::boundary::{ExecOutcome, RequestCtx, ResponseBody, RoutingMode};
use crate::control::{ControlPlane, Plan};
use crate::error::CoreError;
use crate::host::Host;

use super::request::Classified;

pub(super) fn run<H: Host>(
    core: &Core<H>,
    control: &dyn ControlPlane,
    ctx: RequestCtx,
    plan: Plan,
    classified: Classified,
    _identity: gproxy_channel_api::CallerIdentity,
) -> Result<ExecOutcome, CoreError> {
    if classified.key.kind
        != OperationKind::ContentGeneration(ContentGenerationKind::OpenAiResponsesWebSocket)
    {
        return Err(CoreError::Unsupported);
    }
    Ok(crate::funnel::bridged_websocket(Box::new(
        ResponsesBridge {
            core: core.clone(),
            control: control.detached(),
            plan,
            headers: clean_headers(ctx.headers),
            mode: ctx.mode,
            request_id: ctx.request_id,
            sequence: 0,
            active: None,
            pending: String::new(),
            queued: VecDeque::new(),
            closed: false,
        },
    )))
}

struct ResponsesBridge<H: Host> {
    core: Core<H>,
    control: Box<dyn ControlPlane>,
    plan: Plan,
    headers: http::HeaderMap,
    mode: RoutingMode,
    request_id: String,
    sequence: u64,
    active: Option<crate::boundary::ByteStream>,
    pending: String,
    queued: VecDeque<WsFrame>,
    closed: bool,
}

impl<H: Host> WsDuplex for ResponsesBridge<H> {
    fn send<'a>(&'a mut self, frame: WsFrame) -> BoxFuture<'a, Result<(), TransportError>> {
        Box::pin(async move {
            match frame {
                WsFrame::Close(code) => {
                    self.closed = true;
                    self.active = None;
                    self.queued.push_back(WsFrame::Close(code));
                    Ok(())
                }
                WsFrame::Binary(_) => Err(TransportError::Interrupted(
                    "Responses websocket accepts text frames only".into(),
                )),
                WsFrame::Text(text) => self.start(text).await,
            }
        })
    }

    fn recv<'a>(&'a mut self) -> BoxFuture<'a, Result<Option<WsFrame>, TransportError>> {
        Box::pin(async move {
            loop {
                if let Some(frame) = self.queued.pop_front() {
                    return Ok(Some(frame));
                }
                if self.closed {
                    return Ok(None);
                }
                let Some(active) = self.active.as_mut() else {
                    std::future::pending::<()>().await;
                    continue;
                };
                match active.next().await {
                    Some(Ok(chunk)) => {
                        let text = std::str::from_utf8(&chunk).map_err(|_| {
                            TransportError::Interrupted("upstream SSE was not UTF-8".into())
                        })?;
                        self.pending.push_str(text);
                        let terminal = drain_sse(&mut self.pending, &mut self.queued);
                        if terminal {
                            self.active = None;
                        }
                    }
                    Some(Err(error)) => {
                        self.active = None;
                        return Err(error);
                    }
                    None => {
                        self.active = None;
                    }
                }
            }
        })
    }
}

impl<H: Host> ResponsesBridge<H> {
    async fn start(&mut self, text: String) -> Result<(), TransportError> {
        if self.active.is_some() {
            return Err(TransportError::Interrupted(
                "Responses websocket already has an active response".into(),
            ));
        }
        let mut value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|_| TransportError::Interrupted("invalid response.create frame".into()))?;
        let object = value.as_object_mut().ok_or_else(|| {
            TransportError::Interrupted("response.create frame must be an object".into())
        })?;
        if object.get("type").and_then(serde_json::Value::as_str) != Some("response.create") {
            return Err(TransportError::Interrupted(
                "unsupported Responses websocket frame".into(),
            ));
        }
        object.remove("type");
        if object.remove("generate") == Some(serde_json::Value::Bool(false)) {
            self.queued.push_back(WsFrame::Text(warmup_event()));
            return Ok(());
        }
        object.insert("stream".into(), serde_json::Value::Bool(true));
        object.insert("store".into(), serde_json::Value::Bool(false));
        self.sequence = self.sequence.saturating_add(1);
        let request_id = format!("{}-ws-{}", self.request_id, self.sequence);
        let outcome = self
            .core
            .execute_planned(
                self.control.as_ref(),
                RequestCtx {
                    request_id,
                    method: http::Method::POST,
                    path: "/v1/responses".into(),
                    query: None,
                    headers: self.headers.clone(),
                    body: Bytes::from(serde_json::to_vec(&value).expect("JSON value serializes")),
                    upgrade: false,
                    mode: self.mode.clone(),
                },
                self.plan.clone(),
            )
            .await
            .map_err(|error| TransportError::Interrupted(error.to_string()))?;
        match outcome.body {
            ResponseBody::Stream(stream) => self.active = Some(stream),
            ResponseBody::Full(body) => {
                self.queued
                    .push_back(WsFrame::Text(String::from_utf8_lossy(&body).into_owned()));
            }
            ResponseBody::WebSocket(_) => {
                return Err(TransportError::Interrupted(
                    "nested Responses websocket is unsupported".into(),
                ));
            }
        }
        Ok(())
    }
}

fn clean_headers(mut headers: http::HeaderMap) -> http::HeaderMap {
    for name in [
        http::header::CONNECTION,
        http::header::UPGRADE,
        http::header::HOST,
        http::header::CONTENT_LENGTH,
    ] {
        headers.remove(name);
    }
    let websocket = headers
        .keys()
        .filter(|name| name.as_str().starts_with("sec-websocket-"))
        .cloned()
        .collect::<Vec<_>>();
    for name in websocket {
        headers.remove(name);
    }
    headers
}

fn drain_sse(pending: &mut String, output: &mut VecDeque<WsFrame>) -> bool {
    *pending = pending.replace("\r\n", "\n");
    let mut terminal = false;
    while let Some(end) = pending.find("\n\n") {
        let block = pending[..end].to_owned();
        pending.drain(..end + 2);
        let data = block
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        terminal |= serde_json::from_str::<serde_json::Value>(&data)
            .ok()
            .and_then(|value| value.get("type")?.as_str().map(str::to_owned))
            .is_some_and(|kind| matches!(kind.as_str(), "response.completed" | "response.failed"));
        output.push_back(WsFrame::Text(data));
    }
    terminal
}

fn warmup_event() -> String {
    serde_json::json!({
        "type":"response.completed",
        "sequence_number":0,
        "response":{
            "id":"gproxy-warmup",
            "object":"response",
            "created_at":0,
            "status":"completed",
            "output":[],
            "usage":{
                "input_tokens":0,
                "output_tokens":0,
                "total_tokens":0,
                "output_tokens_details":{"reasoning_tokens":0}
            }
        }
    })
    .to_string()
}
