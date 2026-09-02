use futures_util::FutureExt as _;
use futures_util::future::{Either, select};
use gproxy_channel_api::{WsDuplex, WsFrame};

use crate::error::CoreError;
use crate::host::Host;

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
