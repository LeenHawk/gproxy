//! The lifetime of a Responses connection is independent of its admitted turns.
//! A host opens a chain once, then admits each create/automatic successor before
//! forwarding it. Only actual generation exchanges contribute usage.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

use gproxy_channel::channel::UsageStreamEnd;
use gproxy_protocol::{Dialect, WireRequest, connection::WsFrame};
use gproxy_seaorm::BatchConnectionTrait;
use serde_json::Value;

use crate::{
    AttemptContext, CaptureEvent, Core, CoreError, CoreResult, CredentialData, Execution,
    RequestContext, TraceEvent, UsageState, WebSocketExecution,
    api::lifecycle::now_ms,
    execute::{Exchange, Funnel},
    rewrite::{Phase, RewriteContext, SelectedRules, select_rules},
};

/// An opaque, connection-owned chain. Clones share the current and reserved
/// turn, but never move the chain to another principal, provider or account.
#[derive(Clone, Default)]
pub struct ResponsesSession(Arc<Mutex<State>>);

#[derive(Default)]
struct State {
    binding: Option<Binding>,
    current: Option<Arc<Turn>>,
    pending: Option<Arc<Turn>>,
    pending_acknowledged: bool,
    closed: bool,
    expires_at_ms: Option<i64>,
}

#[derive(Clone)]
struct Binding {
    request: Arc<RequestContext>,
    credential: Arc<CredentialData>,
    dialect: Dialect,
    connection_id: Option<String>,
    agent_assignment: Option<crate::AgentAssignmentRef>,
}

/// No secrets: enough for the upper layer to restrict a continuation's plan.
#[derive(Clone, Debug)]
pub struct ResponsesBinding {
    pub provider_id: String,
    pub credential_id: String,
    pub model: String,
    pub dialect: Dialect,
    pub expires_at_ms: i64,
}

pub(crate) struct Turn {
    pub attempt: Arc<AttemptContext>,
    pub funnel: Arc<Funnel>,
    pub exchange: Option<Arc<Exchange>>,
    pub request_rules: SelectedRules,
    pub response_rules: SelectedRules,
    started: AtomicBool,
}

impl std::fmt::Debug for ResponsesSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResponsesSession")
            .field("binding", &self.binding())
            .finish_non_exhaustive()
    }
}

impl ResponsesSession {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_expiry(expires_at_ms: i64) -> Self {
        Self(Arc::new(Mutex::new(State {
            expires_at_ms: Some(expires_at_ms),
            ..State::default()
        })))
    }

    pub fn binding(&self) -> Option<ResponsesBinding> {
        let state = self.0.lock().unwrap();
        state.binding.as_ref().map(|b| ResponsesBinding {
            provider_id: b.credential.provider_id.clone(),
            credential_id: b.credential.id.clone(),
            model: b.request.target.upstream_model.clone().unwrap_or_default(),
            dialect: b.dialect,
            expires_at_ms: state.expires_at_ms.unwrap(),
        })
    }

    pub(crate) fn bind(
        &self,
        request: Arc<RequestContext>,
        credential: Arc<CredentialData>,
        dialect: Dialect,
        connection_id: Option<String>,
        agent_assignment: Option<crate::AgentAssignmentRef>,
    ) {
        let mut state = self.0.lock().unwrap();
        let maximum = request.started_at_ms + 24 * 60 * 60 * 1000;
        state.expires_at_ms = Some(state.expires_at_ms.unwrap_or(maximum).min(maximum));
        state.binding = Some(Binding {
            request,
            credential,
            dialect,
            connection_id,
            agent_assignment,
        });
    }

    pub(crate) fn current(&self) -> Option<Arc<Turn>> {
        self.0.lock().unwrap().current.clone()
    }

    pub(crate) fn response_started(&self) {
        if let Some(turn) = self.current() {
            turn.started.store(true, Ordering::Relaxed);
        }
    }

    /// A steering message can arrive after the predecessor's terminal was
    /// committed and its reservation was promoted. Reject that unstarted
    /// successor as well, without cancelling a response that really started.
    pub(crate) async fn reject_successor(&self) {
        let turn = {
            let mut state = self.0.lock().unwrap();
            state.pending_acknowledged = false;
            state.pending.take().or_else(|| {
                if state
                    .current
                    .as_ref()
                    .is_some_and(|turn| !turn.started.load(Ordering::Relaxed))
                {
                    state.current.take()
                } else {
                    None
                }
            })
        };
        if let Some(turn) = turn {
            if let Some(exchange) = &turn.exchange {
                exchange
                    .finish(UsageStreamEnd::Interrupted, None, now_ms())
                    .await;
            }
            turn.funnel.finish(UsageState::Cancelled).await;
        }
    }

    pub fn current_request_id(&self) -> Option<String> {
        self.current()
            .map(|turn| turn.attempt.request.request_id.clone())
    }

    pub fn pending_request_id(&self) -> Option<String> {
        self.0
            .lock()
            .unwrap()
            .pending
            .as_ref()
            .map(|t| t.attempt.request.request_id.clone())
    }

    pub(crate) fn capture_id(&self) -> Option<String> {
        self.current()
            .and_then(|t| t.exchange.as_ref().map(|e| e.context.capture_id.clone()))
    }

    /// Called at the logical boundary, never at an HTTP segment boundary.
    pub(crate) async fn finish_turn(&self, state: UsageState) {
        let turn = {
            let mut inner = self.0.lock().unwrap();
            let turn = inner.current.take();
            inner.current = inner.pending.take();
            inner.pending_acknowledged = false;
            turn
        };
        if let Some(turn) = turn {
            if let Some(exchange) = &turn.exchange {
                exchange
                    .finish(UsageStreamEnd::Complete, None, now_ms())
                    .await;
            }
            turn.funnel.finish(state).await;
        }
    }

    pub async fn cancel_pending(&self) {
        let turn = {
            let mut state = self.0.lock().unwrap();
            state.pending_acknowledged = false;
            state.pending.take()
        };
        if let Some(turn) = turn {
            if let Some(exchange) = &turn.exchange {
                exchange
                    .finish(UsageStreamEnd::Interrupted, None, now_ms())
                    .await;
            }
            turn.funnel.finish(UsageState::Cancelled).await;
        }
    }

    pub(crate) async fn observe(&self, frame: &WsFrame) {
        let Some(turn) = self.current() else { return };
        if let Some(exchange) = &turn.exchange {
            exchange.observe_ws_frame(frame);
        }
        let WsFrame::Text(text) = frame else { return };
        let Ok(event) = serde_json::from_str::<Value>(text) else {
            return;
        };
        match event["type"].as_str() {
            Some("response.steer.accepted" | "response.steer.pending") => {
                self.0.lock().unwrap().pending_acknowledged = true;
            }
            Some("response.created") => {
                turn.started.store(true, Ordering::Relaxed);
            }
            Some("response.completed" | "response.incomplete") => {
                self.finish_turn(UsageState::Completed).await;
            }
            Some("response.failed") => {
                self.cancel_pending().await;
                self.finish_turn(UsageState::Failed).await;
            }
            Some("error") if !turn.started.load(Ordering::Relaxed) => {
                self.cancel_pending().await;
                self.finish_turn(UsageState::Failed).await
            }
            Some("error") => {
                let rejected = {
                    let state = self.0.lock().unwrap();
                    state.pending.is_some() && !state.pending_acknowledged
                };
                if rejected {
                    self.cancel_pending().await;
                }
            }
            Some("response.steer.failed") => self.cancel_pending().await,
            _ => {}
        }
    }

    pub async fn close(&self) {
        let turns = {
            let mut state = self.0.lock().unwrap();
            state.closed = true;
            [state.current.take(), state.pending.take()]
        };
        for turn in turns.into_iter().flatten() {
            turn.attempt.request.cancellation.cancel();
            if let Some(exchange) = &turn.exchange {
                exchange
                    .finish(UsageStreamEnd::Interrupted, None, now_ms())
                    .await;
            }
            turn.funnel.finish(UsageState::Cancelled).await;
        }
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Core<C> {
    pub async fn connect_responses_session(
        &self,
        context: Arc<RequestContext>,
        request: WireRequest<()>,
        session: ResponsesSession,
    ) -> CoreResult<WebSocketExecution> {
        if context.operation.dialect != Dialect::OpenAiResponsesWebSocket {
            return Err(CoreError::InvalidTarget(
                "expected Responses WebSocket".into(),
            ));
        }
        crate::execute::run_responses_websocket(self, context, request, session).await
    }

    /// Admission and resolution happen above core for every turn. A bound
    /// connection only narrows their result, and never grants access itself.
    pub async fn begin_responses_turn(
        &self,
        context: Arc<RequestContext>,
        session: &ResponsesSession,
        headers: &http::HeaderMap,
        lane: Option<&str>,
        pending: bool,
        warmup: bool,
    ) -> CoreResult<Execution<()>> {
        let binding =
            session.0.lock().unwrap().binding.clone().ok_or_else(|| {
                CoreError::InvalidTarget("Responses connection is not bound".into())
            })?;
        {
            let state = session.0.lock().unwrap();
            if state.closed
                || state.expires_at_ms.is_some_and(|expiry| expiry <= now_ms())
                || (pending && state.current.is_none())
                || if pending {
                    state.pending.is_some()
                } else {
                    state.current.is_some()
                }
            {
                return Err(CoreError::InvalidTarget(
                    "Responses lane is closed or busy".into(),
                ));
            }
        }
        if binding.request.scope != context.scope
            || binding.credential.provider_id != context.target.provider.entity.id
            || binding.request.target.upstream_model != context.target.upstream_model
            || !context
                .target
                .credentials
                .iter()
                .any(|c| c.id == binding.credential.id)
        {
            return Err(CoreError::Forbidden(
                "Responses continuation is outside the admitted target",
            ));
        }
        let (funnel, completion) = Funnel::new(context.clone(), self.observer().clone());
        let guard = funnel.guard();
        funnel.set_meter(self.usage_meter());
        crate::execute::check_budget(self, &context, &funnel).await?;
        let attempt = Arc::new(AttemptContext {
            attempt_id: format!("{}-1", context.request_id),
            request: context.clone(),
            ordinal: 1,
            credential_version: binding.credential.state.load(),
            credential: binding.credential,
            agent_assignment: binding.agent_assignment,
        });
        if !warmup || binding.dialect == Dialect::OpenAiResponsesWebSocket {
            let operation = if binding.dialect == Dialect::OpenAiResponsesWebSocket {
                context.operation.operation
            } else {
                gproxy_protocol::Operation::StreamGenerateContent
            };
            crate::quota::charge_responses_segment(self.store(), self.cache(), &attempt, operation)
                .await?;
        }
        let provider = &context.target.provider;
        let rules = RewriteContext {
            operation: context.operation,
            upstream_model: context.target.upstream_model.as_deref(),
            requested_model: context.target.requested_model.as_deref(),
            request_headers: headers,
        };
        let request_rules = select_rules(&context.snapshot, provider, Phase::Request, &rules);
        let response_rules = select_rules(&context.snapshot, provider, Phase::Response, &rules);
        let exchange = binding.connection_id.as_ref().map(|connection_id| {
            let exchange = Exchange::new(
                funnel.clone(),
                attempt.clone(),
                context.operation,
                provider.channel.clone(),
                response_rules.body.clone(),
                context.snapshot.limits.capability(None),
                now_ms(),
            );
            exchange.record(CaptureEvent::TurnStart {
                connection_id,
                lane,
            });
            exchange.start_ws_usage();
            exchange
        });
        let turn = Arc::new(Turn {
            attempt,
            funnel: funnel.clone(),
            exchange,
            request_rules,
            response_rules,
            started: AtomicBool::new(false),
        });
        if let Some(exchange) = &turn.exchange {
            exchange.flush_capture().await;
        }
        let installed = {
            let mut state = session.0.lock().unwrap();
            let closed = state.closed || (pending && state.current.is_none());
            let slot = if pending {
                &mut state.pending
            } else {
                &mut state.current
            };
            if closed || slot.is_some() {
                false
            } else {
                *slot = Some(turn.clone());
                true
            }
        };
        if !installed {
            if let Some(exchange) = &turn.exchange {
                exchange
                    .finish(UsageStreamEnd::Interrupted, None, now_ms())
                    .await;
            }
            funnel.finish(UsageState::Cancelled).await;
            return Err(CoreError::InvalidTarget(
                "Responses lane is closed or busy".into(),
            ));
        }
        funnel.trace(TraceEvent::AttemptStarted(&turn.attempt));
        let settled = funnel.arm();
        drop(guard);
        Ok(Execution::new((), completion, settled))
    }
}
