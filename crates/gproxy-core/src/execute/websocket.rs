//! WebSocket operations: the handshake goes through the same attempt loop
//! as HTTP; an established socket is wrapped so both directions are captured,
//! metered and rewritten per message, and closing or dropping it settles the
//! request. Sessions are bounded by cancellation and the frame cap, not by the
//! HTTP idle/total timeouts: a realtime session legitimately waits on a user.

use super::{Exchange, Funnel, ObservedClient, prepare};
use crate::session::{AssignmentHandle, AssignmentOutcome};
use crate::{
    AttemptContext, AttemptOutcome, BlockSource, CaptureDirection, CaptureEvent, Core, CoreError,
    CoreResult, CredentialBlock, Execution, RequestContext, TraceEvent, UsageState,
    WebSocketExecution,
    api::lifecycle::now_ms,
    availability::{DEFAULT_RATE_LIMIT_MS, retry_after_ms},
    convert::{self, Route},
    rewrite::{
        Phase, RewriteContext, SelectedRules, apply_headers, apply_query, apply_unit, select_rules,
    },
};
use futures_util::{Sink, StreamExt};
use gproxy_channel::{
    ChannelBinding,
    channel::{UsageFrame, UsageStreamContext, UsageStreamEnd, UsageTransport},
};
use gproxy_protocol::{
    Dialect, WireRequest,
    capability::UpstreamConnection,
    connection::{TransportError, WebSocket, WsFrame, WsReceiver, WsSender},
    transform::TransformError,
};
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::upstream::operation_endpoint::EndpointTransport;
use http::StatusCode;
use std::{
    collections::HashSet,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

pub(crate) async fn run_websocket<C: BatchConnectionTrait + Send + Sync + 'static>(
    core: &Core<C>,
    request: Arc<RequestContext>,
    wire: WireRequest<()>,
) -> CoreResult<WebSocketExecution> {
    let (funnel, completion) = Funnel::new(request.clone(), core.observer().clone());
    let _request_guard = funnel.guard();
    let result = run_websocket_inner(core, request, wire, funnel.clone(), completion).await;
    if let Err(error) = &result {
        let state = if matches!(error, CoreError::Cancelled) {
            UsageState::Cancelled
        } else {
            UsageState::Failed
        };
        funnel.finish(state).await;
    }
    result
}

async fn run_websocket_inner<C: BatchConnectionTrait + Send + Sync + 'static>(
    core: &Core<C>,
    request: Arc<RequestContext>,
    mut wire: WireRequest<()>,
    funnel: Arc<Funnel>,
    completion: crate::UsageCompletion,
) -> CoreResult<WebSocketExecution> {
    funnel.set_meter(core.usage_meter());
    super::attempt::reject_when_over_budget(core, &request, &funnel).await?;
    let snapshot = request.snapshot.clone();
    let limits = snapshot.limits;
    let provider = request.target.provider.clone();
    let operation = request.operation;
    let upstream_model = request.target.upstream_model.clone();

    match convert::route(&provider, operation) {
        Ok(Route::Passthrough) => {}
        Ok(Route::Convert { upstream } | Route::Synthesize { upstream }) => {
            if operation.dialect != Dialect::OpenAiResponsesWebSocket {
                funnel.finish(UsageState::Failed).await;
                return Err(CoreError::Transform(TransformError::unsupported(
                    "route",
                    format!("{:?} over WebSocket is passthrough only", operation.dialect),
                )));
            }
            return convert::responses_ws::serve(
                core, request, &wire, funnel, completion, upstream,
            )
            .await;
        }
        Err(error) => {
            funnel.finish(UsageState::Failed).await;
            return Err(error.into());
        }
    }
    let inbound_headers = wire.headers.clone();
    let rewrite_context = RewriteContext {
        operation,
        upstream_model: upstream_model.as_deref(),
        requested_model: None,
        request_headers: &inbound_headers,
    };
    let request_rules = select_rules(&snapshot, &provider, Phase::Request, &rewrite_context);
    let response_rules = select_rules(&snapshot, &provider, Phase::Response, &rewrite_context);
    if !request_rules.headers.is_empty() {
        apply_headers(&request_rules.headers, &mut wire.headers)?;
    }
    if !request_rules.query.is_empty()
        && let Some(query) = apply_query(&request_rules.query, wire.query.as_deref())?
    {
        wire.query = Some(query);
    }

    // Request rules may change the query; validate the final continuation too,
    // before selecting any credential or emitting an upstream handshake.
    let request = match core.bind_realtime_continuation(request, &wire).await {
        Ok(request) => request,
        Err(error) => {
            funnel.finish(UsageState::Failed).await;
            return Err(error);
        }
    };

    if operation.operation == gproxy_protocol::Operation::ConnectRealtime {
        funnel.set_realtime_dedup(crate::realtime::SettlementDedup::new(
            core.cache().clone(),
            crate::realtime::call_id(&wire)?,
        ));
    }
    let attempts = request.max_attempts.get();
    let mut excluded: HashSet<String> = HashSet::new();
    let mut refreshed: HashSet<String> = HashSet::new();
    let mut held: Option<(UpstreamConnection, Arc<Exchange>)> = None;
    let mut ordinal = 0u32;
    let mut carried: Option<AssignmentHandle> = None;
    while ordinal < attempts {
        ordinal += 1;
        let now = now_ms();
        if request.cancellation.is_cancelled() {
            drop(held.take());
            funnel.finish(UsageState::Cancelled).await;
            return Err(CoreError::Cancelled);
        }
        let selection = match core.select_credential(&request, &excluded, now).await {
            Ok(selection) => selection,
            Err(CoreError::NoUsableCredential) if held.is_some() => {
                let (connection, exchange) = held.take().expect("held");
                exchange.make_terminal();
                let settled = funnel.arm();
                return Ok(Execution::new(connection, completion, settled));
            }
            Err(error) => {
                drop(held.take());
                funnel.finish(UsageState::Failed).await;
                return Err(error);
            }
        };
        if let Some((UpstreamConnection::Rejected(response), exchange)) = held.take() {
            super::stream::drain(response.body, exchange.limits.operation_total).await;
        }
        let credential = selection.credential;
        let mut assignment = match (selection.assignment, carried.take()) {
            (Some(handle), Some(previous))
                if previous.reference.assignment_id == handle.reference.assignment_id =>
            {
                Some(previous)
            }
            (handle, Some(previous)) => {
                core.settle_assignment(&previous, failed(false, "superseded"), now)
                    .await?;
                handle
            }
            (handle, None) => handle,
        };
        let version = credential.state.load();
        let attempt = Arc::new(AttemptContext {
            attempt_id: format!("{}-{ordinal}", request.request_id),
            request: request.clone(),
            ordinal,
            credential: credential.clone(),
            credential_version: version.clone(),
            agent_assignment: assignment.as_ref().map(|h| h.reference.clone()),
        });
        funnel.trace(TraceEvent::AttemptStarted(&attempt));
        let exchange = Exchange::new(
            funnel.clone(),
            attempt.clone(),
            operation,
            provider.channel.clone(),
            response_rules.body.clone(),
            limits.capability(None),
            now,
        );
        let observed = ObservedClient::new(credential.websocket_client.clone(), exchange.clone());
        let channel_state: Arc<dyn gproxy_channel::channel::ChannelState> =
            Arc::new(crate::ChannelStateStore::new(
                crate::ProtocolState::new(core, limits.capability(None)),
                &provider.entity.id,
                &credential.id,
            ));
        let binding = ChannelBinding::new(
            provider.channel.as_ref(),
            prepare::provider_view(&provider),
            prepare::credential_view(&credential, &version),
            Arc::new(observed),
        )
        .state(channel_state)
        .instance(core.instance_id().clone())
        .endpoint(provider.operation_url(operation, EndpointTransport::WebSocket));
        let handshake = WireRequest {
            method: wire.method.clone(),
            path: wire.path.clone(),
            query: wire.query.clone(),
            headers: wire.headers.clone(),
            body: (),
        };
        let cancellation = request.cancellation.clone();
        let connected = tokio::select! {
            biased;
            () = cancellation.cancelled() => None,
            result = binding.connect(operation, handshake) => Some(result),
        };
        let finished_at = now_ms();
        let connection = match connected {
            None => {
                let outcome = AttemptOutcome::Cancelled;
                funnel.trace(TraceEvent::AttemptFinished {
                    attempt: &attempt,
                    outcome: &outcome,
                    finished_at_ms: finished_at,
                });
                exchange
                    .finish(UsageStreamEnd::Interrupted, None, None, None, finished_at)
                    .await;
                if let Some(handle) = assignment.take() {
                    let _ = core
                        .settle_assignment(&handle, failed(true, "cancelled"), finished_at)
                        .await;
                }
                funnel.finish(UsageState::Cancelled).await;
                return Err(CoreError::Cancelled);
            }
            Some(Err(error)) => {
                let outcome = AttemptOutcome::Failed(error);
                funnel.trace(TraceEvent::AttemptFinished {
                    attempt: &attempt,
                    outcome: &outcome,
                    finished_at_ms: finished_at,
                });
                exchange
                    .finish(UsageStreamEnd::Interrupted, None, None, None, finished_at)
                    .await;
                if let Some(handle) = assignment.take() {
                    core.settle_assignment(&handle, failed(true, "transport"), finished_at)
                        .await?;
                }
                if core
                    .record_failure(
                        &credential.provider_id,
                        &credential.id,
                        upstream_model.as_deref(),
                        operation.operation,
                        finished_at,
                    )
                    .await?
                    .is_some()
                {
                    excluded.insert(credential.id.clone());
                }
                let AttemptOutcome::Failed(error) = outcome else {
                    unreachable!()
                };
                if ordinal >= attempts {
                    funnel.finish(UsageState::Failed).await;
                    return Err(CoreError::Channel(error));
                }
                continue;
            }
            Some(Ok(connection)) => connection,
        };
        match connection {
            UpstreamConnection::Connected { handshake, socket } => {
                let outcome = AttemptOutcome::Succeeded {
                    status: handshake.status,
                };
                funnel.trace(TraceEvent::AttemptFinished {
                    attempt: &attempt,
                    outcome: &outcome,
                    finished_at_ms: finished_at,
                });
                core.record_success(
                    &credential.provider_id,
                    &credential.id,
                    upstream_model.as_deref(),
                    operation.operation,
                    finished_at,
                )
                .await?;
                core.pin_affinity(&request, &credential.id, finished_at)
                    .await?;
                if let Some(handle) = assignment.take() {
                    core.settle_assignment(&handle, AssignmentOutcome::Activated, finished_at)
                        .await?;
                }
                exchange.start_ws_usage_observer(&handshake);
                exchange.make_terminal();
                let socket =
                    observe_socket(exchange, socket, &request_rules, limits.max_ws_frame_bytes);
                let settled = funnel.arm();
                return Ok(Execution::new(
                    UpstreamConnection::Connected { handshake, socket },
                    completion,
                    settled,
                ));
            }
            UpstreamConnection::Rejected(response) => {
                let status = response.status;
                let refreshable = provider.channel.credential_refresh().is_some();
                let retry_same = matches!(status.as_u16(), 401 | 403)
                    && refreshable
                    && !refreshed.contains(&credential.id);
                let unusable = matches!(status.as_u16(), 429 | 401 | 403 | 500..=599);
                if !unusable {
                    let outcome = AttemptOutcome::Succeeded { status };
                    funnel.trace(TraceEvent::AttemptFinished {
                        attempt: &attempt,
                        outcome: &outcome,
                        finished_at_ms: finished_at,
                    });
                    if let Some(handle) = assignment.take() {
                        core.settle_assignment(&handle, AssignmentOutcome::Activated, finished_at)
                            .await?;
                    }
                    exchange.make_terminal();
                    let settled = funnel.arm();
                    return Ok(Execution::new(
                        UpstreamConnection::Rejected(response),
                        completion,
                        settled,
                    ));
                }
                let retry_after = retry_after_ms(&response.headers);
                let outcome = AttemptOutcome::Rejected {
                    status,
                    retry_after: retry_after.map(|ms| std::time::Duration::from_millis(ms as u64)),
                };
                funnel.trace(TraceEvent::AttemptFinished {
                    attempt: &attempt,
                    outcome: &outcome,
                    finished_at_ms: finished_at,
                });
                if retry_same {
                    refreshed.insert(credential.id.clone());
                    if core
                        .refresh_credential(
                            &credential.provider_id,
                            &credential.id,
                            crate::RefreshMode::Force,
                        )
                        .await
                        .is_err()
                    {
                        if let Some(handle) = assignment.take() {
                            core.settle_assignment(&handle, failed(false, "refresh"), finished_at)
                                .await?;
                        }
                        excluded.insert(credential.id.clone());
                        core.record_failure(
                            &credential.provider_id,
                            &credential.id,
                            upstream_model.as_deref(),
                            operation.operation,
                            finished_at,
                        )
                        .await?;
                    }
                } else if status == StatusCode::TOO_MANY_REQUESTS {
                    core.record_block(
                        &credential.provider_id,
                        &credential.id,
                        CredentialBlock {
                            scope: gproxy_channel::channel::QuotaScope::All,
                            operation: None,
                            until_ms: finished_at + retry_after.unwrap_or(DEFAULT_RATE_LIMIT_MS),
                            source: BlockSource::RateLimited,
                            observed_at_ms: finished_at,
                        },
                        finished_at,
                    )
                    .await?;
                    excluded.insert(credential.id.clone());
                } else if core
                    .record_failure(
                        &credential.provider_id,
                        &credential.id,
                        upstream_model.as_deref(),
                        operation.operation,
                        finished_at,
                    )
                    .await?
                    .is_some()
                {
                    excluded.insert(credential.id.clone());
                }
                if retry_same && !excluded.contains(&credential.id) {
                    carried = assignment.take();
                } else if let Some(handle) = assignment.take() {
                    core.settle_assignment(
                        &handle,
                        failed(status.is_server_error(), status.as_str()),
                        finished_at,
                    )
                    .await?;
                }
                held = Some((UpstreamConnection::Rejected(response), exchange));
            }
        }
    }
    if let Some((connection, exchange)) = held.take() {
        exchange.make_terminal();
        let settled = funnel.arm();
        return Ok(Execution::new(connection, completion, settled));
    }
    funnel.finish(UsageState::Failed).await;
    Err(CoreError::NoUsableCredential)
}

fn transport_error(message: &str) -> TransportError {
    Box::new(std::io::Error::other(message.to_owned()))
}

fn frame_len(frame: &WsFrame) -> usize {
    match frame {
        WsFrame::Text(text) => text.len(),
        WsFrame::Binary(bytes) | WsFrame::Ping(bytes) | WsFrame::Pong(bytes) => bytes.len(),
        WsFrame::Close(_) => 0,
    }
}

fn rewrite_text(
    rules: &[Arc<crate::RewriteRuleData>],
    frame: WsFrame,
) -> Result<WsFrame, TransportError> {
    match frame {
        WsFrame::Text(text) if !rules.is_empty() => {
            let event = serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .and_then(|v| v.get("type")?.as_str().map(str::to_owned));
            match apply_unit(rules, event.as_deref(), &text) {
                Ok(Some(rewritten)) => Ok(WsFrame::Text(rewritten)),
                Ok(None) => Ok(WsFrame::Text(text)),
                Err(error) => Err(transport_error(&error.to_string())),
            }
        }
        other => Ok(other),
    }
}

struct Guard(Arc<Exchange>, bool);
impl Drop for Guard {
    fn drop(&mut self) {
        if !self.1 {
            self.0.clone().finish_detached(
                UsageStreamEnd::Interrupted,
                http::StatusCode::SWITCHING_PROTOCOLS,
                http::HeaderMap::new(),
                None,
                now_ms(),
            );
        }
    }
}

fn observe_socket(
    exchange: Arc<Exchange>,
    socket: WebSocket,
    request_rules: &SelectedRules,
    max_frame_bytes: u64,
) -> WebSocket {
    let response_rules = exchange.response_rules.clone();
    let cancellation = exchange.context.attempt.request.cancellation.clone();
    struct Incoming {
        inner: Option<WsReceiver>,
        guard: Guard,
        rules: Vec<Arc<crate::RewriteRuleData>>,
        max: u64,
        cancellation: tokio_util::sync::CancellationToken,
    }
    let incoming = futures_util::stream::unfold(
        Incoming {
            inner: Some(socket.incoming),
            guard: Guard(exchange.clone(), false),
            rules: response_rules,
            max: max_frame_bytes,
            cancellation,
        },
        |mut state| async move {
            let inner = state.inner.as_mut()?;
            let next = tokio::select! {
                biased;
                () = state.cancellation.cancelled() => Some(Err(transport_error("request cancelled"))),
                next = inner.next() => next,
            };
            match next {
                None => {
                    state.inner = None;
                    state.guard.1 = true;
                    state
                        .guard
                        .0
                        .finish(
                            UsageStreamEnd::Complete,
                            Some(StatusCode::SWITCHING_PROTOCOLS),
                            None,
                            None,
                            now_ms(),
                        )
                        .await;
                    None
                }
                Some(Err(error)) => {
                    state.inner = None;
                    state.guard.1 = true;
                    state
                        .guard
                        .0
                        .finish(
                            UsageStreamEnd::Interrupted,
                            Some(StatusCode::SWITCHING_PROTOCOLS),
                            None,
                            None,
                            now_ms(),
                        )
                        .await;
                    Some((Err(error), state))
                }
                Some(Ok(frame)) => {
                    if frame_len(&frame) as u64 > state.max {
                        state.inner = None;
                        state.guard.1 = true;
                        state
                            .guard
                            .0
                            .finish(
                                UsageStreamEnd::Interrupted,
                                Some(StatusCode::SWITCHING_PROTOCOLS),
                                None,
                                None,
                                now_ms(),
                            )
                            .await;
                        return Some((
                            Err(transport_error("frame exceeds the frame limit")),
                            state,
                        ));
                    }
                    if state.guard.0.wants_full_capture() {
                        state.guard.0.record(CaptureEvent::Frame {
                            direction: CaptureDirection::Response,
                            frame: &frame,
                        });
                    }
                    state.guard.0.observe_ws_frame(&frame);
                    let frame = rewrite_text(&state.rules, frame);
                    Some((frame, state))
                }
            }
        },
    );
    let outgoing = ObservedSink {
        inner: socket.outgoing,
        exchange,
        rules: request_rules.body.clone(),
        max: max_frame_bytes,
    };
    WebSocket {
        incoming: Box::pin(incoming),
        outgoing: Box::pin(outgoing),
    }
}

struct ObservedSink {
    inner: WsSender,
    exchange: Arc<Exchange>,
    rules: Vec<Arc<crate::RewriteRuleData>>,
    max: u64,
}

impl Sink<WsFrame> for ObservedSink {
    type Error = TransportError;

    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.as_mut().poll_ready(cx)
    }

    fn start_send(mut self: Pin<&mut Self>, frame: WsFrame) -> Result<(), Self::Error> {
        if frame_len(&frame) as u64 > self.max {
            return Err(transport_error("frame exceeds the frame limit"));
        }
        let frame = rewrite_text(&self.rules, frame)?;
        if self.exchange.wants_full_capture() {
            self.exchange.record(CaptureEvent::Frame {
                direction: CaptureDirection::Request,
                frame: &frame,
            });
        }
        self.inner.as_mut().start_send(frame)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.as_mut().poll_flush(cx)
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.as_mut().poll_close(cx)
    }
}

impl Exchange {
    pub(crate) fn observe_ws_frame(&self, frame: &WsFrame) {
        if let Some(observer) = self.usage_observer.lock().unwrap().as_mut() {
            let _ = observer.observe(UsageFrame::WebSocket(frame));
        }
    }

    pub(crate) fn start_ws_usage_observer(&self, handshake: &gproxy_protocol::WireResponse<()>) {
        if !self.funnel.policy().usage {
            return;
        }
        let Some(stream) = self.channel.usage_stream() else {
            return;
        };
        let observer = stream.start(UsageStreamContext {
            operation: self.context.operation,
            request_body: None,
            status: handshake.status,
            headers: &handshake.headers,
            transport: UsageTransport::WebSocket,
        });
        if let Ok(observer) = observer {
            *self.usage_observer.lock().unwrap() = Some(observer);
        }
    }
}

fn failed(uncertain: bool, error: &str) -> AssignmentOutcome {
    AssignmentOutcome::Failed {
        uncertain,
        error: error.to_owned(),
    }
}
