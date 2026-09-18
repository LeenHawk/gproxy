//! The attempt loop for HTTP operations: select, prepare, dispatch, classify,
//! and either return the response through the funnel or move to the next
//! eligible credential inside the permitted set.

use super::{Exchange, Funnel, ObservedClient, prepare};
use crate::{
    AttemptContext, AttemptOutcome, BlockSource, Core, CoreError, CoreResult, CredentialBlock,
    Execution, HttpExecution, RequestContext, TraceEvent, UsageState,
    api::lifecycle::now_ms,
    availability::{DEFAULT_RATE_LIMIT_MS, retry_after_ms},
    rewrite::{Phase, RewriteContext, apply_body, apply_headers, apply_query, select_rules},
};
use gproxy_channel::ChannelBinding;
use gproxy_protocol::{HttpBody, WireRequest, WireResponse, connection::Bytes};
use gproxy_seaorm::BatchConnectionTrait;
use gproxy_store::entity::upstream::operation_endpoint::EndpointTransport;
use http::StatusCode;
use std::{collections::HashSet, sync::Arc, time::Duration};
use web_time::Instant;

fn remaining(request: &RequestContext) -> Option<Duration> {
    request
        .deadline
        .map(|deadline| deadline.saturating_duration_since(Instant::now()))
}

enum Classified {
    /// Returned to the caller as-is: success, or a client-side 4xx.
    Final,
    /// The credential is unusable for now; move on if budget remains.
    Exclude,
    /// Try the same credential once more after a forced refresh.
    Refresh,
}

fn classify(status: StatusCode, refreshable: bool, refreshed: bool) -> Classified {
    match status.as_u16() {
        401 | 403 if refreshable && !refreshed => Classified::Refresh,
        429 | 401 | 403 | 500..=599 => Classified::Exclude,
        _ => Classified::Final,
    }
}

pub(crate) async fn run_http<C: BatchConnectionTrait>(
    core: &Core<C>,
    request: Arc<RequestContext>,
    wire: WireRequest<HttpBody>,
) -> CoreResult<HttpExecution> {
    let (funnel, completion) = Funnel::new(request.clone(), core.observer().clone());
    let snapshot = request.snapshot.clone();
    let limits = snapshot.limits;
    let provider = request.target.provider.clone();
    let operation = request.operation;
    let upstream_model = request.target.upstream_model.clone();

    let inbound_headers = wire.headers.clone();
    let rewrite_context = RewriteContext {
        operation,
        upstream_model: upstream_model.as_deref(),
        requested_model: None,
        request_headers: &inbound_headers,
    };
    let request_rules = select_rules(&snapshot, &provider, Phase::Request, &rewrite_context);
    let response_rules = select_rules(&snapshot, &provider, Phase::Response, &rewrite_context);

    let want_replay = request.max_attempts.get() > 1 || !request_rules.body.is_empty();
    let (mut wire, replayable) =
        prepare::buffer_request(wire, want_replay, limits.max_request_body_bytes).await;
    // Request rules run once: they do not depend on the credential.
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
    let mut held: Option<(WireResponse<HttpBody>, Arc<Exchange>)> = None;
    while ordinal < attempts {
        ordinal += 1;
        let now = now_ms();
        if request.cancellation.is_cancelled() {
            funnel.finish(UsageState::Cancelled).await;
            return Err(CoreError::Cancelled);
        }
        let budget = remaining(&request);
        if budget.is_some_and(|left| left.is_zero()) {
            funnel.finish(UsageState::Failed).await;
            return Err(CoreError::DeadlineExceeded);
        }
        let selection = match core.select_credential(&request, &excluded, now).await {
            Ok(selection) => selection,
            Err(CoreError::NoUsableCredential) if held.is_some() => {
                let (response, exchange) = held.take().expect("held");
                exchange.make_terminal();
                let settled = funnel.arm();
                return Ok(Execution::new(response, completion, settled));
            }
            Err(error) => {
                drop(held.take());
                funnel.finish(UsageState::Failed).await;
                return Err(error);
            }
        };
        // A new attempt supersedes the previous rejected answer.
        drop(held.take());
        let credential = selection.credential;
        let version = credential.state.load();
        let attempt = Arc::new(AttemptContext {
            attempt_id: format!("{}-{ordinal}", request.request_id),
            request: request.clone(),
            ordinal,
            credential: credential.clone(),
            credential_version: version.clone(),
            agent_assignment: None,
        });
        funnel.trace(TraceEvent::AttemptStarted(&attempt));

        let this_wire = if ordinal < attempts {
            match prepare::clone_request(wire.as_ref().expect("request present")) {
                Some(cloned) => cloned,
                None => wire.take().expect("request present"),
            }
        } else {
            wire.take().expect("request present")
        };
        let exchange = Exchange::new(
            funnel.clone(),
            attempt.clone(),
            operation,
            provider.channel.clone(),
            response_rules.body.clone(),
            limits.capability(budget),
            now,
        );
        let observed = ObservedClient::new(credential.client.as_ref(), exchange.clone());
        let endpoint = provider.operation_url(operation, EndpointTransport::Http);
        let binding = ChannelBinding::new(
            provider.channel.as_ref(),
            prepare::provider_view(&provider),
            prepare::credential_view(&credential, &version),
            &observed,
        )
        .endpoint(endpoint);
        let cancellation = request.cancellation.clone();
        let sent = tokio::select! {
            biased;
            () = cancellation.cancelled() => Err(None),
            result = tokio::time::timeout(limits.capability(budget).operation_total, binding.send(operation, this_wire)) => match result {
                Ok(result) => result.map_err(Some),
                Err(_) => Err(None),
            },
        };
        let finished_at = now_ms();
        let mut response = match sent {
            Ok(response) => response,
            Err(None) => {
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
                exchange
                    .finish(
                        gproxy_channel::channel::UsageStreamEnd::Interrupted,
                        None,
                        None,
                        None,
                        finished_at,
                    )
                    .await;
                funnel
                    .finish(if cancelled {
                        UsageState::Cancelled
                    } else {
                        UsageState::Failed
                    })
                    .await;
                return Err(if cancelled {
                    CoreError::Cancelled
                } else {
                    CoreError::DeadlineExceeded
                });
            }
            Err(Some(error)) => {
                let outcome = AttemptOutcome::Failed(error);
                funnel.trace(TraceEvent::AttemptFinished {
                    attempt: &attempt,
                    outcome: &outcome,
                    finished_at_ms: finished_at,
                });
                exchange
                    .finish(
                        gproxy_channel::channel::UsageStreamEnd::Interrupted,
                        None,
                        None,
                        None,
                        finished_at,
                    )
                    .await;
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
                if ordinal >= attempts || wire.is_none() {
                    funnel.finish(UsageState::Failed).await;
                    return Err(CoreError::Channel(error));
                }
                continue;
            }
        };

        let refreshable = provider.channel.credential_refresh().is_some();
        match classify(
            response.status,
            refreshable,
            refreshed.contains(&credential.id),
        ) {
            Classified::Final => {
                let outcome = AttemptOutcome::Succeeded {
                    status: response.status,
                };
                funnel.trace(TraceEvent::AttemptFinished {
                    attempt: &attempt,
                    outcome: &outcome,
                    finished_at_ms: finished_at,
                });
                if response.status.is_success() {
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
                }
                if !response_rules.headers.is_empty() {
                    apply_headers(&response_rules.headers, &mut response.headers)?;
                }
                exchange.make_terminal();
                let settled = funnel.arm();
                return Ok(Execution::new(response, completion, settled));
            }
            Classified::Refresh => {
                refreshed.insert(credential.id.clone());
                drop(response);
                match core
                    .refresh_credential(
                        &credential.provider_id,
                        &credential.id,
                        crate::RefreshMode::Force,
                    )
                    .await
                {
                    Ok(_) => {
                        // Same credential, fresh material: do not count the
                        // attempt against the budget for a different credential.
                        let outcome = AttemptOutcome::Rejected {
                            status: StatusCode::UNAUTHORIZED,
                            retry_after: None,
                        };
                        funnel.trace(TraceEvent::AttemptFinished {
                            attempt: &attempt,
                            outcome: &outcome,
                            finished_at_ms: finished_at,
                        });
                        if wire.is_none() {
                            funnel.finish(UsageState::Failed).await;
                            return Err(CoreError::NoUsableCredential);
                        }
                        continue;
                    }
                    Err(_) => {
                        // Refresh failed: this credential is out for the request.
                        exclude(
                            core,
                            &funnel,
                            &attempt,
                            &credential,
                            upstream_model.as_deref(),
                            operation.operation,
                            StatusCode::UNAUTHORIZED,
                            None,
                            finished_at,
                        )
                        .await?;
                        excluded.insert(credential.id.clone());
                    }
                }
            }
            Classified::Exclude => {
                let status = response.status;
                let retry_after = retry_after_ms(&response.headers);
                let block = (status == StatusCode::TOO_MANY_REQUESTS).then(|| CredentialBlock {
                    scope: gproxy_channel::channel::QuotaScope::All,
                    operation: None,
                    until_ms: finished_at + retry_after.unwrap_or(DEFAULT_RATE_LIMIT_MS),
                    source: BlockSource::RateLimited,
                    observed_at_ms: finished_at,
                });
                if ordinal >= attempts || wire.is_none() {
                    // Budget spent: the last upstream answer is the answer.
                    let outcome = AttemptOutcome::Rejected {
                        status,
                        retry_after: retry_after.map(|ms| Duration::from_millis(ms as u64)),
                    };
                    funnel.trace(TraceEvent::AttemptFinished {
                        attempt: &attempt,
                        outcome: &outcome,
                        finished_at_ms: finished_at,
                    });
                    if let Some(block) = block {
                        core.record_block(
                            &credential.provider_id,
                            &credential.id,
                            block,
                            finished_at,
                        )
                        .await?;
                    } else {
                        core.record_failure(
                            &credential.provider_id,
                            &credential.id,
                            upstream_model.as_deref(),
                            operation.operation,
                            finished_at,
                        )
                        .await?;
                    }
                    exchange.make_terminal();
                    let settled = funnel.arm();
                    return Ok(Execution::new(response, completion, settled));
                }
                let blocked = exclude(
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
                if blocked {
                    excluded.insert(credential.id.clone());
                }
                held = Some((response, exchange));
            }
        }
    }
    if let Some((response, exchange)) = held.take() {
        exchange.make_terminal();
        let settled = funnel.arm();
        return Ok(Execution::new(response, completion, settled));
    }
    funnel.finish(UsageState::Failed).await;
    Err(CoreError::NoUsableCredential)
}

/// Trace the rejection and write the block or bump the streak. Returns whether
/// the credential is now blocked and must be excluded for this request.
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
) -> CoreResult<bool> {
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
            Ok(true)
        }
        None => Ok(core
            .record_failure(
                &credential.provider_id,
                &credential.id,
                upstream_model,
                operation,
                now_ms,
            )
            .await?
            .is_some()),
    }
}
