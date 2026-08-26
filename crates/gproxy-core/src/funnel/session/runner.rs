mod resync;

use gproxy_channel_api::{SessionObservation, WsFrame};

use crate::Shared;
use crate::host::Host;
use crate::usage::Ended;

use super::super::FunnelCtx;
use super::guard::Guard;
use super::install::Installed;
use super::reconnect::Received;

pub(super) fn run<H: Host>(
    host: Shared<H>,
    ctx: FunnelCtx,
    installed: Installed,
) -> gproxy_channel_api::BoxFuture<'static, ()> {
    let Installed {
        mut socket,
        mut meter,
        control,
        connector,
        initial,
        lease,
        mut credential_version,
    } = installed;
    let provider = ctx.target.provider.clone();
    let requested_tier = ctx
        .pricing
        .as_ref()
        .and_then(|pricing| pricing.service_tier.clone());
    let mut guard = Guard::new(host.clone(), ctx, lease);
    guard.set_primary_model(meter.primary_model());
    Box::pin(async move {
        let mut compromised = false;
        for sample in initial {
            if let Err(error) = guard.totals_mut().add(
                sample,
                control.as_ref(),
                &provider,
                requested_tier.as_deref(),
            ) {
                compromised = true;
                super::usage::log_compromise(guard.ctx(), &error);
            }
        }
        let mut reconnect_attempt = 0;
        let mut renewed_at = web_time::Instant::now();
        let mut resync = resync::Resync::default();
        loop {
            if resync.expired() {
                compromised = true;
                if resync.failure_limit_reached() {
                    break;
                }
                meter.require_resync();
                if super::reconnect::replace(
                    &host,
                    guard.ctx(),
                    &connector,
                    guard.lease(),
                    &mut credential_version,
                    &mut reconnect_attempt,
                    &mut socket,
                )
                .await
                .is_err()
                {
                    break;
                }
                renewed_at = web_time::Instant::now();
                resync.start();
                continue;
            }
            let renew_in = super::ownership::RENEW_AFTER.saturating_sub(renewed_at.elapsed());
            let wake_in = resync.wake_in(renew_in);
            match super::reconnect::receive(host.as_ref(), socket.as_mut(), wake_in, guard.lease())
                .await
            {
                Ok(Received::Renewed) => {
                    renewed_at = web_time::Instant::now();
                }
                Ok(Received::Frame(Some(frame @ WsFrame::Text(_)))) => {
                    let was_ready = meter.ready();
                    match meter.observe(&frame) {
                        SessionObservation::None => {}
                        SessionObservation::Usage(sample) if was_ready => {
                            if let Err(error) = guard.totals_mut().add(
                                sample,
                                control.as_ref(),
                                &provider,
                                requested_tier.as_deref(),
                            ) {
                                compromised = true;
                                super::usage::log_compromise(guard.ctx(), &error);
                            }
                        }
                        SessionObservation::Usage(_) => {
                            compromised = true;
                            tracing::error!(request_id = %guard.ctx().request_id, "Realtime usage arrived before trusted session state");
                        }
                        SessionObservation::Compromised {
                            reason,
                            resync: needs_resync,
                        } => {
                            compromised = true;
                            tracing::error!(request_id = %guard.ctx().request_id, reason, "Realtime meter integrity was compromised");
                            if needs_resync && {
                                meter.require_resync();
                                super::reconnect::replace(
                                    &host,
                                    guard.ctx(),
                                    &connector,
                                    guard.lease(),
                                    &mut credential_version,
                                    &mut reconnect_attempt,
                                    &mut socket,
                                )
                                .await
                                .is_err()
                            } {
                                break;
                            }
                            renewed_at = web_time::Instant::now();
                            resync.start();
                        }
                    }
                    if meter.ready() {
                        guard.set_primary_model(meter.primary_model());
                        resync.ready();
                    }
                }
                Ok(Received::Frame(Some(frame @ WsFrame::Binary(_)))) => {
                    compromised = true;
                    if let SessionObservation::Compromised { reason, .. } = meter.observe(&frame) {
                        tracing::error!(request_id = %guard.ctx().request_id, reason, "Realtime meter integrity was compromised");
                    }
                }
                Ok(Received::Frame(Some(WsFrame::Close(code))))
                    if matches!(code, None | Some(1000 | 1001)) =>
                {
                    let _ = socket.send(WsFrame::Close(code)).await;
                    break;
                }
                Ok(Received::Frame(Some(WsFrame::Close(code)))) => {
                    let _ = socket.send(WsFrame::Close(code)).await;
                    compromised = true;
                    meter.require_resync();
                    if super::reconnect::replace(
                        &host,
                        guard.ctx(),
                        &connector,
                        guard.lease(),
                        &mut credential_version,
                        &mut reconnect_attempt,
                        &mut socket,
                    )
                    .await
                    .is_err()
                    {
                        break;
                    }
                    renewed_at = web_time::Instant::now();
                    resync.start();
                }
                Ok(Received::Frame(None)) | Err(_) => {
                    compromised = true;
                    meter.require_resync();
                    if super::reconnect::replace(
                        &host,
                        guard.ctx(),
                        &connector,
                        guard.lease(),
                        &mut credential_version,
                        &mut reconnect_attempt,
                        &mut socket,
                    )
                    .await
                    .is_err()
                    {
                        break;
                    }
                    renewed_at = web_time::Instant::now();
                    resync.start();
                }
            }
        }
        if compromised {
            guard.totals_mut().mark_compromised();
        }
        guard
            .finish(if compromised {
                Ended::Interrupted
            } else {
                Ended::Complete
            })
            .await;
    })
}
