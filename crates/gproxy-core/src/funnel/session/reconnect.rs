use futures_util::FutureExt as _;
use futures_util::future::{Either, select};
use gproxy_channel_api::{WsDuplex, WsFrame};

use crate::Shared;
use crate::error::CoreError;
use crate::host::Host;

use super::super::FunnelCtx;
use super::connector::Connector;
use super::ownership::Lease;

pub(super) enum Received {
    Frame(Option<WsFrame>),
    Renewed,
}

pub(super) async fn receive<H: Host>(
    host: &H,
    socket: &mut dyn WsDuplex,
    renew_in: std::time::Duration,
    lease: &Lease,
) -> Result<Received, CoreError> {
    let receive = socket.recv().fuse();
    let renew = host.wait(renew_in).fuse();
    futures_util::pin_mut!(receive, renew);
    match select(receive, renew).await {
        Either::Left((result, _)) => result.map(Received::Frame).map_err(Into::into),
        Either::Right(_) => {
            super::ownership::renew(host, lease).await?;
            Ok(Received::Renewed)
        }
    }
}

pub(super) async fn replace<H: Host>(
    host: &Shared<H>,
    ctx: &FunnelCtx,
    connector: &Connector,
    lease: &Lease,
    credential_version: &mut u64,
    attempt: &mut u64,
    socket: &mut Box<dyn WsDuplex>,
) -> Result<(), CoreError> {
    degraded(
        host.as_ref(),
        ctx,
        *credential_version,
        "Realtime sideband observer disconnected",
    )
    .await;
    let _ = socket.send(WsFrame::Close(Some(1011))).await;
    let (next, version) = open(host, ctx, connector, lease, attempt).await?;
    *socket = next;
    *credential_version = version;
    Ok(())
}

pub(super) async fn open<H: Host>(
    host: &Shared<H>,
    ctx: &FunnelCtx,
    connector: &Connector,
    lease: &Lease,
    attempt: &mut u64,
) -> Result<(Box<dyn WsDuplex>, u64), CoreError> {
    let mut delay = std::time::Duration::from_secs(1);
    let mut force_refresh = false;
    let mut refresh_attempts = 0_u8;
    loop {
        super::ownership::renew(host.as_ref(), lease).await?;
        host.wait(delay).await;
        *attempt = attempt.saturating_add(1);
        let prepared = connector.prepare(host.as_ref(), force_refresh).await?;
        force_refresh = false;
        let connection = prepared.open(host.as_ref()).await;
        super::capture::sideband(
            host.as_ref(),
            ctx,
            *attempt,
            connection.url,
            connection.body,
            connection.opened.is_ok(),
        )
        .await;
        match connection.opened {
            Ok(socket) => return Ok((socket, connection.credential_version)),
            Err(error @ gproxy_channel_api::TransportError::Status(404 | 410)) => {
                return Err(error.into());
            }
            Err(gproxy_channel_api::TransportError::Status(401 | 403)) if refresh_attempts < 2 => {
                refresh_attempts += 1;
                force_refresh = true;
            }
            Err(error @ gproxy_channel_api::TransportError::Status(401 | 403)) => {
                return Err(error.into());
            }
            Err(error @ gproxy_channel_api::TransportError::Status(status))
                if (400..=499).contains(&status) && !matches!(status, 408 | 409 | 425 | 429) =>
            {
                return Err(error.into());
            }
            Err(error) => {
                degraded(
                    host.as_ref(),
                    ctx,
                    connection.credential_version,
                    "Realtime sideband reconnect failed",
                )
                .await;
                tracing::warn!(request_id = %ctx.request_id, error = %error, "Realtime sideband reconnect failed");
                delay = (delay * 2).min(std::time::Duration::from_secs(30));
            }
        }
    }
}

pub(super) async fn degraded<H: Host>(
    host: &H,
    ctx: &FunnelCtx,
    credential_version: u64,
    detail: &'static str,
) {
    host.record_credential_health(
        ctx.target.credential,
        credential_version,
        crate::CredentialHealth::Degraded,
        None,
        detail,
    )
    .await;
}
