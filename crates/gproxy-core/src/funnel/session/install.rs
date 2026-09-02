use futures_util::FutureExt as _;
use futures_util::future::{Either, select};
use gproxy_channel_api::{
    Channel, PreparedRequest, RealtimeMeter, SessionUsage, WsDuplex, WsFrame,
};

use crate::control::ControlPlane;
use crate::error::CoreError;
use crate::host::Host;

use super::super::FunnelCtx;

pub(super) struct Installed {
    pub socket: Box<dyn WsDuplex>,
    pub termination: PreparedRequest,
    pub meter: RealtimeMeter,
    pub control: Box<dyn ControlPlane>,
    pub initial: Vec<SessionUsage>,
    pub lease: super::ownership::Lease,
}

pub(super) async fn open<H: Host>(
    host: &H,
    channel: std::sync::Arc<dyn Channel>,
    control: &dyn ControlPlane,
    ctx: &FunnelCtx,
    response_headers: &http::HeaderMap,
) -> Result<Installed, CoreError> {
    let connector = super::connector::Connector::new(
        channel.clone(),
        ctx.target.clone(),
        ctx.request_body.clone(),
        ctx.request_headers
            .clone()
            .expect("session funnel retained its request headers"),
        response_headers.clone(),
    );
    let prepared = connector.prepare(host, false).await?;
    let lease = super::ownership::claim(host, channel.as_ref(), ctx, &prepared.id).await?;
    let attempt = prepared.open(host).await;
    super::capture::sideband(
        host,
        ctx,
        0,
        attempt.url,
        attempt.body,
        attempt.opened.is_ok(),
    )
    .await;
    let mut socket = match attempt.opened {
        Ok(socket) => socket,
        Err(error) => {
            host.record_credential_health(
                ctx.target.credential,
                &ctx.target.upstream_model,
                attempt.credential_version,
                crate::CredentialHealth::Degraded,
                None,
                "Realtime sideband handshake failed",
            )
            .await;
            super::ownership::release(host, &lease).await;
            return Err(error.into());
        }
    };
    let termination = attempt.termination;
    let mut meter = attempt.meter;
    let mut initial = Vec::new();
    let ready_started = web_time::Instant::now();
    while !meter.ready() {
        let timeout = std::time::Duration::from_secs(10).saturating_sub(ready_started.elapsed());
        if timeout.is_zero() {
            close(&mut socket).await;
            super::ownership::release(host, &lease).await;
            return Err(session_timeout());
        }
        let frame = match recv_ready(host, socket.as_mut(), timeout).await {
            Ok(Some(frame @ WsFrame::Text(_))) => frame,
            Ok(Some(WsFrame::Binary(_))) => {
                close(&mut socket).await;
                super::ownership::release(host, &lease).await;
                return Err(CoreError::Internal(
                    "Realtime sideband sent binary data before session state".into(),
                ));
            }
            Ok(Some(WsFrame::Close(code))) => {
                let _ = socket.send(WsFrame::Close(code)).await;
                super::ownership::release(host, &lease).await;
                return Err(CoreError::Transport(
                    gproxy_channel_api::TransportError::Interrupted(
                        "Realtime sideband closed before session state".into(),
                    ),
                ));
            }
            Ok(None) => {
                super::ownership::release(host, &lease).await;
                return Err(CoreError::Transport(
                    gproxy_channel_api::TransportError::Interrupted(
                        "Realtime sideband closed before session state".into(),
                    ),
                ));
            }
            Err(error) => {
                close(&mut socket).await;
                super::ownership::release(host, &lease).await;
                return Err(error);
            }
        };
        match meter.observe(&frame) {
            gproxy_channel_api::SessionObservation::Usage(sample) => initial.push(sample),
            gproxy_channel_api::SessionObservation::None => {}
            gproxy_channel_api::SessionObservation::Compromised { reason, .. } => {
                close(&mut socket).await;
                super::ownership::release(host, &lease).await;
                return Err(gproxy_channel_api::ChannelError::Decode(reason).into());
            }
        }
    }
    Ok(Installed {
        socket,
        termination,
        meter,
        control: control.detached(),
        initial,
        lease,
    })
}

async fn recv_ready<H: Host>(
    host: &H,
    socket: &mut dyn WsDuplex,
    timeout: std::time::Duration,
) -> Result<Option<WsFrame>, CoreError> {
    let receive = socket.recv().fuse();
    let deadline = host.wait(timeout).fuse();
    futures_util::pin_mut!(receive, deadline);
    match select(receive, deadline).await {
        Either::Left((result, _)) => result.map_err(Into::into),
        Either::Right(_) => Err(session_timeout()),
    }
}

fn session_timeout() -> CoreError {
    CoreError::Transport(gproxy_channel_api::TransportError::Interrupted(
        "Realtime sideband session state timed out".into(),
    ))
}

async fn close(socket: &mut Box<dyn WsDuplex>) {
    let _ = socket.send(WsFrame::Close(Some(1011))).await;
}
