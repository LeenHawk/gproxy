use crate::Shared;
use crate::host::Host;
use crate::usage::Ended;
use futures_util::FutureExt as _;

use super::super::{FunnelCtx, settlement};
use super::ownership::Lease;
use super::usage::Totals;

pub(super) struct Guard<H: Host> {
    host: Shared<H>,
    ctx: Option<FunnelCtx>,
    lease: Option<Lease>,
    totals: Option<Totals>,
}

impl<H: Host> Guard<H> {
    pub(super) fn direct(host: Shared<H>, ctx: FunnelCtx) -> Self {
        Self {
            host,
            ctx: Some(ctx),
            lease: None,
            totals: Some(Totals::new()),
        }
    }
    pub(super) fn new(host: Shared<H>, ctx: FunnelCtx, lease: Lease) -> Self {
        Self {
            host,
            ctx: Some(ctx),
            lease: Some(lease),
            totals: Some(Totals::new()),
        }
    }

    pub(super) fn ctx(&self) -> &FunnelCtx {
        self.ctx.as_ref().expect("active Realtime session context")
    }

    pub(super) fn lease(&self) -> &Lease {
        self.lease.as_ref().expect("active Realtime session lease")
    }

    pub(super) fn totals_mut(&mut self) -> &mut Totals {
        self.totals
            .as_mut()
            .expect("active Realtime session totals")
    }

    pub(super) fn set_primary_model(&mut self, model: &str) {
        self.ctx
            .as_mut()
            .expect("active Realtime session context")
            .target
            .upstream_model = model.into();
    }

    pub(super) async fn finish(mut self, ended: Ended) {
        let ctx = self.ctx.take().expect("active Realtime session context");
        let totals = self.totals.take().expect("active Realtime session totals");
        let lease = self.lease.take();
        let host = self.host.clone();
        let task: gproxy_channel_api::BoxFuture<'static, ()> = Box::pin(async move {
            settle(host.as_ref(), &ctx, &totals, ended).await;
            if let Some(lease) = lease {
                super::ownership::release(host.as_ref(), &lease).await;
            }
        });
        // Cancellation of the closing client must not cancel or repeat persistence.
        let task = task.shared();
        self.host
            .spawner()
            .expect("session spawner checked before egress")
            .spawn(Box::pin(task.clone()));
        task.await;
    }
}

impl<H: Host> Drop for Guard<H> {
    fn drop(&mut self) {
        let (Some(ctx), Some(totals)) = (self.ctx.take(), self.totals.take()) else {
            return;
        };
        let host = self.host.clone();
        let lease = self.lease.take();
        let task_host = host.clone();
        host.spawner()
            .expect("session capability was checked before egress")
            .spawn(Box::pin(async move {
                settle(task_host.as_ref(), &ctx, &totals, Ended::Interrupted).await;
                if let Some(lease) = lease {
                    super::ownership::release(task_host.as_ref(), &lease).await;
                }
            }));
    }
}

async fn settle<H: Host>(host: &H, ctx: &FunnelCtx, totals: &Totals, ended: Ended) {
    let direct = ctx
        .key
        .is_some_and(|key| key.operation() == gproxy_protocol::Operation::ConnectRealtime);
    settlement::complete(
        host,
        ctx,
        settlement::Completion {
            status: direct.then_some(http::StatusCode::SWITCHING_PROTOCOLS),
            response_body: None,
            estimated_output_chars: None,
            record_usage: true,
            usage: Some(totals.usage.clone()),
            actual_service_tier: None,
            cost_override: Some(totals.cost),
            capture_response: direct,
            ended,
        },
    )
    .await;
}
