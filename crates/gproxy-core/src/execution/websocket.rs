use std::collections::VecDeque;

use crate::api::Core;
use crate::boundary::{ExecOutcome, RequestCtx, ResponseBody, RoutingMode};
use crate::control::{ControlPlane, FailoverBudget, Plan, Target};
use crate::error::CoreError;
use crate::host::Host;
use crate::usage::Ended;
use bytes::Bytes;
use futures_util::StreamExt as _;
use gproxy_channel_api::{BoxFuture, TransportError, WsDuplex, WsFrame};
use gproxy_protocol::openai::{
    KnownResponseStreamEvent, ResponseStreamEvent, ResponseWebSocketRequest,
};
use gproxy_protocol::{ContentGenerationKind, OperationKind};

use super::request::Classified;

mod native;
mod state;
mod wire;
use state::ActiveResponse;
use wire::*;

pub(super) fn run<H: Host>(
    core: &Core<H>,
    control: &dyn ControlPlane,
    ctx: RequestCtx,
    plan: Plan,
    classified: Classified,
    identity: gproxy_channel_api::CallerIdentity,
) -> Result<ExecOutcome, CoreError> {
    if classified.key.kind()
        != OperationKind::ContentGeneration(ContentGenerationKind::OpenAiResponsesWebSocket)
    {
        return Err(CoreError::Unsupported);
    }
    Ok(crate::funnel::bridged_websocket(Box::new(
        ResponsesBridge {
            core: core.clone(),
            control: control.detached(),
            fallback_plan: plan,
            identity,
            headers: clean_headers(ctx.headers),
            mode: ctx.mode,
            request_id: ctx.request_id,
            sequence: 0,
            pinned: None,
            native: None,
            active: None,
            http_active: None,
            http_pending: String::new(),
            queued: VecDeque::new(),
            closed: false,
        },
    )))
}

struct ResponsesBridge<H: Host> {
    core: Core<H>,
    control: Box<dyn ControlPlane>,
    fallback_plan: Plan,
    identity: gproxy_channel_api::CallerIdentity,
    headers: http::HeaderMap,
    mode: RoutingMode,
    request_id: String,
    sequence: u64,
    pinned: Option<(Target, u64)>,
    native: Option<Box<dyn WsDuplex>>,
    active: Option<ActiveResponse>,
    http_active: Option<crate::boundary::ByteStream>,
    http_pending: String,
    queued: VecDeque<WsFrame>,
    closed: bool,
}

impl<H: Host> WsDuplex for ResponsesBridge<H> {
    fn send<'a>(&'a mut self, frame: WsFrame) -> BoxFuture<'a, Result<(), TransportError>> {
        Box::pin(async move {
            match frame {
                WsFrame::Close(code) => self.close(code).await,
                WsFrame::Binary(_) => Err(TransportError::Interrupted(
                    "Responses websocket accepts text frames only".into(),
                )),
                WsFrame::Text(text) => self.client_text(text).await,
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
                if self.http_active.is_some() {
                    self.recv_http().await?;
                    continue;
                }
                if self.native.is_some() {
                    return self.recv_native().await;
                }
                std::future::pending::<()>().await;
            }
        })
    }
}

impl<H: Host> ResponsesBridge<H> {
    async fn client_text(&mut self, text: String) -> Result<(), TransportError> {
        let request: ResponseWebSocketRequest = serde_json::from_str(&text).map_err(|error| {
            TransportError::Interrupted(format!("invalid websocket frame: {error}"))
        })?;
        match request {
            ResponseWebSocketRequest::ResponseCreate(request) => {
                if request.generate == Some(false) {
                    self.queued.push_back(WsFrame::Text(warmup_event()));
                    return Ok(());
                }
                if self.active.is_some() || self.http_active.is_some() {
                    self.queue_error(409, "a response is already active");
                    return Ok(());
                }
                self.start_response(text, request.response.model.as_ref())
                    .await
            }
            ResponseWebSocketRequest::ResponseInject(request) => {
                let Some(active) = self.active.as_mut() else {
                    self.queue_inject_failed(
                        &request.response_id,
                        request.input,
                        "response_not_found",
                    );
                    return Ok(());
                };
                if active.response_id.as_deref() != Some(&request.response_id) {
                    self.queue_inject_failed(
                        &request.response_id,
                        request.input,
                        "response_not_found",
                    );
                    return Ok(());
                }
                active.pending_injections = active.pending_injections.saturating_add(1);
                self.native
                    .as_mut()
                    .expect("active native response has a socket")
                    .send(WsFrame::Text(text))
                    .await
            }
            ResponseWebSocketRequest::ResponseSteer(request) => {
                if self.http_active.is_some() {
                    self.queue_steer_failed(request, "steering_not_supported");
                    return Ok(());
                }
                let Some(active) = self.active.as_ref() else {
                    self.queue_steer_failed(request, "response_not_found");
                    return Ok(());
                };
                if active.response_id.as_deref() != Some(&request.previous_response_id) {
                    self.queue_steer_failed(request, "response_not_found");
                    return Ok(());
                }
                self.active
                    .as_mut()
                    .expect("checked active response")
                    .pending_steers = active.pending_steers.saturating_add(1);
                self.native
                    .as_mut()
                    .expect("active native response has a socket")
                    .send(WsFrame::Text(text))
                    .await
            }
            ResponseWebSocketRequest::Unknown(_) => {
                self.queue_error(422, "unsupported Responses websocket frame");
                Ok(())
            }
        }
    }

    async fn start_response(
        &mut self,
        text: String,
        model: Option<&gproxy_protocol::openai::OpenAiModelId>,
    ) -> Result<(), TransportError> {
        self.sequence = self.sequence.saturating_add(1);
        let request_id = format!("{}-ws-{}", self.request_id, self.sequence);
        let model = model.and_then(wire_string);
        let mut request = RequestCtx {
            request_id,
            client_ip: None,
            method: http::Method::GET,
            path: "/v1/responses".into(),
            query: None,
            headers: self.headers.clone(),
            body: Bytes::from(text),
            upgrade: false,
            mode: self.mode.clone(),
        };
        let mut classified = Classified::responses_websocket(model);
        crate::execution::preprocess::apply(self.control.as_ref(), &mut request, &mut classified)
            .map_err(transport)?;
        let resolved = self
            .control
            .resolve_preprocessed(
                classified.model.as_deref(),
                &request.mode,
                Some(classified.routing_affinity(self.identity.user_key_id)),
            )
            .unwrap_or_else(|_| self.fallback_plan.clone());
        let plan = if let Some((target, _)) = &self.pinned {
            if !resolved.targets.iter().any(|candidate| {
                candidate.provider.id == target.provider.id
                    && candidate.credential == target.credential
                    && candidate.upstream_model == target.upstream_model
            }) {
                self.queue_error(409, "websocket connection is pinned to another model");
                return Ok(());
            }
            Plan {
                targets: vec![target.clone()],
                budget: FailoverBudget { max_attempts: 1 },
            }
        } else {
            resolved
        };
        if self.pinned.is_none() {
            if self.connect_native(&request, &plan, &classified).await? {
                return Ok(());
            }
            return self.start_http(request, plan).await;
        }
        self.prepare_pinned(&request, &plan, &classified).await
    }

    async fn start_http(
        &mut self,
        mut request: RequestCtx,
        plan: Plan,
    ) -> Result<(), TransportError> {
        let mut value: serde_json::Value =
            serde_json::from_slice(&request.body).map_err(transport)?;
        let object = value.as_object_mut().expect("typed frame is an object");
        object.remove("type");
        object.remove("generate");
        object.remove("client_metadata");
        object.insert("stream".into(), serde_json::Value::Bool(true));
        object.insert("store".into(), serde_json::Value::Bool(false));
        request.method = http::Method::POST;
        request.body = Bytes::from(serde_json::to_vec(&value).expect("JSON serializes"));
        let outcome = self
            .core
            .execute_planned(self.control.as_ref(), request, plan)
            .await
            .map_err(transport)?;
        match outcome.body {
            ResponseBody::Stream(stream) => self.http_active = Some(stream),
            ResponseBody::Full(body) => self
                .queued
                .push_back(WsFrame::Text(String::from_utf8_lossy(&body).into_owned())),
            ResponseBody::WebSocket(_) => {
                return Err(TransportError::Interrupted(
                    "nested websocket is unsupported".into(),
                ));
            }
        }
        Ok(())
    }

    async fn recv_http(&mut self) -> Result<(), TransportError> {
        let active = self.http_active.as_mut().expect("checked HTTP stream");
        match active.next().await {
            Some(Ok(chunk)) => {
                let text = std::str::from_utf8(&chunk).map_err(|_| {
                    TransportError::Interrupted("upstream SSE was not UTF-8".into())
                })?;
                self.http_pending.push_str(text);
                if drain_sse(&mut self.http_pending, &mut self.queued) {
                    self.http_active = None;
                }
            }
            Some(Err(error)) => {
                self.http_active = None;
                return Err(error);
            }
            None => self.http_active = None,
        }
        Ok(())
    }

    async fn recv_native(&mut self) -> Result<Option<WsFrame>, TransportError> {
        let frame = self
            .native
            .as_mut()
            .expect("checked native socket")
            .recv()
            .await?;
        let Some(frame) = frame else {
            self.finish_active(Ended::Interrupted).await;
            self.closed = true;
            return Ok(None);
        };
        let WsFrame::Text(text) = frame else {
            if matches!(frame, WsFrame::Close(_)) {
                self.finish_active(Ended::Interrupted).await;
                self.closed = true;
            }
            return Ok(Some(frame));
        };
        self.observe_native(&text).await;
        Ok(Some(WsFrame::Text(text)))
    }

    async fn observe_native(&mut self, text: &str) {
        let Ok(ResponseStreamEvent::Known(event)) = serde_json::from_str(text) else {
            return;
        };
        let Some(active) = self.active.as_mut() else {
            return;
        };
        active.output_chars = active
            .output_chars
            .saturating_add(crate::usage::utf8_chars(text.as_bytes()));
        match *event {
            KnownResponseStreamEvent::ResponseCreated(event) => {
                if active.response_id.is_some() && active.pending_steers > 0 {
                    active.pending_steers = 0;
                    active.terminal = None;
                }
                active.response_id = Some(event.response.id.clone());
            }
            KnownResponseStreamEvent::ResponseInjectCreated(_)
            | KnownResponseStreamEvent::ResponseInjectFailed(_) => {
                active.pending_injections = active.pending_injections.saturating_sub(1);
            }
            KnownResponseStreamEvent::ResponseSteerAccepted(_) => {}
            KnownResponseStreamEvent::ResponseSteerPending(_)
            | KnownResponseStreamEvent::ResponseSteerFailed(_) => {
                active.pending_steers = active.pending_steers.saturating_sub(1);
            }
            KnownResponseStreamEvent::ResponseCompleted(event)
            | KnownResponseStreamEvent::ResponseIncomplete(event) => {
                active.terminal = Some(Ended::Complete);
                active.responses.push(Bytes::from(
                    serde_json::to_vec(&event.response).expect("response serializes"),
                ));
            }
            KnownResponseStreamEvent::ResponseFailed(event) => {
                active.terminal = Some(Ended::Interrupted);
                active.responses.push(Bytes::from(
                    serde_json::to_vec(&event.response).expect("response serializes"),
                ));
            }
            KnownResponseStreamEvent::Error(_) => {
                active.terminal = Some(Ended::Interrupted);
            }
            _ => {}
        }
        if self.active.as_ref().is_some_and(|active| {
            active.terminal.is_some()
                && active.pending_injections == 0
                && active.pending_steers == 0
        }) {
            self.finish_active(Ended::Complete).await;
        }
    }

    async fn finish_active(&mut self, fallback: Ended) {
        let Some(mut active) = self.active.take() else {
            return;
        };
        let ended = active.terminal.take().unwrap_or(fallback);
        let channel = self
            .core
            .channels
            .get(&active.facts.target.provider.channel)
            .expect("pinned channel remains registered");
        let usage = combined_response_usage(channel, &active.facts, &active.responses);
        let tier = active.responses.last().and_then(|response| {
            crate::control::response_service_tier(&http::HeaderMap::new(), response)
        });
        crate::funnel::complete_stream(
            self.core.host.clone(),
            active.facts,
            http::StatusCode::SWITCHING_PROTOCOLS,
            usage,
            tier,
            Some(active.output_chars),
            ended,
        )
        .await;
    }

    async fn close(&mut self, code: Option<u16>) -> Result<(), TransportError> {
        self.finish_active(Ended::Interrupted).await;
        if let Some(socket) = self.native.as_mut() {
            let _ = socket.send(WsFrame::Close(code)).await;
        }
        self.closed = true;
        self.http_active = None;
        self.queued.push_back(WsFrame::Close(code));
        Ok(())
    }
}
