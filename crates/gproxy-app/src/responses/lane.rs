use super::*;
use std::collections::BTreeSet;

use futures_util::{SinkExt, stream::SelectAll};
use gproxy_core::{ResponsesSession, SessionIdentity, SessionSource, UsageCompletion};
use gproxy_protocol::{
    WireRequest,
    capability::UpstreamConnection,
    connection::{Bytes, WsSender},
};
use serde::{Deserialize, Serialize};

use crate::{AdmissionRequest, Admitted, call::drive};

#[derive(Clone, Serialize, Deserialize)]
struct Binding {
    chain: String,
    provider: String,
    credential: String,
    model: String,
    expires_at_ms: i64,
    #[serde(default)]
    http_bridge: bool,
}

struct Chain {
    session: ResponsesSession,
    outgoing: WsSender,
    completion: UsageCompletion,
    binding: Binding,
    cancellation: tokio_util::sync::CancellationToken,
    last_used: web_time::Instant,
}

struct Turn {
    id: String,
    chain: String,
    original: Value,
    admitted: Admitted,
    usage: UsageCompletion,
    capture: Option<DownstreamCapture>,
    response_id: Option<String>,
}

#[cfg(not(target_arch = "wasm32"))]
use futures_util::stream::BoxStream;
#[cfg(target_arch = "wasm32")]
use futures_util::stream::LocalBoxStream as BoxStream;

type Incoming = BoxStream<'static, (String, Option<Result<WsFrame, TransportError>>)>;

struct Runner<C> {
    context: Arc<Context<C>>,
    lane: Lane,
    events: mpsc::Sender<Event>,
    chains: HashMap<String, Chain>,
    responses: HashMap<String, String>,
    incoming: SelectAll<Incoming>,
    current: Option<Turn>,
    pending: Option<Turn>,
    sequence: i64,
    activity: web_time::Instant,
}

pub(super) async fn run<C: BatchConnectionTrait + Send + Sync + 'static>(
    context: Arc<Context<C>>,
    lane: Lane,
    mut commands: mpsc::Receiver<Message>,
    events: mpsc::Sender<Event>,
) {
    let mut runner = Runner {
        context,
        lane,
        events,
        chains: HashMap::new(),
        responses: HashMap::new(),
        incoming: SelectAll::new(),
        current: None,
        pending: None,
        sequence: 0,
        activity: web_time::Instant::now(),
    };
    let idle = runner
        .context
        .app
        .gproxy()
        .core()
        .snapshot()
        .limits
        .stream_idle_timeout;
    loop {
        let deadline = runner
            .chains
            .iter()
            .filter(|(id, _)| !runner.active(id))
            .map(|(_, chain)| chain.last_used + idle)
            .min()
            .or_else(|| runner.chains.is_empty().then_some(runner.activity + idle));
        tokio::select! {
            () = runner.context.cancellation.cancelled() => break,
            frame = runner.incoming.next(), if !runner.incoming.is_empty() => {
                if let Some((chain, frame)) = frame { runner.receive(&chain, frame).await; }
            }
            command = commands.recv() => {
                let Some(command) = command else { break };
                runner.activity = web_time::Instant::now();
                runner.command(command).await;
            }
            _ = crate::rt::timeout(deadline.map(|at| at.saturating_duration_since(web_time::Instant::now())).unwrap_or_default(), std::future::pending::<()>()), if deadline.is_some() => {
                let expired: Vec<_> = runner.chains.iter().filter(|(id, chain)| !runner.active(id) && chain.last_used.elapsed() >= idle).map(|(id, _)| id.clone()).collect();
                for id in expired {
                    if let Some(chain) = runner.chains.get_mut(&id) {
                        let _ = crate::rt::timeout(std::time::Duration::from_secs(5), chain.outgoing.send(WsFrame::Close(None))).await;
                    }
                    runner.disconnected(&id).await;
                }
                if runner.chains.is_empty() && commands.is_empty() { break; }
            }
        }
    }
    let sessions: Vec<_> = runner.chains.values().map(|c| c.session.clone()).collect();
    for session in sessions {
        session.close().await;
    }
    let _ = crate::rt::timeout(std::time::Duration::from_secs(5), async {
        for chain in runner.chains.values_mut() {
            let _ = chain.outgoing.send(WsFrame::Close(None)).await;
        }
    })
    .await;
    runner.incoming.clear();
    for (_, chain) in runner.chains.drain() {
        drop(chain.outgoing);
        let _ = chain.completion.await;
    }
    if let Some(turn) = runner.current.take() {
        runner.finish(turn, CaptureOutcome::Cancelled, None).await;
    }
    if let Some(turn) = runner.pending.take() {
        runner.finish(turn, CaptureOutcome::Cancelled, None).await;
    }
}

impl<C: BatchConnectionTrait + Send + Sync + 'static> Runner<C> {
    fn active(&self, chain: &str) -> bool {
        self.current
            .as_ref()
            .is_some_and(|turn| turn.chain == chain)
            || self
                .pending
                .as_ref()
                .is_some_and(|turn| turn.chain == chain)
    }

    async fn command(&mut self, message: Message) {
        let kind = message.value["type"].as_str().unwrap_or("");
        if !matches!(kind, "response.create" | "response.steer")
            && let Some(turn) = &self.current
            && turn.capture.is_some()
            && let Some(sequence) = message.capture_sequence
        {
            let _ = self
                .events
                .send(Event::RequestTurn {
                    sequence,
                    turn: turn.id.clone(),
                })
                .await;
        }
        if kind == "response.create" {
            if self.current.is_some() {
                self.reply(
                    None,
                    error(
                        409,
                        self.lane.as_deref(),
                        "response_in_progress",
                        "lane already has an active response",
                    ),
                )
                .await;
                return;
            }
            self.create(message, false).await;
            return;
        }
        let Some(current) = &self.current else {
            let known = message.value["response_id"]
                .as_str()
                .is_some_and(|id| self.responses.contains_key(id));
            let code = if known {
                "response_already_completed"
            } else {
                "response_not_found"
            };
            self.reply(
                None,
                control_error(
                    &message.value,
                    self.lane.as_deref(),
                    code,
                    "lane has no active response",
                    self.sequence,
                ),
            )
            .await;
            self.sequence += 1;
            return;
        };
        let chain_id = current.chain.clone();
        let turn_id = current.id.clone();
        let response_id = message.value["response_id"]
            .as_str()
            .or_else(|| message.value["previous_response_id"].as_str());
        if matches!(
            kind,
            "response.steer" | "response.inject" | "response.interrupt"
        ) && (response_id.is_none() || response_id != current.response_id.as_deref())
        {
            let known = response_id.is_some_and(|id| self.responses.contains_key(id));
            let code = if known {
                "response_already_completed"
            } else {
                "response_not_found"
            };
            self.reply(
                Some(turn_id),
                control_error(
                    &message.value,
                    self.lane.as_deref(),
                    code,
                    "control must name the active response",
                    self.sequence,
                ),
            )
            .await;
            self.sequence += 1;
            return;
        }
        if response_id.is_some_and(|id| self.responses.get(id) != Some(&chain_id)) {
            self.reply(
                Some(turn_id),
                error(
                    400,
                    self.lane.as_deref(),
                    "response_not_found",
                    "response is outside the active chain",
                ),
            )
            .await;
            return;
        }
        if kind == "response.steer" {
            if let Err(error) = serde_json::from_value::<
                gproxy_protocol::wire::openai::responses::steering::SteerRequest,
            >(message.value.clone())
            {
                self.reply(
                    Some(turn_id),
                    control_error(
                        &message.value,
                        self.lane.as_deref(),
                        "invalid_request",
                        &error.to_string(),
                        self.sequence,
                    ),
                )
                .await;
                self.sequence += 1;
                return;
            }
            if self.pending.is_some() {
                self.reply(
                    Some(turn_id),
                    error(
                        409,
                        self.lane.as_deref(),
                        "steering_in_progress",
                        "a successor is already reserved",
                    ),
                )
                .await;
                return;
            }
            let mut body = current.original.clone();
            body["previous_response_id"] = message.value["previous_response_id"].clone();
            let next = Message {
                value: body,
                text: message.text.clone(),
                request_id: message.request_id,
                capture_sequence: message.capture_sequence,
            };
            if !self.create(next, true).await {
                return;
            }
        } else if kind != "response.interrupt"
            && let Err(error) = self.authorize_control().await
        {
            self.reply(
                Some(turn_id),
                super::error(
                    error.status_code(),
                    self.lane.as_deref(),
                    error.code(),
                    &error.to_string(),
                ),
            )
            .await;
            return;
        }
        if let Some(chain) = self.chains.get_mut(&chain_id)
            && chain
                .outgoing
                .send(WsFrame::Text(message.text))
                .await
                .is_err()
        {
            self.disconnected(&chain_id).await;
        }
    }

    async fn create(&mut self, message: Message, pending: bool) -> bool {
        let mut request = self.request(&message);
        let model = message.value["model"]
            .as_str()
            .filter(|m| !m.is_empty())
            .map(|m| self.model(m));
        request.model = model.clone();
        let capture = DownstreamCapture::open_turn(
            &self.context.app.data().observation,
            &request,
            &self.context.caller,
            &self.context.request.request_id,
            self.lane.as_deref(),
        );
        if capture.is_some()
            && let Some(sequence) = message.capture_sequence
        {
            let _ = self
                .events
                .send(Event::RequestTurn {
                    sequence,
                    turn: message.request_id.clone(),
                })
                .await;
        }
        let result = self.prepare(&request, &message.value, pending).await;
        let (chain_id, mut admitted, usage) = match result {
            Ok(value) => value,
            Err(error) => {
                self.reply(
                    capture.as_ref().map(|_| message.request_id.clone()),
                    app_error(self.lane.as_deref(), &error),
                )
                .await;
                let _ = self
                    .events
                    .send(Event::Finished {
                        capture: capture.map(Box::new),
                        outcome: CaptureOutcome::Failed {
                            error: error.to_string(),
                        },
                    })
                    .await;
                return false;
            }
        };
        if !pending {
            admitted.finish();
        }
        let turn = Turn {
            id: message.request_id.clone(),
            chain: chain_id.clone(),
            original: message.value,
            admitted,
            usage,
            capture,
            response_id: None,
        };
        if pending {
            self.pending = Some(turn);
        } else {
            self.current = Some(turn);
            let chain = self.chains.get_mut(&chain_id).unwrap();
            if chain
                .outgoing
                .send(WsFrame::Text(message.text))
                .await
                .is_err()
            {
                self.disconnected(&chain_id).await;
                return false;
            }
        }
        true
    }

    async fn prepare(
        &mut self,
        request: &DataPlaneRequest,
        body: &Value,
        pending: bool,
    ) -> Result<(String, Admitted, UsageCompletion), AppError> {
        let model = request
            .model
            .as_deref()
            .ok_or_else(|| AppError::invalid("response.create.model is required"))?;
        let caller = self.refresh_caller().await?;
        let data = self.context.app.data();
        let core = self.context.app.gproxy().core().snapshot();
        let providers: BTreeSet<_> = core
            .providers
            .keys()
            .filter(|id| {
                request
                    .provider_id
                    .as_ref()
                    .is_none_or(|selected| selected == *id)
            })
            .cloned()
            .collect();
        let credentials = core.credentials.keys().cloned().collect();
        let warmup = body["generate"] == false;
        let mut admitted = self
            .context
            .app
            .admission(&data)
            .admit_with_rates(
                AdmissionRequest::new(
                    &caller,
                    request.operation,
                    &request.parts.headers,
                    &request.request_id,
                    &providers,
                    &credentials,
                )
                .model(Some(model))
                .body(Some(body))
                .agent_session_id(request.agent_session_id.as_deref()),
                !warmup,
                !warmup && !pending,
            )
            .await?;
        let result = async {
        let previous = body["previous_response_id"].as_str();
        let remembered = match previous {
            Some(id) => match self.responses.get(id).and_then(|chain| self.chains.get(chain)) {
                Some(chain) => Some(chain.binding.clone()),
                None => {
                    let entry = self.context.app.gproxy().store().protocol_states()
                        .get_live_many(&[(binding_scope(&admitted.scope), id.into())], crate::now_ms()).await?
                        .pop().flatten()
                        .ok_or_else(|| AppError::invalid("previous_response_id is not available; supply full input without it"))?;
                    Some(serde_json::from_slice::<Binding>(&entry.payload).map_err(|e| AppError::internal(e.to_string()))?)
                }
            },
            None => None,
        };
        if let Some(binding) = &remembered {
            admitted.providers.retain(|p| p == &binding.provider);
            admitted.credentials.retain(|c| c == &binding.credential);
        }
        let chain_id = remembered.as_ref().map(|b| b.chain.clone()).unwrap_or_else(random_id);
        let agent_session_id = admitted.session.as_ref().and_then(|s| s.agent_session_id.clone());
        admitted.session = Some(SessionIdentity { id: chain_id.clone(), source: SessionSource::Generic,
            field: Some("responses.chain".into()), agent_session_id });
        if !self.chains.contains_key(&chain_id) {
            let session = remembered.as_ref().map(|b| {
                let session = ResponsesSession::with_expiry(b.expires_at_ms);
                if b.http_bridge { session.with_http_bridge() } else { session }
            })
                .unwrap_or_default();
            let mut handshake = request.parts.clone();
            // The chain's cancellation belongs to the client connection, not
            // to its first generation. Idle reuse must survive a finished turn.
            handshake.method = http::Method::GET;
            let wire = wire(&handshake);
            let mut builder = drive!(self.context.app.gproxy().connect(request.operation, wire), admitted, request, Some(model));
            let cancellation = self.context.cancellation.child_token();
            builder = builder.cancellation(cancellation.clone());
            let execution = builder.open_responses(session.clone()).await?;
            let (connection, completion) = execution.into_parts();
            let socket = match connection {
                UpstreamConnection::Connected { socket, .. } => socket,
                UpstreamConnection::Rejected(response) => {
                    let status = response.status.as_u16();
                    let body = collect(response.body).await;
                    let _ = completion.await;
                    let message = serde_json::from_slice::<Value>(&body).ok()
                        .filter(|v| v["error"]["message"].is_string())
                        .map(|v| json!({"error":v["error"]}).to_string())
                        .unwrap_or_else(|| format!("upstream rejected WebSocket handshake ({status})"));
                    return Err(AppError::Sdk(gproxy_sdk::SdkError::Upstream { status, body: message }));
                }
            };
            let bound = session.binding().ok_or_else(|| AppError::internal("Responses target not bound"))?;
            if let Some(remembered) = &remembered
                && remembered.model != bound.model
            { return Err(AppError::invalid("continuation model differs from its bound model")); }
            let binding = Binding { chain: chain_id.clone(), provider: bound.provider_id,
                credential: bound.credential_id, model: bound.model, expires_at_ms: bound.expires_at_ms, http_bridge: session.is_http_bridge() };
            let id = chain_id.clone();
            self.incoming.push(Box::pin(socket.incoming.take_until(cancellation.clone().cancelled_owned()).map(Some)
                .chain(futures_util::stream::once(async { None }))
                .map(move |frame| (id.clone(), frame))));
            self.chains.insert(chain_id.clone(), Chain { session, outgoing: socket.outgoing, completion, binding, cancellation, last_used: web_time::Instant::now() });
        }
        let (session, binding) = {
            let chain = self.chains.get_mut(&chain_id).unwrap();
            chain.last_used = web_time::Instant::now();
            (chain.session.clone(), chain.binding.clone())
        };
        if remembered.is_some() || !session.is_http_bridge() {
            admitted.providers.retain(|p| p == &binding.provider);
            admitted.credentials.retain(|c| c == &binding.credential);
        }
        let builder = drive!(self.context.app.gproxy().connect(request.operation, wire(&request.parts)), admitted, request, Some(model));
        let (_, usage) = builder.begin_responses_turn(&session, body, self.lane.as_deref(), pending).await?.into_parts();
        Ok::<_, AppError>((chain_id, usage))
        }.await;
        match result {
            Ok((chain_id, usage)) => Ok((chain_id, admitted, usage)),
            Err(error) => {
                admitted.release().await;
                Err(error)
            }
        }
    }

    async fn receive(&mut self, chain_id: &str, frame: Option<Result<WsFrame, TransportError>>) {
        if let Some(chain) = self.chains.get_mut(chain_id)
            && let Some(binding) = chain.session.binding()
        {
            chain.binding.provider = binding.provider_id;
            chain.binding.credential = binding.credential_id;
            chain.binding.model = binding.model;
            chain.binding.http_bridge = chain.session.is_http_bridge();
        }
        let frame = match frame {
            Some(Ok(WsFrame::Ping(bytes))) => {
                if let Some(chain) = self.chains.get_mut(chain_id) {
                    let _ = chain.outgoing.send(WsFrame::Pong(bytes)).await;
                }
                return;
            }
            Some(Ok(WsFrame::Pong(_))) => return,
            Some(Ok(WsFrame::Close(_))) | Some(Err(_)) | None => {
                self.disconnected(chain_id).await;
                return;
            }
            Some(Ok(frame)) => frame,
        };
        let Some(chain) = self.chains.get_mut(chain_id) else {
            return;
        };
        chain.last_used = web_time::Instant::now();
        let value = match &frame {
            WsFrame::Text(text) => serde_json::from_str::<Value>(text).ok(),
            _ => None,
        };
        if let Some(id) = value.as_ref().and_then(|v| v["response"]["id"].as_str()) {
            self.responses.insert(id.into(), chain_id.into());
        }
        let kind = value
            .as_ref()
            .and_then(|v| v["type"].as_str())
            .unwrap_or("");
        if kind == "response.created"
            && let Some(turn) = self.current.as_mut()
            && turn.chain == chain_id
        {
            turn.response_id = value
                .as_ref()
                .and_then(|v| v["response"]["id"].as_str())
                .map(str::to_owned);
        }
        if let Some(sequence) = value.as_ref().and_then(|v| v["sequence_number"].as_i64()) {
            self.sequence = self.sequence.max(sequence + 1);
        }
        let turn_id = self
            .current
            .as_ref()
            .filter(|t| t.chain == chain_id)
            .map(|t| t.id.clone());
        let reservation_rejected = kind == "error"
            && self.pending.as_ref().is_some_and(|turn| {
                self.chains.get(chain_id).is_none_or(|chain| {
                    chain.session.pending_request_id().as_deref() != Some(&turn.id)
                })
            });
        if (kind == "response.steer.failed" || reservation_rejected)
            && let Some(turn) = self.pending.take()
        {
            self.finish(
                turn,
                CaptureOutcome::Failed {
                    error: "steering rejected".into(),
                },
                None,
            )
            .await;
        }
        let generation_error = kind == "error"
            && self
                .chains
                .get(chain_id)
                .is_none_or(|chain| chain.session.current_request_id() != turn_id);
        if (matches!(
            kind,
            "response.completed" | "response.incomplete" | "response.failed"
        ) || generation_error)
            && self.current.as_ref().is_some_and(|t| t.chain == chain_id)
        {
            let mut turn = self.current.take().unwrap();
            let mut next = self.pending.take();
            if matches!(kind, "response.failed" | "error")
                && let Some(pending) = next.take()
            {
                self.finish(pending, CaptureOutcome::Cancelled, None).await;
            }
            if let Some(next) = &mut next {
                let mut kept = Vec::new();
                for lease in turn.admitted.rate_limit_leases.drain(..) {
                    if lease.is_permit() {
                        next.admitted.rate_limit_leases.push(lease);
                    } else {
                        kept.push(lease);
                    }
                }
                turn.admitted.rate_limit_leases = kept;
                next.admitted.finish();
            }
            let outcome = if matches!(kind, "response.failed" | "error") {
                CaptureOutcome::Failed {
                    error: "Responses generation failed".into(),
                }
            } else {
                CaptureOutcome::Complete
            };
            let saved = self.finish(turn, outcome, value.as_ref()).await;
            self.current = next;
            if !saved {
                self.disconnected(chain_id).await;
                return;
            }
        }
        self.reply(turn_id, frame).await;
    }

    async fn finish(
        &mut self,
        mut turn: Turn,
        mut outcome: CaptureOutcome,
        terminal: Option<&Value>,
    ) -> bool {
        let _ = turn.usage.await;
        let mut saved = true;
        if turn.original["store"] != false
            && let Some(id) = terminal.and_then(|v| v["response"]["id"].as_str())
            && let Some(chain) = self.chains.get(&turn.chain)
        {
            let binding = chain.binding.clone();
            if let Err(error) = save_binding(
                self.context.app.gproxy().store(),
                &turn.admitted.scope,
                id,
                &binding,
            )
            .await
            {
                saved = false;
                outcome = CaptureOutcome::Failed {
                    error: error.to_string(),
                };
                self.reply(
                    Some(turn.id.clone()),
                    super::error(
                        500,
                        self.lane.as_deref(),
                        "continuation_unavailable",
                        "could not save the response continuation",
                    ),
                )
                .await;
            }
        }
        turn.admitted.release().await;
        let _ = self
            .events
            .send(Event::Finished {
                capture: turn.capture.map(Box::new),
                outcome,
            })
            .await;
        saved
    }

    async fn disconnected(&mut self, chain_id: &str) {
        if let Some(chain) = self.chains.remove(chain_id) {
            chain.cancellation.cancel();
            chain.session.close().await;
            drop(chain.outgoing);
            // Its incoming stream may be queued in SelectAll. Close settlement
            // is handled by that stream's guard; don't wait on it here.
            drop(chain.completion);
        }
        let expired: Vec<_> = self
            .responses
            .iter()
            .filter(|(_, chain)| *chain == chain_id)
            .map(|(id, _)| id.clone())
            .collect();
        self.responses.retain(|_, chain| chain != chain_id);
        if !expired.is_empty() {
            let _ = self
                .events
                .send(Event::Forgotten {
                    lane: self.lane.clone(),
                    responses: expired,
                })
                .await;
        }
        if self.current.as_ref().is_some_and(|t| t.chain == chain_id) {
            let turn = self.current.take().unwrap();
            self.reply(
                Some(turn.id.clone()),
                error(
                    502,
                    self.lane.as_deref(),
                    "upstream_disconnected",
                    "upstream closed before response completion",
                ),
            )
            .await;
            self.finish(turn, CaptureOutcome::Interrupted, None).await;
        }
        if self.pending.as_ref().is_some_and(|t| t.chain == chain_id) {
            let turn = self.pending.take().unwrap();
            self.finish(turn, CaptureOutcome::Cancelled, None).await;
        }
    }

    async fn reply(&mut self, turn: Option<String>, mut frame: WsFrame) {
        if let (Some(lane), WsFrame::Text(text)) = (&self.lane, &mut frame)
            && let Ok(mut value) = serde_json::from_str::<Value>(text)
            && value.get("stream_id").is_none_or(Value::is_null)
        {
            value["stream_id"] = lane.clone().into();
            *text = value.to_string();
        }
        let _ = self
            .events
            .send(Event::Frame {
                lane: self.lane.clone(),
                turn,
                frame,
            })
            .await;
    }

    async fn refresh_caller(&mut self) -> Result<Caller, AppError> {
        // An edge isolate has no background synchronizer. A WS turn can arrive
        // long after the upgrade's tick, including after permissions changed.
        #[cfg(target_arch = "wasm32")]
        {
            self.context.app.gproxy().tick().await?;
            self.context.app.tick().await?;
        }
        let data = self.context.app.data();
        let authentication = self
            .context
            .app
            .authenticator(&data)
            .authenticate_request(&self.context.request.parts.headers)
            .await;
        let caller = match authentication {
            Err(AppError::Unauthorized(reason)) => {
                let key = self.context.request.parts.uri.query().and_then(|query| {
                    form_urlencoded::parse(query.as_bytes())
                        .find(|(name, _)| name == "key")
                        .map(|(_, value)| value.into_owned())
                });
                match key {
                    Some(key) => {
                        self.context
                            .app
                            .authenticator(&data)
                            .authenticate_token(&key)
                            .await?
                    }
                    None => return Err(AppError::Unauthorized(reason)),
                }
            }
            result => result?,
        };
        if caller.user_id != self.context.caller.user_id
            || caller.api_key_id != self.context.caller.api_key_id
            || caller.organization_id != self.context.caller.organization_id
            || caller.team_id != self.context.caller.team_id
        {
            return Err(AppError::forbidden("Responses connection identity changed"));
        }
        Ok(caller)
    }

    async fn authorize_control(&mut self) -> Result<(), AppError> {
        let caller = self.refresh_caller().await?;
        let (turn_id, model, original, chain) = {
            let turn = self.current.as_ref().unwrap();
            (
                turn.id.clone(),
                turn.admitted.attribution.model.clone(),
                turn.original.clone(),
                turn.chain.clone(),
            )
        };
        let data = self.context.app.data();
        let core = self.context.app.gproxy().core().snapshot();
        let providers = core.providers.keys().cloned().collect();
        let credentials = core.credentials.keys().cloned().collect();
        let admitted = self
            .context
            .app
            .admission(&data)
            .admit_with_rates(
                AdmissionRequest::new(
                    &caller,
                    self.context.request.operation,
                    &self.context.request.parts.headers,
                    &turn_id,
                    &providers,
                    &credentials,
                )
                .model(model.as_deref())
                .body(Some(&original)),
                false,
                false,
            )
            .await?;
        let binding = &self.chains.get(&chain).unwrap().binding;
        if !admitted.providers.contains(&binding.provider)
            || !admitted.credentials.contains(&binding.credential)
        {
            return Err(AppError::forbidden(
                "Responses continuation is no longer permitted",
            ));
        }
        Ok(())
    }

    fn request(&self, message: &Message) -> DataPlaneRequest {
        let mut request = DataPlaneRequest::new(
            &message.request_id,
            self.context.request.operation,
            self.context.request.parts.clone(),
            Bytes::copy_from_slice(message.text.as_bytes()),
        );
        request.provider_id = self.context.request.provider_id.clone();
        request.client_ip = self.context.request.client_ip.clone();
        request.cancellation = Some(self.context.cancellation.child_token());
        request.agent_session_id = self.context.request.agent_session_id.clone();
        request
    }

    fn model(&self, model: &str) -> String {
        match &self.context.prefix {
            Some(prefix)
                if !model
                    .strip_prefix(prefix)
                    .is_some_and(|s| s.starts_with('/')) =>
            {
                format!("{prefix}/{model}")
            }
            _ => model.into(),
        }
    }
}

fn wire(parts: &http::request::Parts) -> WireRequest<()> {
    WireRequest {
        method: http::Method::GET,
        path: parts.uri.path().into(),
        query: parts
            .uri
            .query()
            .map(|query| {
                query
                    .split('&')
                    .filter(|pair| {
                        let key = pair.split('=').next().unwrap_or("");
                        let key = form_urlencoded::parse(key.as_bytes())
                            .next()
                            .map(|(key, _)| key.into_owned())
                            .unwrap_or_default();
                        !matches!(key.as_str(), "key" | "api_key" | "access_token")
                    })
                    .collect::<Vec<_>>()
                    .join("&")
            })
            .filter(|query| !query.is_empty()),
        headers: parts.headers.clone(),
        body: (),
    }
}

fn binding_scope(scope: &str) -> String {
    format!("responses-target:{}:{scope}", scope.len())
}

async fn save_binding<C: BatchConnectionTrait>(
    store: &gproxy_store::Store<C>,
    scope: &str,
    response: &str,
    binding: &Binding,
) -> Result<(), AppError> {
    use gproxy_store::operations::state::{StateChange, StateOutcome, StateValue};
    let scope = binding_scope(scope);
    let payload = serde_json::to_vec(binding).unwrap();
    let result = store
        .protocol_states()
        .compare_exchange_many(
            vec![StateChange {
                scope: scope.clone(),
                key: response.into(),
                expected: None,
                replacement: Some(StateValue {
                    payload: payload.clone(),
                    expires_at_ms: Some(binding.expires_at_ms),
                }),
            }],
            crate::now_ms(),
        )
        .await?;
    if matches!(result.first(), Some(StateOutcome::Applied(_))) {
        return Ok(());
    }
    let previous = store
        .protocol_states()
        .get_live_many(&[(scope, response.into())], crate::now_ms())
        .await?
        .pop()
        .flatten();
    if previous
        .is_some_and(|r| r.payload == payload && r.expires_at_ms == Some(binding.expires_at_ms))
    {
        return Ok(());
    }
    Err(AppError::Conflict(
        "Responses ID is already bound to another chain".into(),
    ))
}

async fn collect(body: gproxy_protocol::HttpBody) -> Bytes {
    match body {
        gproxy_protocol::HttpBody::Bytes(bytes) => bytes,
        gproxy_protocol::HttpBody::Stream(mut stream) => {
            let mut bytes = Vec::new();
            while let Some(Ok(chunk)) = stream.next().await {
                bytes.extend_from_slice(&chunk);
            }
            bytes.into()
        }
    }
}
