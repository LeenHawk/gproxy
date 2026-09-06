use gproxy_channel_api::{
    BoxFuture, RealtimeMeter, SessionObservation, TransportError, WsDuplex, WsFrame,
};

use crate::Shared;
use crate::boundary::ExecOutcome;
use crate::control::ControlPlane;
use crate::host::Host;
use crate::usage::Ended;

use super::super::FunnelCtx;
use super::guard::Guard;

mod drain;

pub(crate) async fn realtime<H: Host>(
    host: Shared<H>,
    ctx: FunnelCtx,
    control: &dyn ControlPlane,
    socket: Box<dyn WsDuplex>,
) -> ExecOutcome {
    // Calls created through SDP already have an owned server-side meter.
    if ctx
        .resource
        .as_ref()
        .is_some_and(|(kind, _)| *kind == "realtime_call")
    {
        return super::super::websocket(host, ctx, socket).await;
    }
    let meter = RealtimeMeter::new(&ctx.request_body, &ctx.target.upstream_model);
    super::super::bridged_websocket(Box::new(MeteredSocket {
        host: host.clone(),
        session: Some(Session {
            socket,
            guard: Guard::direct(host, ctx),
            control: control.detached(),
            meter,
        }),
    }))
}

struct MeteredSocket<H: Host> {
    host: Shared<H>,
    session: Option<Session<H>>,
}

struct Session<H: Host> {
    socket: Box<dyn WsDuplex>,
    guard: Guard<H>,
    control: Box<dyn ControlPlane>,
    meter: RealtimeMeter,
}

impl<H: Host> Session<H> {
    fn compromised(&mut self, reason: &str) -> TransportError {
        super::usage::log_compromise(self.guard.ctx(), &reason);
        self.guard.totals_mut().mark_compromised();
        TransportError::Interrupted(reason.into())
    }

    fn observe(&mut self, frame: &WsFrame) -> Result<(), TransportError> {
        let ready = self.meter.ready();
        match self.meter.observe(frame) {
            SessionObservation::None => {}
            SessionObservation::Usage(sample) if ready => {
                let provider = self.guard.ctx().target.provider.clone();
                let tier = self
                    .guard
                    .ctx()
                    .pricing
                    .as_ref()
                    .and_then(|price| price.service_tier.clone());
                if let Err(error) = self.guard.totals_mut().add(
                    sample,
                    self.control.as_ref(),
                    &provider,
                    tier.as_deref(),
                ) {
                    return Err(self.compromised(&error.to_string()));
                }
            }
            SessionObservation::Usage(_) => {
                return Err(self.compromised("Realtime usage arrived before server session state"));
            }
            SessionObservation::Compromised { reason, .. } => return Err(self.compromised(&reason)),
        }
        if self.meter.ready() {
            self.guard.set_primary_model(self.meter.primary_model());
        }
        Ok(())
    }
}

impl<H: Host> WsDuplex for MeteredSocket<H> {
    fn send<'a>(&'a mut self, frame: WsFrame) -> BoxFuture<'a, Result<(), TransportError>> {
        Box::pin(async move {
            let Some(session) = self.session.as_mut() else {
                return Err(TransportError::Interrupted(
                    "Realtime session is closed".into(),
                ));
            };
            let close = matches!(frame, WsFrame::Close(_));
            let result = session.socket.send(frame).await;
            if result.is_err() || close {
                let session = self.session.take().expect("active session");
                drain::finish(
                    self.host.clone(),
                    session,
                    if result.is_err() {
                        Ended::Interrupted
                    } else {
                        Ended::Complete
                    },
                )
                .await;
            }
            result
        })
    }

    fn recv<'a>(&'a mut self) -> BoxFuture<'a, Result<Option<WsFrame>, TransportError>> {
        Box::pin(async move {
            let Some(session) = self.session.as_mut() else {
                return Ok(None);
            };
            match session.socket.recv().await {
                Ok(Some(frame @ WsFrame::Close(code))) => {
                    let _ = session.socket.send(WsFrame::Close(code)).await;
                    let session = self.session.take().expect("active session");
                    session
                        .guard
                        .finish(if code.is_none() || code == Some(1000) {
                            Ended::Complete
                        } else {
                            Ended::Interrupted
                        })
                        .await;
                    Ok(Some(frame))
                }
                Ok(Some(frame)) => {
                    if let Err(error) = session.observe(&frame) {
                        let _ = session.socket.send(WsFrame::Close(Some(1011))).await;
                        self.session
                            .take()
                            .expect("active session")
                            .guard
                            .finish(Ended::Interrupted)
                            .await;
                        return Err(error);
                    }
                    Ok(Some(frame))
                }
                Ok(None) => {
                    self.session
                        .take()
                        .expect("active session")
                        .guard
                        .finish(Ended::Complete)
                        .await;
                    Ok(None)
                }
                Err(error) => {
                    self.session
                        .take()
                        .expect("active session")
                        .guard
                        .finish(Ended::Interrupted)
                        .await;
                    Err(error)
                }
            }
        })
    }
}

impl<H: Host> Drop for MeteredSocket<H> {
    fn drop(&mut self) {
        let Some(mut session) = self.session.take() else {
            return;
        };
        let host = self.host.clone();
        self.host
            .spawner()
            .expect("session spawner checked before egress")
            .spawn(Box::pin(async move {
                let _ = session.socket.send(WsFrame::Close(Some(1001))).await;
                drain::finish(host, session, Ended::Interrupted).await;
            }));
    }
}
