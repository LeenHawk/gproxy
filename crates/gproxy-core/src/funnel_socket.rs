use gproxy_channel_api::{BoxFuture, TransportError, WsDuplex, WsFrame};

use crate::Shared;
use crate::funnel::{FunnelCtx, complete_stream};
use crate::host::Host;
use crate::usage::Ended;

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
