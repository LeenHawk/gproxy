mod capture;
mod connector;
mod guard;
mod install;
mod observer;
mod ownership;
mod runner;
mod socket;
mod termination;
mod usage;

pub(crate) use socket::realtime;

use bytes::Bytes;
use gproxy_channel_api::{Channel, Disposition};

use crate::Shared;
use crate::boundary::ExecOutcome;
use crate::control::ControlPlane;
use crate::host::Host;

use super::{BufferedRelay, FunnelCtx, Settled};

pub(super) async fn buffered<H: Host>(
    host: Shared<H>,
    channel: std::sync::Arc<dyn Channel>,
    control: &dyn ControlPlane,
    mut ctx: FunnelCtx,
    relay: BufferedRelay,
    disposition: Disposition,
) -> ExecOutcome {
    let BufferedRelay {
        response: upstream_response,
        capture_body,
        outward_ready,
        ..
    } = relay;
    let (parts, body) = upstream_response.into_parts();
    let upstream_status = parts.status;
    if !parts.status.is_success() || disposition != Disposition::Success {
        let outcome = outward(
            &ctx,
            channel.as_ref(),
            parts,
            body.clone(),
            outward_ready,
            disposition,
        );
        capture::call(
            host.as_ref(),
            &ctx,
            upstream_status,
            capture_body.unwrap_or(body),
        )
        .await;
        host.finish_admission(&ctx.request_id, None).await;
        completed(&ctx, Some(outcome.0), "session setup failed");
        return outcome_response(&ctx, outcome);
    }
    let installed = match install::open(
        host.as_ref(),
        channel.clone(),
        control,
        &ctx,
        &parts.headers,
    )
    .await
    {
        Ok(installed) => installed,
        Err(error) => {
            capture::call(
                host.as_ref(),
                &ctx,
                parts.status,
                capture_body.unwrap_or(body),
            )
            .await;
            host.finish_admission(&ctx.request_id, None).await;
            completed(&ctx, None, "session observer failed");
            return error_response(&ctx, error);
        }
    };
    ctx.request_headers = None;
    if ctx.target.upstream_model.is_empty() {
        ctx.target.upstream_model = installed.meter.primary_model().into();
    }
    crate::execution::resource::observe(
        host.as_ref(),
        channel.as_ref(),
        &ctx,
        parts.status,
        &parts.headers,
        capture_body.as_deref().unwrap_or(&body),
    )
    .await;
    let captured = capture_body.unwrap_or_else(|| body.clone());
    let outcome = outward(
        &ctx,
        channel.as_ref(),
        parts,
        body,
        outward_ready,
        disposition,
    );
    capture::call(host.as_ref(), &ctx, upstream_status, captured).await;
    let response = outcome_response(&ctx, outcome);
    host.spawner()
        .expect("session capability was checked before egress")
        .spawn(runner::run(host.clone(), ctx, installed));
    response
}

type Outward = (http::StatusCode, http::HeaderMap, Bytes, Disposition);

fn outward(
    ctx: &FunnelCtx,
    channel: &dyn Channel,
    parts: http::response::Parts,
    body: Bytes,
    ready: bool,
    disposition: Disposition,
) -> Outward {
    if ready {
        (parts.status, parts.headers, body, disposition)
    } else {
        let shaped = ctx.key.map_or_else(
            || Ok(body.clone()),
            |key| {
                channel.shape_response(gproxy_channel_api::ResponseShapeCtx {
                    key,
                    status: parts.status,
                    headers: &parts.headers,
                    body: &body,
                })
            },
        );
        super::transform_buffered(ctx, parts.status, parts.headers, shaped, disposition)
    }
}

fn outcome_response(ctx: &FunnelCtx, (status, headers, body, disposition): Outward) -> ExecOutcome {
    let (status, headers, body, disposition) =
        super::synth::outward_body(ctx, status, headers, body, disposition);
    ExecOutcome {
        status,
        headers: super::outward_headers(ctx, headers),
        body,
        disposition,
        _settled: Settled(()),
    }
}

fn error_response(ctx: &FunnelCtx, error: crate::CoreError) -> ExecOutcome {
    outcome_response(ctx, super::transform_error(error))
}

fn completed(ctx: &FunnelCtx, status: Option<http::StatusCode>, reason: &'static str) {
    tracing::info!(
        request_id = %ctx.request_id,
        provider_id = ctx.target.provider.id,
        credential_id = ctx.target.credential.0,
        operation = ?ctx.source_key.map(|key| key.operation()),
        status = status.map(|status| status.as_u16()),
        reason,
        "request.completed"
    );
}
