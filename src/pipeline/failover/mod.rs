//! Upstream failover loop (§6.4) — the ONE place candidates are iterated and
//! `Channel::classify` is called. Stream & non-stream share the loop and differ
//! only at the body tail (D4). M2: the transform plan (passthrough vs
//! cross-protocol) is resolved PER candidate, before `prepare`.
//!
//! M7a: a lazy pre-use refresh (refresh only if the channel says the secret is
//! stale) sits before each attempt, and an AuthDead classification triggers a
//! single forced refresh + replay of the SAME candidate. A per-request retry
//! budget caps the number of candidate attempts. The single-attempt mechanics
//! (prepare → send → classify, body materialization) live in [`attempt`].

mod attempt;

pub use attempt::BodySource;
use attempt::{
    AttemptOutcome, Materialized, UpstreamRespCapture, attempt, materialize, refresh_failed,
};

use std::time::Duration;

use crate::app::AppState;
use crate::channel::Disposition;
use crate::pipeline::capture;
use crate::pipeline::context::{Candidate, RequestCtx};
use crate::pipeline::error::PipelineError;
use crate::pipeline::health_hooks;
use crate::pipeline::local_ops;
use crate::pipeline::outcome::{ExecOutcome, ResponseBody};
use crate::pipeline::settle;
use crate::pipeline::transform::{self as transform_step, AttemptMemo, TransformPlan};
use crate::protocol::Operation;

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

    for cand in candidates {
        if attempts >= max_attempts {
            tracing::warn!(
                attempts,
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
        let (plan, rules, local_models) = {
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
            // §6.3 merged models: manual + variant rows join a successful
            // upstream list (additions captured while the guard is held).
            let local_models = (ctx.op.expect("classified").operation == Operation::ListModels)
                .then(|| {
                    cp.exposed_models_by_provider
                        .get(&cand.provider.id)
                        .cloned()
                })
                .flatten();
            (plan, rules, local_models)
        };

        // §3.3 per-credential rpm/tpm budget — a budget skip is not a health
        // failure (the key is fine, just busy this minute). It is also not a
        // candidate ATTEMPT (no upstream call), so the retry budget is untouched.
        if budget_exhausted(state, cand).await {
            last_err = Some(PipelineError::Transport(
                "credential rpm budget exhausted".into(),
            ));
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
            .refresh
            .ensure_fresh(
                state,
                &channel,
                &cand.credential,
                &cand.provider,
                opened,
                false,
            )
            .await
        {
            Ok(v) => v,
            Err(e) => {
                refresh_failed(state, ctx, cand, &e);
                last_err = Some(PipelineError::Channel(e));
                continue;
            }
        };

        // One candidate attempt (counts against the budget).
        attempts += 1;
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
            && refreshed_creds.insert(cand.credential.id)
        {
            match state
                .refresh
                .ensure_fresh(
                    state,
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
                        // Retry transport/prepare failure already audited + health
                        // recorded inside `attempt`; just advance.
                        Err(e) => {
                            last_err = Some(e);
                            continue;
                        }
                    }
                }
                // Forced refresh failed: cool + audit + next candidate. The
                // original AuthDead outcome's health is recorded below.
                Err(e) => {
                    tracing::warn!(
                        credential_id = cand.credential.id,
                        error = %e,
                        "forced refresh after AuthDead failed; cooling credential"
                    );
                    outcome
                }
            }
        } else {
            outcome
        };

        // §3.2/§16.3 disposition → health (+ edge-persisted credential edges),
        // recorded EXACTLY ONCE per logical candidate on the FINAL disposition.
        // A still-AuthDead final cools the credential 600s (health_hooks).
        health_hooks::record_attempt(state, cand, &outcome.disposition, outcome.send_ms);

        if !outcome.disposition.should_failover() {
            // Success, or a Permanent 4xx the client should see — return it.
            // §17: billing context is captured BEFORE the body is handed out
            // (content-generation successes only; capture needs no snapshot
            // at settle time — pricing is resolved here).
            let AttemptOutcome {
                status,
                mut headers,
                source,
                sent_body,
                disposition,
                send_ms,
                sent_url,
                method,
                sent_headers,
            } = outcome;
            let latency_ms = send_ms.map(|ms| ms as i64).unwrap_or(0);
            // §8-D upstream response capture is gated here; the guard/return
            // value carries it. `materialize` runs BEFORE `log_upstream` so the
            // non-streaming upstream body can be folded into the same INSERT,
            // and BEFORE `settle::capture` (which moves `sent_body`).
            let up_cap = {
                let ls = state.cp().log_settings.clone();
                (ls.enable_upstream_log && ls.enable_upstream_log_body).then(|| {
                    UpstreamRespCapture {
                        state: state.clone(),
                        request_id: ctx.request_id.clone(),
                    }
                })
            };
            // The attempt reached the provider and is being relayed; log its
            // upstream row EVEN IF response materialization fails afterwards
            // (a 2xx whose cross-protocol transform errors must still leave an
            // upstream trace). Borrow `upstream_raw` from the Ok arm; `?` below
            // propagates a materialize error only after the row is written.
            let mat = materialize(&channel, source, &plan, ctx, status, up_cap);
            // §8-D upstream capture: the attempt actually returned to the
            // client (success or relayed permanent 4xx). Failed-over attempts
            // were audited above; gating happens inside `capture`. `resp_body`
            // is the non-streaming provider response (streams backfill via guard).
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
            let Materialized { body, .. } = mat?;
            let settle_ctx = status
                .is_success()
                .then(|| {
                    settle::SettleCtx::capture(state, ctx, cand, &channel, sent_body, latency_ms)
                })
                .flatten();
            if plan.is_transform() {
                // converted bytes no longer match the upstream framing
                headers.remove(http::header::CONTENT_LENGTH);
            }
            // §6.3 merged models: append manual + variant entries to a
            // successful upstream list (inbound-shaped by now).
            let body = match (&local_models, body) {
                (Some(models), ResponseBody::Full(b)) if status.is_success() => {
                    headers.remove(http::header::CONTENT_LENGTH);
                    let family = ctx.op.expect("classified").provider_family();
                    ResponseBody::Full(local_ops::merge_into_list(
                        family,
                        b,
                        &local_ops::entries_from(models),
                    ))
                }
                (_, body) => body,
            };
            // §17: streams get the relay buffer + Drop guard; full bodies
            // settle inline (`Complete`).
            let body = settle::attach(settle_ctx, body, ctx.stream).await;
            // §17: embeddings / image generation are provider-shaped (not
            // content-generation) so `capture` skipped them — settle them inline
            // from the buffered JSON (a no-op for every other op).
            if status.is_success()
                && let ResponseBody::Full(b) = &body
            {
                settle::provider::settle(state, ctx, cand, b).await;
            }
            return Ok(ExecOutcome {
                status,
                headers,
                body,
                disposition,
            });
        }

        // AuthDead / RateLimited / Transient → drop this attempt, try the next.
        settle::audit_failure(
            state,
            &ctx.request_id,
            cand,
            settle::FailedAttempt {
                url: &outcome.sent_url,
                method: outcome.method.as_str(),
                status: i64::from(outcome.status.as_u16()),
                latency_ms: outcome.send_ms.map(|ms| ms as i64).unwrap_or(0),
                error: &format!("upstream {} ({:?})", outcome.status, outcome.disposition),
            },
        );
        last_err = Some(PipelineError::Transport(format!(
            "upstream {} ({:?})",
            outcome.status, outcome.disposition
        )));
    }

    // §6.3 count fallback: count must not fail just because upstreams did —
    // answer locally (the first candidate supplies provider settings).
    if ctx.op.expect("classified").operation == Operation::CountTokens
        && let Some(cand) = candidates.first()
        && let Some(o) = local_ops::serve_local(state, &state.cp(), ctx, cand)
    {
        tracing::warn!("all upstream count attempts failed; serving local count fallback");
        return Ok(o);
    }

    Err(last_err.unwrap_or(PipelineError::AllAttemptsFailed))
}

/// Per-credential minute budgets (§3.3), incr-then-check on the shared cache
/// (same off-by-one semantics as authz). rpm increments per attempt; tpm is
/// read-only here — settle-time reconciliation (M6 §17) feeds `ctpm:*` with
/// each request's actual total tokens. A counter backend failure counts as
/// exhausted (fail-closed): a configured budget must not vanish with the cache.
async fn budget_exhausted(state: &AppState, cand: &Candidate) -> bool {
    let bucket = crate::util::time::unix_now() / 60;
    let ttl = Some(Duration::from_secs(120));
    if let Some(limit) = cand.credential.rpm_limit {
        let key = format!("crpm:{}:m{bucket}", cand.credential.id);
        if !matches!(state.cache.incr(&key, 1, ttl).await, Ok(n) if n <= limit) {
            return true;
        }
    }
    if let Some(limit) = cand.credential.tpm_limit {
        let key = format!("ctpm:{}:m{bucket}", cand.credential.id);
        if !matches!(state.cache.incr(&key, 0, ttl).await, Ok(n) if n <= limit) {
            return true;
        }
    }
    false
}
