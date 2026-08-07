//! Upstream failover loop (§6.4) — the ONE place candidates are iterated and
//! `Channel::classify` is called. Stream & non-stream share the loop and differ
//! only at the body tail (D4). M2: the transform plan (passthrough vs
//! cross-protocol) is resolved PER candidate, before `prepare`.
//! M7a: a lazy pre-use refresh (refresh only if the channel says the secret is
//! stale) sits before each attempt, and an AuthDead classification triggers a
//! single forced refresh + replay of the SAME candidate. A per-request retry
//! budget caps the number of candidate attempts. The single-attempt mechanics
//! (prepare → send → classify, body materialization) live in the internal `attempt` module.

mod attempt;
mod budget;
mod fallback;
mod telemetry;

pub use attempt::BodySource;
use attempt::{
    AttemptOutcome, Materialized, ResponseRuleCtx, UpstreamRespCapture, attempt, materialize,
    refresh_failed,
};

use crate::app::AppState;
use crate::channel::Disposition;
use crate::health::CredAdmit;
use crate::health::config::breaker_config;
use crate::pipeline::balance;
use crate::pipeline::capture;
use crate::pipeline::context::{Candidate, RequestCtx};
use crate::pipeline::error::PipelineError;
use crate::pipeline::health_hooks;
use crate::pipeline::local_ops;
use crate::pipeline::outcome::{ExecOutcome, ResponseBody};
use crate::pipeline::settle;
use crate::pipeline::transform::{self as transform_step, AttemptMemo, TransformPlan};
use crate::protocol::OperationKind;

fn sanitize_public_upstream_headers(ctx: &RequestCtx, headers: &mut http::HeaderMap) {
    if !matches!(
        ctx.mode,
        crate::pipeline::context::RoutingMode::Aggregated
            | crate::pipeline::context::RoutingMode::Namespace { .. }
    ) {
        return;
    }
    let remove: Vec<_> = headers
        .keys()
        .filter(|name| {
            let name = name.as_str();
            name.starts_with("x-codex-") && name != "x-codex-turn-state"
        })
        .cloned()
        .collect();
    for name in remove {
        headers.remove(name);
    }
}

/// Iterate candidates until one succeeds or returns a permanent error. The
/// channel AND the transform plan are resolved PER candidate (a route's members
/// may span providers / channels / wire protocols).
pub async fn run_failover(
    state: &AppState,
    ctx: &RequestCtx,
    candidates: &[Candidate],
) -> Result<ExecOutcome, PipelineError> {
    let mut last_err: Option<PipelineError> = None;
    let mut memo = AttemptMemo::default();
    // Credentials force-refreshed once this request after an AuthDead — guards
    // against an infinite refresh→retry loop (each cred gets ONE forced refresh).
    let mut refreshed_creds: std::collections::HashSet<i64> = std::collections::HashSet::new();
    // §6.4 per-request failover budget: bounds fan-out on a large unhealthy
    // pool. Counts candidate ATTEMPTS; the AuthDead forced-refresh retry is the
    // SAME logical candidate and does NOT increment this. A route with more
    // candidates than the budget stops early and returns `last_err`.
    let max_attempts = state.config.max_attempts;
    let mut attempts: u32 = 0;

    for (candidate_index, cand) in candidates.iter().enumerate() {
        if attempts >= max_attempts {
            tracing::warn!(
                attempt = attempts,
                max_attempts,
                "failover budget exhausted; stopping with last error"
            );
            break;
        }

        let Some(channel) = state.channels.get(&cand.provider.channel) else {
            last_err = Some(PipelineError::UnknownChannel(cand.provider.channel.clone()));
            continue;
        };

        // M2 dispatch decision per candidate. The snapshot guard is scoped to
        // this lookup only — never held across the upstream call.
        let (plan, rules) = {
            let cp = state.cp();
            let plan = match transform_step::plan_for(
                &cp,
                cand.provider.id,
                ctx.op.expect("classified"),
            ) {
                // `local` is intentional config addressed to this request —
                // serve it here, never shop for another candidate.
                Ok(TransformPlan::Local) => {
                    return match local_ops::serve_local(state, &cp, ctx, cand) {
                        Some(o) => Ok(o),
                        None => Err(PipelineError::LocalUnimplemented),
                    };
                }
                Ok(p) => p,
                // unsupported / no-pair: this candidate can't serve it; next.
                Err(e) => {
                    last_err = Some(e);
                    continue;
                }
            };
            let rules = cp.rule_sets_by_provider.get(&cand.provider.id).cloned();
            (plan, rules)
        };

        // §3.3 per-credential rpm/tpm budget — a budget skip is not a health
        // failure (the key is fine, just busy this minute). It is also not a
        // candidate ATTEMPT (no upstream call), so the retry budget is untouched.
        if budget::exhausted(state, cand).await {
            last_err = Some(PipelineError::Transport(
                "credential rpm budget exhausted".into(),
            ));
            continue;
        }

        // Candidate planning only peeks at health. Acquire the exact
        // credential-model admission immediately before doing credential work,
        // so a failure earlier in this same failover loop takes effect now.
        let cred_cfg = breaker_config(&cand.provider.settings_json);
        if state.health.admit_credential_model(
            cand.credential.id,
            &cand.upstream_model_id,
            &cred_cfg,
            crate::util::time::unix_now(),
        ) == CredAdmit::No
        {
            continue;
        }

        // §14.1 decrypt-at-use: the snapshot carries sealed secrets; open per
        // attempt (µs-scale). Unreadable secret → skip candidate, not 500.
        let opened = match state.cipher.open(&cand.credential.secret_json) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(
                    credential_id = cand.credential.id,
                    error = %e,
                    "secret open failed; skipping credential"
                );
                last_err = Some(PipelineError::Channel(
                    crate::channel::ChannelError::InvalidCredential(
                        "sealed secret unreadable".into(),
                    ),
                ));
                continue;
            }
        };

        // §14.5 lazy pre-use refresh: refresh ONLY if this channel says the
        // opened secret is stale; otherwise returns it unchanged. A refresh
        // failure is treated like an unreadable secret — cool + audit + skip.
        let mut secret = match state
            .ensure_fresh_credential(&channel, &cand.credential, &cand.provider, opened, false)
            .await
        {
            Ok(v) => v,
            Err(e) => {
                refresh_failed(state, ctx, cand, &e).await;
                last_err = Some(PipelineError::Channel(e));
                continue;
            }
        };

        // One candidate attempt (counts against the budget).
        attempts += 1;
        let has_next = candidate_index + 1 < candidates.len() && attempts < max_attempts;
        let outcome = match attempt(
            state,
            ctx,
            cand,
            &channel,
            &secret,
            &plan,
            rules.as_deref().map(Vec::as_slice),
            &mut memo,
        )
        .await
        {
            Ok(o) => o,
            Err(e) => {
                let error = e.to_string();
                telemetry::attempt_error(ctx, cand, attempts, max_attempts, &error, has_next);
                last_err = Some(e);
                continue;
            }
        };

        // §14.5 AuthDead-triggered forced refresh + retry-once. The lazy gate
        // above said "fresh", yet upstream rejected the auth — so the channel's
        // staleness view is wrong (clock skew, server-side revocation). Force a
        // refresh ONCE per credential and replay the SAME candidate. The retry
        // does NOT consume the retry budget (same logical candidate).
        let outcome = if outcome.disposition == Disposition::AuthDead
            && cand.credential.kind != "api_key"
            && refreshed_creds.insert(cand.credential.id)
        {
            let error = format!("upstream {} ({:?})", outcome.status, outcome.disposition);
            telemetry::forced_refresh_retry(ctx, cand, attempts, max_attempts, &outcome, &error);
            match state
                .ensure_fresh_credential(
                    &channel,
                    &cand.credential,
                    &cand.provider,
                    secret.clone(),
                    true,
                )
                .await
            {
                Ok(fresh) => {
                    secret = fresh;
                    match attempt(
                        state,
                        ctx,
                        cand,
                        &channel,
                        &secret,
                        &plan,
                        rules.as_deref().map(Vec::as_slice),
                        &mut memo,
                    )
                    .await
                    {
                        Ok(o) => o,
                        Err(e) => {
                            let error = e.to_string();
                            telemetry::attempt_error(
                                ctx,
                                cand,
                                attempts,
                                max_attempts,
                                &error,
                                has_next,
                            );
                            last_err = Some(e);
                            continue;
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        credential_id = cand.credential.id,
                        error = %e,
                        "forced refresh after AuthDead failed; cooling credential"
                    );
                    refresh_failed(state, ctx, cand, &e).await;
                    let error = e.to_string();
                    telemetry::outcome_failed(
                        ctx,
                        cand,
                        (attempts, max_attempts),
                        &outcome,
                        &error,
                        has_next,
                        false,
                    );
                    last_err = Some(PipelineError::Channel(e));
                    continue;
                }
            }
        } else {
            outcome
        };
        health_hooks::record_attempt(
            state,
            &ctx.request_id,
            cand,
            &outcome.disposition,
            outcome.send_ms,
        )
        .await;
        if !outcome.disposition.should_failover() {
            telemetry::selected(
                ctx,
                cand,
                attempts,
                max_attempts,
                outcome.status.as_u16(),
                &outcome.disposition,
                outcome.send_ms.unwrap_or(0.0),
            );
            // Success, or a Permanent 4xx the client should see — return it.
            // §17: billing context is captured BEFORE the body is handed out
            // (content-generation successes only; capture needs no snapshot
            // at settle time — pricing is resolved here).
            let AttemptOutcome {
                status,
                mut headers,
                source,
                sent_body,
                count_body,
                disposition,
                send_ms,
                sent_url,
                method,
                sent_headers,
                multi_step,
            } = outcome;
            let latency_ms = send_ms.map(|ms| ms as i64).unwrap_or(0);
            if status.is_success() {
                balance::record_response_affinity(state.cache.as_ref(), ctx, &headers, cand).await;
            }
            // A direct successful stream needs its row id before its body is
            // exposed. Custom streams own their exact per-call guards inside
            // `CapturingClient`; buffered responses are folded into the later
            // INSERT as before.
            let direct_stream_capture = {
                let ls = state.cp().log_settings.clone();
                !multi_step
                    && matches!(&source, BodySource::Streaming(_))
                    && ls.enable_upstream_log
                    && ls.enable_upstream_log_body
            };
            let up_cap = if direct_stream_capture {
                capture::start_upstream_stream(
                    state,
                    ctx,
                    cand,
                    capture::UpstreamWire {
                        status,
                        latency_ms,
                        url: &sent_url,
                        method: &method,
                        sent_headers: sent_headers.as_ref(),
                        sent_body: &sent_body,
                        resp_body: None,
                    },
                )
                .await
                .map(|capture_id| UpstreamRespCapture {
                    state: state.clone(),
                    capture_id,
                })
            } else {
                None
            };
            // The attempt reached the provider and is being relayed; log its
            // upstream row EVEN IF response materialization fails afterwards
            // (a 2xx whose cross-protocol transform errors must still leave an
            // upstream trace). Borrow `upstream_raw` from the Ok arm; `?` below
            // propagates a materialize error only after the row is written.
            let rule_filter_model = memo
                .inbound_model(ctx)
                .or_else(|| crate::pipeline::classify::path_model_id(&ctx.path))
                .unwrap_or_else(|| cand.upstream_model_id.clone());
            let response_rules = rules.as_deref().map(|rules| ResponseRuleCtx {
                rules: rules.as_slice(),
                model: rule_filter_model.as_str(),
            });
            let shaped_op = plan.shape_op(ctx);
            let usage_kind = match shaped_op.kind() {
                OperationKind::ContentGeneration(k) => Some(k),
                OperationKind::Provider(_) => None,
                _ => unreachable!(
                    "new non-exhaustive protocol variant requires a lockstep transform update"
                ),
            };
            let settle_ctx = status
                .is_success()
                .then(|| {
                    usage_kind.and_then(|kind| {
                        settle::SettleCtx::capture(
                            state,
                            ctx,
                            cand,
                            &channel,
                            kind,
                            count_body.clone(),
                            latency_ms,
                        )
                    })
                })
                .flatten();
            let mat = materialize(
                &channel,
                source,
                &plan,
                ctx,
                status,
                &cand.provider.settings_json,
                response_rules,
                up_cap,
                settle_ctx,
            )
            .await;
            // §8-D upstream capture: the attempt actually returned to the
            // client (success or relayed permanent 4xx). Failed-over attempts
            // were audited above; gating happens inside `capture`. `resp_body`
            // is the non-streaming provider response (streams backfill via guard).
            // A `multi_step` (Custom) exchange already logged each of its calls
            // inline via the `CapturingClient`, so there is no single aggregate
            // row to write here — skip it (its `sent_url` is empty anyway).
            if !multi_step && !direct_stream_capture {
                capture::log_upstream(
                    state,
                    ctx,
                    cand,
                    capture::UpstreamWire {
                        status,
                        latency_ms,
                        url: &sent_url,
                        method: &method,
                        sent_headers: sent_headers.as_ref(),
                        sent_body: &sent_body,
                        resp_body: mat.as_ref().ok().and_then(|m| m.upstream_raw.as_ref()),
                    },
                )
                .await;
            }
            let Materialized {
                body,
                settle,
                provider_settle,
                ..
            } = mat?;
            if let Some(s) = settle {
                // §17: settle (usage extract + reconcile + usage INSERT) must
                // not delay the client-visible response — detach it, mirroring
                // the streaming StreamGuard. wasm has no detached tasks, so the
                // inline await is the platform cost there.
                #[cfg(not(target_arch = "wasm32"))]
                tokio::spawn(async move { settle::settle_body(s.ctx, &s.body, s.stream).await });
                #[cfg(target_arch = "wasm32")]
                settle::settle_body(s.ctx, &s.body, s.stream).await;
            }
            if plan.is_transform() || plan.is_aggregate_stream() {
                // converted bytes no longer match the upstream framing
                headers.remove(http::header::CONTENT_LENGTH);
            }
            if status.is_success()
                && plan.is_aggregate_stream()
                && matches!(&body, ResponseBody::Full(_))
            {
                // The upstream event stream has been collapsed into one JSON
                // object, so its original media type no longer describes the body.
                headers.insert(
                    http::header::CONTENT_TYPE,
                    http::HeaderValue::from_static("application/json"),
                );
            }
            #[cfg(not(target_arch = "wasm32"))]
            let body = match (status.is_success(), ctx.op.map(|op| op.kind()), body) {
                (
                    true,
                    Some(OperationKind::ContentGeneration(inbound)),
                    ResponseBody::Stream(s),
                ) => ResponseBody::Stream(crate::pipeline::stream::instrument_error_frame(
                    s, inbound,
                )),
                (_, _, body) => body,
            };
            let provider_usage = provider_settle
                .map(|settle| (settle.body, settle.family))
                .or_else(|| {
                    if !settle::provider::billable(ctx.op) {
                        return None;
                    }
                    let ResponseBody::Full(body) = &body else {
                        return None;
                    };
                    let op = ctx.op?;
                    let family = match op.kind() {
                        OperationKind::ContentGeneration(kind) => kind.provider(),
                        OperationKind::Provider(family) => family,
                    _ => unreachable!("new non-exhaustive protocol variant requires a lockstep transform update"),
};
                    Some((body.clone(), family))
                });
            if status.is_success()
                && settle::provider::billable(ctx.op)
                && let Some((provider_body, usage_family)) = provider_usage
            {
                settle::provider::schedule(state, ctx, cand, provider_body, usage_family).await;
            }
            sanitize_public_upstream_headers(ctx, &mut headers);
            return Ok(ExecOutcome {
                status,
                headers,
                body,
                disposition,
            });
        }

        // AuthDead / RateLimited / Transient → drop this attempt, try the next.
        let error = format!("upstream {} ({:?})", outcome.status, outcome.disposition);
        telemetry::outcome_failed(
            ctx,
            cand,
            (attempts, max_attempts),
            &outcome,
            &error,
            has_next,
            true,
        );
        settle::audit_failure(
            state,
            &ctx.request_id,
            cand,
            settle::FailedAttempt {
                url: &outcome.sent_url,
                method: outcome.method.as_str(),
                status: i64::from(outcome.status.as_u16()),
                latency_ms: outcome.send_ms.map(|ms| ms as i64).unwrap_or(0),
                error: &error,
            },
        )
        .await;
        last_err = Some(PipelineError::Transport(error));
    }

    fallback::finish(state, ctx, candidates, last_err, attempts, max_attempts)
}
