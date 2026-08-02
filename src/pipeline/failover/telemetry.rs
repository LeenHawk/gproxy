//! Structured events for one candidate's failover lifecycle.

use crate::channel::Disposition;
use crate::pipeline::context::{Candidate, RequestCtx};

use super::attempt::AttemptOutcome;

struct AttemptEvent<'a> {
    ctx: &'a RequestCtx,
    candidate: &'a Candidate,
    attempt: u32,
    max_attempts: u32,
    status: u16,
    disposition: Option<&'a Disposition>,
    latency_ms: f64,
    error: &'a str,
    will_retry: bool,
}

fn attempt_failed(event: &AttemptEvent<'_>) {
    let error = crate::http::telemetry::redact_url_query(event.error);
    tracing::warn!(
        request_id = %event.ctx.request_id,
        attempt = event.attempt,
        max_attempts = event.max_attempts,
        provider = %event.candidate.provider.name,
        channel = %event.candidate.provider.channel,
        credential_id = event.candidate.credential.id,
        upstream_model = %event.candidate.upstream_model_id,
        status = event.status,
        disposition = ?event.disposition,
        latency_ms = event.latency_ms,
        error = %error,
        will_retry = event.will_retry,
        "upstream.attempt_failed"
    );
}

fn failover(event: &AttemptEvent<'_>) {
    if !event.will_retry {
        return;
    }
    let error = crate::http::telemetry::redact_url_query(event.error);
    tracing::warn!(
        request_id = %event.ctx.request_id,
        attempt = event.attempt,
        max_attempts = event.max_attempts,
        provider = %event.candidate.provider.name,
        channel = %event.candidate.provider.channel,
        credential_id = event.candidate.credential.id,
        upstream_model = %event.candidate.upstream_model_id,
        status = event.status,
        disposition = ?event.disposition,
        latency_ms = event.latency_ms,
        error = %error,
        will_retry = true,
        "upstream.failover"
    );
}

fn forced_refresh_retry_event(event: &AttemptEvent<'_>) {
    let error = crate::http::telemetry::redact_url_query(event.error);
    tracing::info!(
        request_id = %event.ctx.request_id,
        attempt = event.attempt,
        max_attempts = event.max_attempts,
        provider = %event.candidate.provider.name,
        channel = %event.candidate.provider.channel,
        credential_id = event.candidate.credential.id,
        upstream_model = %event.candidate.upstream_model_id,
        status = event.status,
        disposition = ?event.disposition,
        latency_ms = event.latency_ms,
        error = %error,
        will_retry = true,
        "credential.forced_refresh_retry"
    );
}

pub(super) fn attempt_error(
    ctx: &RequestCtx,
    candidate: &Candidate,
    attempt: u32,
    max_attempts: u32,
    error: &str,
    will_retry: bool,
) {
    let event = AttemptEvent {
        ctx,
        candidate,
        attempt,
        max_attempts,
        status: 0,
        disposition: None,
        latency_ms: 0.0,
        error,
        will_retry,
    };
    attempt_failed(&event);
    failover(&event);
}

pub(super) fn outcome_failed(
    ctx: &RequestCtx,
    candidate: &Candidate,
    attempts: (u32, u32),
    outcome: &AttemptOutcome,
    error: &str,
    will_retry: bool,
    record_attempt: bool,
) {
    let (attempt, max_attempts) = attempts;
    let event = outcome_event(
        ctx,
        candidate,
        attempt,
        max_attempts,
        outcome,
        error,
        will_retry,
    );
    if record_attempt {
        attempt_failed(&event);
    }
    failover(&event);
}

pub(super) fn forced_refresh_retry(
    ctx: &RequestCtx,
    candidate: &Candidate,
    attempt: u32,
    max_attempts: u32,
    outcome: &AttemptOutcome,
    error: &str,
) {
    let event = outcome_event(ctx, candidate, attempt, max_attempts, outcome, error, true);
    attempt_failed(&event);
    forced_refresh_retry_event(&event);
}

fn outcome_event<'a>(
    ctx: &'a RequestCtx,
    candidate: &'a Candidate,
    attempt: u32,
    max_attempts: u32,
    outcome: &'a AttemptOutcome,
    error: &'a str,
    will_retry: bool,
) -> AttemptEvent<'a> {
    AttemptEvent {
        ctx,
        candidate,
        attempt,
        max_attempts,
        status: outcome.status.as_u16(),
        disposition: Some(&outcome.disposition),
        latency_ms: outcome.send_ms.unwrap_or(0.0),
        error,
        will_retry,
    }
}

pub(super) fn selected(
    ctx: &RequestCtx,
    candidate: &Candidate,
    attempt: u32,
    max_attempts: u32,
    status: u16,
    disposition: &Disposition,
    latency_ms: f64,
) {
    if ctx.route_name.is_some() {
        tracing::Span::current().record("provider", candidate.provider.name.as_str());
    }
    tracing::debug!(
        request_id = %ctx.request_id,
        attempt,
        max_attempts,
        provider = %candidate.provider.name,
        channel = %candidate.provider.channel,
        credential_id = candidate.credential.id,
        upstream_model = %candidate.upstream_model_id,
        status,
        ?disposition,
        latency_ms,
        "upstream.selected"
    );
}

pub(super) fn all_attempts_failed(ctx: &RequestCtx, attempts: u32, max_attempts: u32, error: &str) {
    let error = crate::http::telemetry::redact_url_query(error);
    tracing::warn!(
        request_id = %ctx.request_id,
        attempt = attempts,
        max_attempts,
        error = %error,
        will_retry = false,
        "upstream.all_attempts_failed"
    );
}
