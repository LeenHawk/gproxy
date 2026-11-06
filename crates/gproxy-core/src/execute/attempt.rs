//! The attempt loop for HTTP operations: select, prepare, dispatch (passthrough
//! or conversion), classify, and either return the answer through the funnel
//! or move to the next eligible credential inside the permitted set.

use super::{Funnel, prepare};
use crate::session::{AssignmentHandle, AssignmentOutcome};
use crate::{
    AttemptContext, AttemptOutcome, AttemptUpstream, BlockSource, Core, CoreError, CoreResult,
    CredentialBlock, Execution, HttpExecution, ProtocolState, RequestContext, StateScope,
    TraceEvent, UsageState,
    api::lifecycle::now_ms,
    availability::{
        CREDIT_BLOCK_MS, DEFAULT_EXHAUSTED_MS, DEFAULT_RATE_LIMIT_MS, UNSERVED_BLOCK_MS,
        failure_scope, retry_after_ms,
    },
    convert::{self, Route},
    rewrite::{Phase, RewriteContext, apply_body, apply_headers, apply_query, select_rules},
};
use gproxy_channel::channel::{QuotaScope, Rejection, RejectionContext, RejectionKind};
use gproxy_protocol::{
    HttpBody, WireRequest, WireResponse,
    connection::Bytes,
    transform::{TransformError, TransformErrorKind},
};
use gproxy_seaorm::BatchConnectionTrait;
use http::StatusCode;
use std::{collections::HashSet, sync::Arc, time::Duration};
use web_time::Instant;

fn remaining(request: &RequestContext) -> Option<Duration> {
    request
        .deadline
        .map(|deadline| deadline.saturating_duration_since(Instant::now()))
}

/// What one attempt produced for the caller.
pub(crate) struct Answer {
    pub response: WireResponse<HttpBody>,
    pub attempt: Option<Arc<AttemptContext>>,
    /// A 2xx stream that opened with an error frame: the upstream may keep
    /// it open, so a superseded one is dropped rather than drained.
    pub abandon: bool,
}

impl Answer {
    fn new(response: WireResponse<HttpBody>, attempt: Option<Arc<AttemptContext>>) -> Self {
        Self {
            response,
            attempt,
            abandon: false,
        }
    }

    /// Drain a superseded rejection through observation before retrying. The
    /// observed stream retains its byte/idle/cancellation limits; a total cap
    /// prevents a slow rejection from holding retries indefinitely.
    async fn discard(self, timeout: Duration) {
        if self.abandon {
            return;
        }
        let body = self.response.body;
        super::stream::drain(body, timeout).await;
    }

    fn status(&self) -> StatusCode {
        self.response.status
    }
    fn headers(&self) -> &http::HeaderMap {
        &self.response.headers
    }
    async fn deliver(
        self,
        funnel: &Arc<Funnel>,
        completion: crate::UsageCompletion,
    ) -> HttpExecution {
        let mut response = self.response;
        let sse_limits = response
            .headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| {
                value
                    .split(';')
                    .next()
                    .is_some_and(|value| value.trim() == "text/event-stream")
            })
            .then(|| {
                self.attempt
                    .as_ref()
                    .map(|attempt| attempt.request.snapshot.limits.codec())
            })
            .flatten();
        let state = if response.status.is_client_error() || response.status.is_server_error() {
            UsageState::Failed
        } else {
            UsageState::Completed
        };
        let settled = match response.body {
            HttpBody::Bytes(_) => funnel.finish(state).await,
            HttpBody::Stream(body) => {
                response.body = HttpBody::Stream(super::stream::settling(
                    funnel.clone(),
                    body,
                    state,
                    sse_limits,
                ));
                funnel.arm()
            }
        };
        Execution::new(response, completion, settled)
    }
}

enum Classified {
    /// Returned to the caller as-is: success, or a client-side 4xx.
    Final,
    /// The credential is unusable for now; move on if budget remains.
    Exclude,
    /// Try the same credential once more after a forced refresh.
    Refresh,
}

/// The channel's reading of the refusal decides when it has one; otherwise
/// the status does. 402 (out of balance) fails over like a rate limit.
fn classify(
    status: StatusCode,
    rejection: Option<&Rejection>,
    refreshable: bool,
    refreshed: bool,
) -> Classified {
    match rejection.map(|r| r.kind) {
        Some(RejectionKind::Request) => Classified::Final,
        Some(RejectionKind::Auth) if refreshable && !refreshed => Classified::Refresh,
        Some(_) => Classified::Exclude,
        None => match status.as_u16() {
            401 | 403 if refreshable && !refreshed => Classified::Refresh,
            402 | 429 | 401 | 403 | 404 | 500..=599 => Classified::Exclude,
            _ => Classified::Final,
        },
    }
}

/// The block a refusal earns, before the failure streak is consulted. None
/// leaves the credential to the streak (auth, server errors).
fn rejection_block(
    status: StatusCode,
    rejection: Option<&Rejection>,
    headers: &http::HeaderMap,
    upstream_model: Option<&str>,
    operation: gproxy_protocol::Operation,
    now_ms: i64,
) -> Option<CredentialBlock> {
    let kind = match rejection {
        Some(rejection) => rejection.kind,
        None if status == StatusCode::TOO_MANY_REQUESTS => RejectionKind::RateLimited,
        None if status == StatusCode::PAYMENT_REQUIRED => RejectionKind::Credit,
        None => return None,
    };
    let said = rejection
        .and_then(|r| r.retry_after)
        .map(|wait| i64::try_from(wait.as_millis()).unwrap_or(i64::MAX))
        .or_else(|| retry_after_ms(headers, now_ms));
    let (source, default_ms) = match kind {
        RejectionKind::RateLimited => (BlockSource::RateLimited, DEFAULT_RATE_LIMIT_MS),
        RejectionKind::QuotaExhausted => (BlockSource::Exhausted, DEFAULT_EXHAUSTED_MS),
        RejectionKind::Credit => (BlockSource::Credit, CREDIT_BLOCK_MS),
        // Unserved is about one model; without one it is left to the streak.
        RejectionKind::ModelUnserved if upstream_model.is_some() => {
            (BlockSource::Unserved, UNSERVED_BLOCK_MS)
        }
        RejectionKind::ModelUnserved | RejectionKind::Auth | RejectionKind::Request => {
            return None;
        }
    };
    let model_scoped =
        matches!(kind, RejectionKind::ModelUnserved) || rejection.is_some_and(|r| r.model_scoped);
    Some(CredentialBlock {
        scope: if model_scoped {
            failure_scope(upstream_model)
        } else {
            QuotaScope::All
        },
        operation: (!operation.produces_usage()).then_some(operation),
        until_ms: now_ms.saturating_add(said.unwrap_or(default_ms)),
        source,
        observed_at_ms: now_ms,
    })
}

/// What went wrong before any upstream answer arrived.
enum Fault {
    Cancelled,
    DeadlineExceeded,
    /// Transport or channel failure: counts against the credential's streak.
    Failed(CoreError),
    /// Conversion refused the request itself; no credential is at fault.
    Client(CoreError),
}

pub(crate) async fn run_http<C: BatchConnectionTrait + Send + Sync + 'static>(
    core: &Core<C>,
    request: Arc<RequestContext>,
    wire: WireRequest<HttpBody>,
) -> CoreResult<HttpExecution> {
    let (funnel, completion) = Funnel::new(request.clone(), core.observer().clone());
    let _request_guard = funnel.guard();
    let result = run_http_attempts(core, request, wire, funnel.clone(), true).await;
    if let Err(error) = &result {
        let state = if matches!(error, CoreError::Cancelled) {
            UsageState::Cancelled
        } else {
            UsageState::Failed
        };
        funnel.finish(state).await;
    }
    match result {
        Ok(answer) => Ok(answer.deliver(&funnel, completion).await),
        Err(error) => Err(error),
    }
}

pub(crate) async fn run_http_attempts<C: BatchConnectionTrait + Send + Sync + 'static>(
    core: &Core<C>,
    request: Arc<RequestContext>,
    wire: WireRequest<HttpBody>,
    funnel: Arc<Funnel>,
    retry_transport: bool,
) -> CoreResult<Answer> {
    funnel.set_meter(core.usage_meter());
    reject_when_over_budget(core, &request, &funnel).await?;
    let snapshot = request.snapshot.clone();
    let operation = request.operation;
    let limits = request
        .execution_limits()
        .for_operation(operation.operation);
    let provider = request.target.provider.clone();
    let upstream_model = request.target.upstream_model.clone();

    let route = match convert::route_for_model(&provider, operation, upstream_model.as_deref()) {
        Ok(route) => route,
        Err(error) => {
            return Err(error.into());
        }
    };
    if route == Route::Unsupported {
        return Err(CoreError::Transform(TransformError::unsupported(
            "route",
            format!(
                "{:?} / {:?} is not supported by provider {}",
                operation.operation, operation.dialect, provider.entity.id
            ),
        )));
    }
    let local = route == Route::Local
        && !(provider
            .channel
            .local_operations()
            .contains(&operation.operation)
            && provider
                .channel
                .native_dialects_for_model(
                    prepare::provider_view(&provider),
                    operation.operation,
                    upstream_model.as_deref(),
                )
                .contains(&operation.dialect));
    let inbound_headers = wire.headers.clone();
    // The file or video id a delete names, for forgetting its owner row once
    // the upstream confirms (`crate::owned`).
    let resource_path = wire.path.clone();
    let rewrite_context = RewriteContext {
        operation,
        upstream_model: upstream_model.as_deref(),
        requested_model: request.target.requested_model.as_deref(),
        request_headers: &inbound_headers,
    };
    // Passthrough rules apply to the client's request once; conversion applies
    // rules per native call inside AttemptUpstream.
    let request_rules = select_rules(&snapshot, &provider, Phase::Request, &rewrite_context);
    let response_rules = select_rules(&snapshot, &provider, Phase::Response, &rewrite_context);

    let converting = matches!(route, Route::TransformTo { .. });
    if converting && convert::is_websocket(operation) {
        return Err(CoreError::Transform(TransformError::unsupported(
            "route",
            "WebSocket operations are passthrough only over HTTP",
        )));
    }
    let remap_model = !converting
        && upstream_model
            .as_deref()
            .is_some_and(|model| request.attribution.model.as_deref() != Some(model));
    let strip_thinking = super::thinking::enabled(&provider.entity.config, operation);
    let claude_compat = gproxy_protocol::adapt::generate::claude_compat::applies_to(operation);
    let want_replay = strip_thinking
        || claude_compat
        || provider.channel.claude_fallback().is_some()
        || converting
        || local
        || remap_model
        || request.max_attempts.get() > 1
        || !request_rules.body.is_empty();
    let (mut wire, replayable) =
        prepare::buffer_request(wire, want_replay, limits.max_request_body_bytes).await;
    if (converting || remap_model || local || strip_thinking || claude_compat) && !replayable {
        return Err(CoreError::Transform(TransformError::new(
            TransformErrorKind::Limit,
            "client.body",
            "request body exceeds the buffering limit required for request preparation",
        )));
    }
    if !converting {
        if let Some(model) = upstream_model.as_deref() {
            prepare::apply_model(&mut wire, operation.dialect, model);
            let mut codec = limits.codec();
            codec.max_body_bytes = limits.max_request_body_bytes;
            prepare::apply_form_model(&mut wire, model, codec).await?;
            if matches!(
                operation.operation,
                gproxy_protocol::Operation::CreateRealtimeCall
                    | gproxy_protocol::Operation::CreateLiveSession
                    | gproxy_protocol::Operation::CreateRealtimeClientSecret
            ) {
                crate::realtime::request::apply_model(&mut wire, model, codec).await?;
            }
        }
        if !request_rules.headers.is_empty() {
            apply_headers(&request_rules.headers, &mut wire.headers)?;
        }
        if !request_rules.query.is_empty()
            && let Some(query) = apply_query(&request_rules.query, wire.query.as_deref())?
        {
            wire.query = Some(query);
        }
        if !request_rules.body.is_empty()
            && let HttpBody::Bytes(bytes) = &wire.body
            && let Some(rewritten) = apply_body(&request_rules.body, bytes)?
        {
            wire.body = HttpBody::Bytes(Bytes::from(rewritten));
        }
    }
    if strip_thinking {
        super::thinking::strip_request(operation.dialect, &mut wire);
    }
    if local {
        let mut response = convert::local::run(&request, &wire)?;
        apply_headers(&response_rules.headers, &mut response.headers)?;
        if let HttpBody::Bytes(bytes) = &response.body
            && let Some(rewritten) = apply_body(&response_rules.body, bytes)?
        {
            response.body = HttpBody::Bytes(Bytes::from(rewritten));
        }
        return Ok(Answer::new(response, None));
    }
    let attempts = if replayable {
        request.max_attempts.get()
    } else {
        1
    };

    let mut excluded: HashSet<String> = HashSet::new();
    let mut refreshed: HashSet<String> = HashSet::new();
    let mut wire = Some(wire);
    let mut ordinal = 0u32;
    // The most recent rejected upstream answer, kept alive so it can be
    // returned if no further credential is usable.
    let mut held: Option<Answer> = None;
    // A reservation carried over a same-credential retry (401 -> refresh).
    let mut carried: Option<AssignmentHandle> = None;
    while ordinal < attempts {
        ordinal += 1;
        let now = now_ms();
        if request.cancellation.is_cancelled() {
            drop(held.take());
            return Err(CoreError::Cancelled);
        }
        let budget = remaining(&request);
        if budget.is_some_and(|left| left.is_zero()) {
            drop(held.take());
            return Err(CoreError::DeadlineExceeded);
        }
        let selection = match core
            .select_credential(&request, &inbound_headers, &excluded, now)
            .await
        {
            Ok(selection) => selection,
            Err(CoreError::NoUsableCredential { .. }) if held.is_some() => {
                let answer = held.take().expect("held");
                return Ok(answer);
            }
            Err(error) => {
                drop(held.take());
                return Err(error);
            }
        };
        // A new attempt supersedes the previous rejected answer.
        if let Some(answer) = held.take() {
            answer
                .discard(limits.capability(remaining(&request)).operation_total)
                .await;
        }
        let credential = selection.credential;
        let mut assignment = match (selection.assignment, carried.take()) {
            (Some(handle), Some(previous))
                if previous.reference.assignment_id == handle.reference.assignment_id =>
            {
                Some(previous)
            }
            (handle, Some(previous)) => {
                settle(core, &mut Some(previous), failed(false, "superseded"), now).await?;
                handle
            }
            (handle, None) => handle,
        };
        // Material about to expire is refreshed before it is pinned; a failed
        // refresh still lets this attempt try the current material.
        let version = credential.state.load();
        if crate::refresh::needs_refresh(&provider, &credential, &version, now)
            && crate::refresh::can_refresh(&provider, &credential, &version)
        {
            let _ = core
                .refresh_credential(
                    &credential.provider_id,
                    &credential.id,
                    crate::RefreshMode::IfNeeded,
                )
                .await;
        }
        // Self-counted request quota is charged before anything goes out; a
        // window at its limit blocks the credential and the next one is tried.
        if core
            .charge_request(
                &credential,
                operation.operation,
                upstream_model.as_deref(),
                now,
            )
            .await?
            .is_some()
        {
            settle(
                core,
                &mut assignment,
                failed(false, "counted window full"),
                now,
            )
            .await?;
            excluded.insert(credential.id.clone());
            ordinal -= 1;
            if excluded.len() >= request.target.credentials.len() {
                break;
            }
            continue;
        }
        let version = credential.state.load();
        let attempt_ordinal = funnel.next_attempt();
        let attempt = Arc::new(AttemptContext {
            attempt_id: format!("{}-{attempt_ordinal}", request.request_id),
            request: request.clone(),
            ordinal: attempt_ordinal,
            credential: credential.clone(),
            credential_version: version.clone(),
            agent_assignment: assignment.as_ref().map(|h| h.reference.clone()),
        });
        funnel.trace(TraceEvent::AttemptStarted(&attempt));
        let channel_state: Arc<dyn gproxy_channel::channel::ChannelState> =
            Arc::new(crate::ChannelStateStore::new(
                ProtocolState::new(core, limits.capability(budget)),
                &provider.entity.id,
                &credential.id,
            ));

        let this_wire = if ordinal < attempts {
            match prepare::clone_request(wire.as_ref().expect("request present")) {
                Some(cloned) => cloned,
                None => wire.take().expect("request present"),
            }
        } else {
            wire.take().expect("request present")
        };
        let capability = limits.capability(budget);
        let cancellation = request.cancellation.clone();
        let dispatched: Result<Answer, Fault> = match route {
            Route::Passthrough | Route::Local => {
                let mut this_wire = this_wire;
                prepare::drop_thinking_handles(operation, &mut this_wire);
                let native = super::NativeCall {
                    funnel: funnel.clone(),
                    attempt: attempt.clone(),
                    operation,
                    response_rules: response_rules.body.clone(),
                    limits: capability,
                    state: channel_state.clone(),
                    instance_id: core.instance_id().clone(),
                };
                let sent = tokio::select! {
                    biased;
                    () = cancellation.cancelled() => Err(Fault::Cancelled),
                    result = crate::rt::timeout(capability.operation_total, native.send(this_wire)) => match result {
                        Some(Ok(response)) => Ok(response),
                        Some(Err(error @ gproxy_channel::ChannelError::InvalidConfig(_))) => {
                            Err(Fault::Client(CoreError::Channel(error)))
                        }
                        Some(Err(error)) => Err(Fault::Failed(CoreError::Channel(error))),
                        None => Err(Fault::DeadlineExceeded),
                    },
                };
                match sent {
                    Ok(mut response) => {
                        if !response_rules.headers.is_empty()
                            && let Err(error) =
                                apply_headers(&response_rules.headers, &mut response.headers)
                        {
                            Err(Fault::Client(error.into()))
                        } else {
                            Ok(Answer::new(response, Some(attempt.clone())))
                        }
                    }
                    Err(fault) => Err(fault),
                }
            }
            Route::TransformTo { target } => {
                let upstream = target.dialect;
                let upstream_host = AttemptUpstream::new(
                    funnel.clone(),
                    attempt.clone(),
                    inbound_headers.clone(),
                    capability,
                    channel_state.clone(),
                    core.instance_id().clone(),
                );
                let state_store = ProtocolState::new(core, capability);
                let scope = StateScope {
                    scope: request.scope.clone(),
                    provider_id: provider.entity.id.clone(),
                    conversation: request
                        .session
                        .as_ref()
                        .filter(|s| s.is_stable())
                        .map(|s| s.id.clone()),
                };
                let conversation_key = scope
                    .conversation
                    .clone()
                    .unwrap_or_else(|| request.request_id.clone());
                let call = convert::Call {
                    core,
                    upstream: &upstream_host,
                    client: operation,
                    target: upstream,
                    model: upstream_model.as_deref(),
                    request: convert::ClientRequest::new(&this_wire),
                    limits: limits.codec(),
                    state_store: &state_store,
                    state_scope: &scope,
                    conversation_key: &conversation_key,
                    provider_id: &provider.entity.id,
                    now_ms: now,
                    collect: operation.operation == gproxy_protocol::Operation::GenerateContent
                        && target.operation == gproxy_protocol::Operation::StreamGenerateContent,
                    synthesize: operation.operation
                        == gproxy_protocol::Operation::StreamGenerateContent
                        && target.operation == gproxy_protocol::Operation::GenerateContent,
                };
                let converted = tokio::select! {
                    biased;
                    () = cancellation.cancelled() => Err(Fault::Cancelled),
                    result = crate::rt::timeout(capability.operation_total, convert::dispatch(&call)) => match result {
                        Some(Ok(converted)) => Ok(converted),
                        Some(Err(error)) => Err(match crate::owned::not_owned_in(&error) {
                            // A file the conversion had to read is not the
                            // caller's: the same 404 the pre-dispatch check
                            // gives, and no fault of the credential.
                            Some(not_owned) => Fault::Client(not_owned.clone().into()),
                            None => match error.kind() {
                                TransformErrorKind::Host => Fault::Failed(error.into()),
                                _ => Fault::Client(error.into()),
                            },
                        }),
                        None => Err(Fault::DeadlineExceeded),
                    },
                };
                converted.map(|converted| match converted {
                    convert::Converted::Success(response)
                    | convert::Converted::Rejected(response) => {
                        Answer::new(response, Some(attempt.clone()))
                    }
                    convert::Converted::Stream(response) => Answer::new(
                        WireResponse {
                            status: response.status,
                            headers: response.headers,
                            body: HttpBody::Stream(response.body),
                        },
                        Some(attempt.clone()),
                    ),
                })
            }
            Route::Unsupported => unreachable!("unsupported route rejected before attempting"),
        };
        let finished_at = now_ms();
        let mut answer = match dispatched {
            Ok(answer) => answer,
            Err(Fault::Cancelled) | Err(Fault::DeadlineExceeded) => {
                let cancelled = request.cancellation.is_cancelled();
                let outcome = if cancelled {
                    AttemptOutcome::Cancelled
                } else {
                    AttemptOutcome::DeadlineExceeded
                };
                funnel.trace(TraceEvent::AttemptFinished {
                    attempt: &attempt,
                    outcome: &outcome,
                    finished_at_ms: finished_at,
                });
                let _ = settle(
                    core,
                    &mut assignment,
                    failed(true, if cancelled { "cancelled" } else { "deadline" }),
                    finished_at,
                )
                .await;
                return Err(if cancelled {
                    CoreError::Cancelled
                } else {
                    CoreError::DeadlineExceeded
                });
            }
            Err(Fault::Client(error)) => {
                let outcome = AttemptOutcome::Failed(
                    gproxy_channel::ChannelError::InvalidResponse(error.to_string()),
                );
                funnel.trace(TraceEvent::AttemptFinished {
                    attempt: &attempt,
                    outcome: &outcome,
                    finished_at_ms: finished_at,
                });
                let _ = settle(core, &mut assignment, failed(false, "client"), finished_at).await;
                return Err(error);
            }
            Err(Fault::Failed(CoreError::Channel(
                gproxy_channel::ChannelError::ContinuationElsewhere { instance_id },
            ))) => {
                // Not this credential's fault and not retryable here: the
                // caller has to reach the process holding the continuation.
                let outcome =
                    AttemptOutcome::Failed(gproxy_channel::ChannelError::ContinuationElsewhere {
                        instance_id: instance_id.clone(),
                    });
                funnel.trace(TraceEvent::AttemptFinished {
                    attempt: &attempt,
                    outcome: &outcome,
                    finished_at_ms: finished_at,
                });
                settle(
                    core,
                    &mut assignment,
                    failed(false, "continuation elsewhere"),
                    finished_at,
                )
                .await?;
                return Err(CoreError::ContinuationElsewhere { instance_id });
            }
            Err(Fault::Failed(error)) => {
                let outcome = AttemptOutcome::Failed(match &error {
                    CoreError::Channel(channel) => {
                        gproxy_channel::ChannelError::InvalidResponse(channel.to_string())
                    }
                    other => gproxy_channel::ChannelError::InvalidResponse(other.to_string()),
                });
                funnel.trace(TraceEvent::AttemptFinished {
                    attempt: &attempt,
                    outcome: &outcome,
                    finished_at_ms: finished_at,
                });
                settle(
                    core,
                    &mut assignment,
                    failed(true, "transport"),
                    finished_at,
                )
                .await?;
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
                if !retry_transport || ordinal >= attempts || wire.is_none() {
                    return Err(error);
                }
                continue;
            }
        };

        // Read enough of the answer to tell what it says about the credential:
        // a refusal's body prefix, or the first frame of a 2xx event stream,
        // which can still be an error. Both are replayed to the caller.
        let mut status = answer.status();
        let mut refusal = None;
        if status.is_success() {
            if is_event_stream(answer.headers())
                && let Some(found) =
                    super::rejection::first_frame_error(&mut answer.response.body, limits.codec())
                        .await
            {
                status = found.status;
                answer.abandon = true;
                refusal = Some(found.payload);
            }
        } else {
            refusal = Some(super::rejection::body_prefix(&mut answer.response.body).await);
        }
        let rejection = refusal.as_ref().and_then(|body| {
            provider.channel.classify_rejection(RejectionContext {
                status,
                headers: answer.headers(),
                body,
                operation,
                upstream_model: upstream_model.as_deref(),
                now_ms: finished_at,
            })
        });
        let refreshable = crate::refresh::can_refresh(&provider, &credential, &version);
        let classified = classify(
            status,
            rejection.as_ref(),
            refreshable && ordinal < attempts && wire.is_some(),
            refreshed.contains(&credential.id),
        );

        // Every answer's headers may carry account quota; exhaustion recorded
        // here replaces the generic rate-limit block below. A final answer is
        // not retried, so its observation need not land before it goes out.
        let quota_blocks = core
            .observe_answer_headers(
                &credential,
                operation,
                upstream_model.as_deref(),
                answer.status(),
                answer.headers(),
                finished_at,
                matches!(classified, Classified::Final).then_some(&*funnel),
            )
            .await
            .unwrap_or_default();
        match classified {
            Classified::Final => {
                let outcome = AttemptOutcome::Succeeded { status };
                funnel.trace(TraceEvent::AttemptFinished {
                    attempt: &attempt,
                    outcome: &outcome,
                    finished_at_ms: finished_at,
                });
                if status.is_success() {
                    core.remember_realtime_call(
                        &request,
                        &credential.id,
                        &mut answer.response,
                        limits.codec(),
                    )
                    .await?;
                    core.settle_owned_resource(
                        &request,
                        &credential.id,
                        &resource_path,
                        converting,
                        &mut answer.response,
                        limits.codec(),
                    )
                    .await?;
                    core.record_success(
                        &credential.provider_id,
                        &credential.id,
                        upstream_model.as_deref(),
                        operation.operation,
                        finished_at,
                    )
                    .await?;
                    // Best effort: the upstream has answered and been paid for, so a
                    // cache that cannot take the pin costs the next request its
                    // affinity, not this one its answer.
                    if let Err(error) = core
                        .pin_affinity(&request, &credential.id, finished_at)
                        .await
                    {
                        tracing::warn!(%error, credential = %credential.id, "session affinity not pinned");
                    }
                }
                // The upstream answered on this credential: the target stands,
                // whatever it said about the request itself.
                settle(
                    core,
                    &mut assignment,
                    AssignmentOutcome::Activated,
                    finished_at,
                )
                .await?;
                return Ok(answer);
            }
            Classified::Refresh => {
                refreshed.insert(credential.id.clone());
                match core
                    .refresh_credential(
                        &credential.provider_id,
                        &credential.id,
                        crate::RefreshMode::UnauthorizedRecovery,
                    )
                    .await
                {
                    Ok(_) => {
                        let outcome = AttemptOutcome::Rejected {
                            status,
                            retry_after: None,
                        };
                        funnel.trace(TraceEvent::AttemptFinished {
                            attempt: &attempt,
                            outcome: &outcome,
                            finished_at_ms: finished_at,
                        });
                        answer
                            .discard(limits.capability(remaining(&request)).operation_total)
                            .await;
                        // Same credential again with fresh material: the
                        // reservation is still being prepared.
                        carried = assignment.take();
                        continue;
                    }
                    Err(_) => {
                        settle(core, &mut assignment, failed(false, "refresh"), finished_at)
                            .await?;
                        // Refresh failed: this credential is out for the request.
                        exclude(
                            core,
                            &funnel,
                            &attempt,
                            &credential,
                            upstream_model.as_deref(),
                            operation.operation,
                            status,
                            None,
                            finished_at,
                        )
                        .await?;
                        excluded.insert(credential.id.clone());
                        held = Some(answer);
                    }
                }
            }
            Classified::Exclude => {
                // Account quota read from the headers replaces the block the
                // refusal itself would earn.
                if quota_blocks.is_empty() {
                    let block = rejection_block(
                        status,
                        rejection.as_ref(),
                        answer.headers(),
                        upstream_model.as_deref(),
                        operation.operation,
                        finished_at,
                    );
                    exclude(
                        core,
                        &funnel,
                        &attempt,
                        &credential,
                        upstream_model.as_deref(),
                        operation.operation,
                        status,
                        block,
                        finished_at,
                    )
                    .await?;
                } else {
                    let outcome = AttemptOutcome::Rejected {
                        status,
                        retry_after: retry_after_ms(answer.headers(), finished_at)
                            .map(|ms| std::time::Duration::from_millis(ms as u64)),
                    };
                    funnel.trace(TraceEvent::AttemptFinished {
                        attempt: &attempt,
                        outcome: &outcome,
                        finished_at_ms: finished_at,
                    });
                }
                settle(
                    core,
                    &mut assignment,
                    failed(status.is_server_error(), status.as_str()),
                    finished_at,
                )
                .await?;
                // Whatever its streak says across requests, this request does
                // not ask a credential that just refused it again.
                excluded.insert(credential.id.clone());
                if ordinal >= attempts || wire.is_none() {
                    // Budget spent: the last upstream answer is the answer.
                    return Ok(answer);
                }
                held = Some(answer);
            }
        }
    }
    if let Some(answer) = held.take() {
        return Ok(answer);
    }
    Err(CoreError::NoUsableCredential {
        retry_after_ms: None,
    })
}

/// Caller budgets are checked once, before any credential is touched: an
/// exhausted one fails the request through the funnel so the Observer sees
/// a failed usage and, with tracing on, the rejection itself.
pub(crate) async fn reject_when_over_budget<C: BatchConnectionTrait>(
    core: &Core<C>,
    request: &Arc<RequestContext>,
    funnel: &Arc<Funnel>,
) -> CoreResult<()> {
    if !request.snapshot.observation.settlement {
        return Ok(());
    }
    match core.check_budgets(request, now_ms()).await {
        Ok(()) => Ok(()),
        Err(error) => {
            if let CoreError::BudgetExhausted {
                quota_id,
                window_key,
                resets_at_ms,
            } = &error
            {
                funnel.trace(TraceEvent::BudgetRejected {
                    request,
                    quota_id,
                    window_key,
                    resets_at_ms: *resets_at_ms,
                });
            }
            Err(error)
        }
    }
}

/// Trace the rejection and write the block or bump the streak.
#[allow(clippy::too_many_arguments)]
async fn exclude<C: BatchConnectionTrait>(
    core: &Core<C>,
    funnel: &Funnel,
    attempt: &Arc<AttemptContext>,
    credential: &crate::CredentialData,
    upstream_model: Option<&str>,
    operation: gproxy_protocol::Operation,
    status: StatusCode,
    block: Option<CredentialBlock>,
    now_ms: i64,
) -> CoreResult<()> {
    let outcome = AttemptOutcome::Rejected {
        status,
        retry_after: block
            .as_ref()
            .map(|b| Duration::from_millis((b.until_ms - now_ms).max(0) as u64)),
    };
    funnel.trace(TraceEvent::AttemptFinished {
        attempt,
        outcome: &outcome,
        finished_at_ms: now_ms,
    });
    match block {
        Some(block) => {
            core.record_block(&credential.provider_id, &credential.id, block, now_ms)
                .await?;
        }
        None => {
            core.record_failure(
                &credential.provider_id,
                &credential.id,
                upstream_model,
                operation,
                now_ms,
            )
            .await?;
        }
    }
    Ok(())
}

fn is_event_stream(headers: &http::HeaderMap) -> bool {
    headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.split(';').next())
        .is_some_and(|value| value.trim() == "text/event-stream")
}

fn failed(uncertain: bool, error: &str) -> AssignmentOutcome {
    AssignmentOutcome::Failed {
        uncertain,
        error: error.to_owned(),
    }
}

/// Settle the attempt's assignment once; later calls find nothing to settle.
async fn settle<C: BatchConnectionTrait + Send + Sync>(
    core: &Core<C>,
    assignment: &mut Option<AssignmentHandle>,
    outcome: AssignmentOutcome,
    now_ms: i64,
) -> CoreResult<()> {
    if let Some(handle) = assignment.take() {
        core.settle_assignment(&handle, outcome, now_ms).await?;
    }
    Ok(())
}
