use std::collections::BTreeSet;

use web_time::Instant;

use gproxy_channel_api::{Disposition, ForwardRetry, ForwardSpec, SurfaceAffinity, SurfaceRequest};

use crate::api::Core;
use crate::boundary::{ExecOutcome, RequestCtx};
use crate::control::{FailoverBudget, Target};
use crate::error::CoreError;
use crate::host::Host;

use super::super::affinity::Selected;
use super::ForwardAttempt;

pub(super) async fn run<H: Host>(
    core: &Core<H>,
    selected: &Selected,
    ctx: &RequestCtx,
    spec: &ForwardSpec,
    websocket: bool,
    budget: FailoverBudget,
    started: Instant,
) -> Result<(ExecOutcome, Target), CoreError> {
    let retryable = spec.retry == ForwardRetry::Retryable;
    if retryable && websocket {
        return Err(CoreError::Internal(
            "surface websocket cannot declare retryable forwarding".into(),
        ));
    }
    let limit = if retryable { budget.max_attempts } else { 1 };
    if limit == 0 {
        return Err(CoreError::UpstreamExhausted(
            "surface attempt budget is zero".into(),
        ));
    }
    let upstream_path = super::super::template::render(spec.upstream_template, &selected.params)?;
    let stream = !websocket
        && !matches!(
            selected.entry.affinity,
            SurfaceAffinity::BodyField { .. }
                | SurfaceAffinity::HeaderOrBodyField { .. }
                | SurfaceAffinity::ResponseBodyToken { .. }
        );
    let mut attempts = 0;
    let mut dead = BTreeSet::new();
    let mut admission_error = None;
    let mut last = "no eligible credential";
    for target in &selected.candidates {
        if attempts >= limit || dead.contains(&target.credential) {
            continue;
        }
        let request = SurfaceRequest {
            label: spec.label,
            key: None,
            stream,
            method: ctx.method.clone(),
            upstream_path: upstream_path.clone(),
            query: ctx.query.clone(),
            headers: ctx.headers.clone(),
            body: ctx.body.clone(),
            credential: Some(target.credential),
        };
        let result = super::request(
            core,
            target,
            request,
            super::AttemptOptions {
                websocket,
                request_id: ctx.request_id.clone(),
                started,
                pricing: None,
                retryable,
            },
        )
        .await;
        match result {
            Ok(ForwardAttempt::Outcome(outcome)) => return Ok((outcome, target.clone())),
            Ok(ForwardAttempt::Retry(disposition)) => {
                attempts += 1;
                if disposition == Disposition::CredentialDead {
                    dead.insert(target.credential);
                }
                last = match disposition {
                    Disposition::Retryable => "retryable upstream response",
                    Disposition::CredentialDead => "credential rejected upstream",
                    Disposition::Success | Disposition::Terminal => "unexpected retry disposition",
                };
            }
            Err(CoreError::Channel(
                gproxy_channel_api::ChannelError::Secret(_)
                | gproxy_channel_api::ChannelError::Refresh(_),
            )) if retryable => {
                dead.insert(target.credential);
                last = "credential unavailable before egress";
            }
            Err(CoreError::Transport(_)) if retryable => {
                attempts += 1;
                last = "upstream transport failed";
            }
            Err(error @ (CoreError::RateLimited { .. } | CoreError::QuotaExceeded))
                if retryable =>
            {
                admission_error = Some(error);
                last = "credential admission limit reached";
            }
            Err(error) => return Err(error),
        }
    }
    if attempts == 0
        && let Some(error) = admission_error
    {
        return Err(error);
    }
    Err(CoreError::UpstreamExhausted(format!(
        "surface forwarding exhausted {attempts} upstream attempt(s): {last}"
    )))
}
