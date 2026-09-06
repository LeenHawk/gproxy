use gproxy_channel_api::{BoxFuture, TransportError, WsDuplex, WsFrame};

use crate::Shared;
use crate::host::Host;
use crate::usage::Ended;

use super::{FunnelCtx, Settled, complete_stream};
use crate::boundary::{ExecOutcome, ResponseBody};
use gproxy_channel_api::Disposition;

pub(crate) struct FunnelSocket<H: Host> {
    inner: Box<dyn WsDuplex>,
    host: Shared<H>,
    ctx: Option<FunnelCtx>,
}

impl<H: Host> FunnelSocket<H> {
    pub(crate) fn new(host: Shared<H>, ctx: FunnelCtx, inner: Box<dyn WsDuplex>) -> Self {
        Self {
            inner,
            host,
            ctx: Some(ctx),
        }
    }

    async fn finish(&mut self, ended: Ended) {
        let Some(ctx) = self.ctx.take() else {
            return;
        };
        complete_stream(
            self.host.clone(),
            ctx,
            http::StatusCode::SWITCHING_PROTOCOLS,
            None,
            None,
            None,
            ended,
        )
        .await;
    }
}

impl<H: Host> WsDuplex for FunnelSocket<H> {
    fn send<'a>(&'a mut self, frame: WsFrame) -> BoxFuture<'a, Result<(), TransportError>> {
        Box::pin(async move {
            let closes = matches!(frame, WsFrame::Close(_));
            match self.inner.send(frame).await {
                Ok(()) if closes => {
                    self.finish(Ended::Complete).await;
                    Ok(())
                }
                Ok(()) => Ok(()),
                Err(error) => {
                    self.finish(Ended::Interrupted).await;
                    Err(error)
                }
            }
        })
    }

    fn recv<'a>(&'a mut self) -> BoxFuture<'a, Result<Option<WsFrame>, TransportError>> {
        Box::pin(async move {
            match self.inner.recv().await {
                Ok(Some(frame @ WsFrame::Close(_))) => {
                    self.finish(Ended::Complete).await;
                    Ok(Some(frame))
                }
                Ok(Some(frame)) => Ok(Some(frame)),
                Ok(None) => {
                    self.finish(Ended::Complete).await;
                    Ok(None)
                }
                Err(error) => {
                    self.finish(Ended::Interrupted).await;
                    Err(error)
                }
            }
        })
    }
}

impl<H: Host> Drop for FunnelSocket<H> {
    fn drop(&mut self) {
        let Some(ctx) = self.ctx.take() else {
            return;
        };
        if let Some(spawner) = self.host.spawner() {
            spawner.spawn(Box::pin(complete_stream(
                self.host.clone(),
                ctx,
                http::StatusCode::SWITCHING_PROTOCOLS,
                None,
                None,
                None,
                Ended::Interrupted,
            )));
        } else {
            tracing::warn!(
                request_id = %ctx.request_id,
                "websocket dropped before inline settlement could complete"
            );
        }
    }
}

pub(crate) fn websocket<H: Host>(
    host: Shared<H>,
    ctx: FunnelCtx,
    socket: Box<dyn gproxy_channel_api::WsDuplex>,
) -> ExecOutcome {
    ExecOutcome {
        status: http::StatusCode::SWITCHING_PROTOCOLS,
        headers: http::HeaderMap::new(),
        body: ResponseBody::WebSocket(Box::new(FunnelSocket::new(host, ctx, socket))),
        disposition: Disposition::Success,
        _settled: Settled(()),
    }
}

pub(crate) fn bridged_websocket(socket: Box<dyn gproxy_channel_api::WsDuplex>) -> ExecOutcome {
    ExecOutcome {
        status: http::StatusCode::SWITCHING_PROTOCOLS,
        headers: http::HeaderMap::new(),
        body: ResponseBody::WebSocket(socket),
        disposition: Disposition::Success,
        _settled: Settled(()),
    }
}
