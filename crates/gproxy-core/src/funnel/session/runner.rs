use gproxy_channel_api::{PreparedRequest, SessionObservation, WsFrame};

use crate::Shared;
use crate::host::Host;
use crate::usage::Ended;

use super::super::FunnelCtx;
use super::guard::Guard;
use super::install::Installed;
use super::observer::Received;

pub(super) fn run<H: Host>(
    host: Shared<H>,
    ctx: FunnelCtx,
    installed: Installed,
) -> gproxy_channel_api::BoxFuture<'static, ()> {
    let Installed {
        mut socket,
        termination,
        mut meter,
        control,
        initial,
        lease,
        ..
    } = installed;
    let provider = ctx.target.provider.clone();
    let requested_tier = ctx
        .pricing
        .as_ref()
        .and_then(|pricing| pricing.service_tier.clone());
    let mut guard = Guard::new(host.clone(), ctx, lease);
    guard.set_primary_model(meter.primary_model());
    Box::pin(async move {
        let mut termination = Some(termination);
        for sample in initial {
            if let Err(error) = guard.totals_mut().add(
                sample,
                control.as_ref(),
                &provider,
                requested_tier.as_deref(),
            ) {
                super::usage::log_compromise(guard.ctx(), &error);
                fail_closed(&host, &mut guard, &mut termination).await;
                guard.finish(Ended::Interrupted).await;
                return;
            }
        }
        let mut ended = Ended::Complete;
        let mut renewed_at = web_time::Instant::now();
        loop {
            let renew_in = super::ownership::RENEW_AFTER.saturating_sub(renewed_at.elapsed());
            match super::observer::receive(host.as_ref(), socket.as_mut(), renew_in, guard.lease())
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
                                super::usage::log_compromise(guard.ctx(), &error);
                                ended = Ended::Interrupted;
                                fail_closed(&host, &mut guard, &mut termination).await;
                                break;
                            }
                        }
                        SessionObservation::Usage(_) => {
                            tracing::error!(request_id = %guard.ctx().request_id, "Realtime usage arrived before trusted session state");
                            ended = Ended::Interrupted;
                            fail_closed(&host, &mut guard, &mut termination).await;
                            break;
                        }
                        SessionObservation::Compromised { reason, .. } => {
                            tracing::error!(request_id = %guard.ctx().request_id, reason, "Realtime meter integrity was compromised");
                            ended = Ended::Interrupted;
                            fail_closed(&host, &mut guard, &mut termination).await;
                            break;
                        }
                    }
                    if meter.ready() {
                        guard.set_primary_model(meter.primary_model());
                    }
                }
                Ok(Received::Frame(Some(frame @ WsFrame::Binary(_)))) => {
                    if let SessionObservation::Compromised { reason, .. } = meter.observe(&frame) {
                        tracing::error!(request_id = %guard.ctx().request_id, reason, "Realtime meter integrity was compromised");
                    }
                    ended = Ended::Interrupted;
                    fail_closed(&host, &mut guard, &mut termination).await;
                    break;
                }
                Ok(Received::Frame(Some(WsFrame::Close(code @ Some(1000))))) => {
                    let _ = socket.send(WsFrame::Close(code)).await;
                    break;
                }
                Ok(Received::Frame(Some(WsFrame::Close(code)))) => {
                    let _ = socket.send(WsFrame::Close(code)).await;
                    ended = Ended::Interrupted;
                    fail_closed(&host, &mut guard, &mut termination).await;
                    break;
                }
                Ok(Received::Frame(None)) => {
                    ended = Ended::Interrupted;
                    fail_closed(&host, &mut guard, &mut termination).await;
                    break;
                }
                Err(error) => {
                    tracing::error!(request_id = %guard.ctx().request_id, error = %error, "Realtime sideband observer failed");
                    ended = Ended::Interrupted;
                    fail_closed(&host, &mut guard, &mut termination).await;
                    break;
                }
            }
        }
        guard.finish(ended).await;
    })
}

async fn fail_closed<H: Host>(
    host: &Shared<H>,
    guard: &mut Guard<H>,
    termination: &mut Option<PreparedRequest>,
) {
    guard.totals_mut().mark_compromised();
    if let Some(termination) = termination.take() {
        super::termination::send(host.as_ref(), guard.ctx(), termination).await;
    }
}
