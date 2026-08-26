use crate::Shared;
use crate::host::Host;
use crate::usage::Ended;

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
        let ctx = self.ctx.as_ref().expect("active Realtime session context");
        let totals = self
            .totals
            .as_ref()
            .expect("active Realtime session totals");
        settle(self.host.as_ref(), ctx, totals, ended).await;
        let lease = self.lease.as_ref().expect("active Realtime session lease");
        super::ownership::release(self.host.as_ref(), lease).await;
        self.ctx = None;
        self.totals = None;
        self.lease = None;
    }
}

impl<H: Host> Drop for Guard<H> {
    fn drop(&mut self) {
        let (Some(ctx), Some(lease), Some(totals)) =
            (self.ctx.take(), self.lease.take(), self.totals.take())
        else {
            return;
        };
        let host = self.host.clone();
        let task_host = host.clone();
        host.spawner()
            .expect("session capability was checked before egress")
            .spawn(Box::pin(async move {
                settle(task_host.as_ref(), &ctx, &totals, Ended::Interrupted).await;
                super::ownership::release(task_host.as_ref(), &lease).await;
            }));
    }
}

async fn settle<H: Host>(host: &H, ctx: &FunnelCtx, totals: &Totals, ended: Ended) {
    settlement::complete(
        host,
        ctx,
        settlement::Completion {
            status: None,
            response_body: None,
            estimated_output_chars: None,
            record_usage: true,
            usage: Some(totals.usage.clone()),
            actual_service_tier: None,
            cost_override: Some(totals.cost),
            capture_response: false,
            ended,
        },
    )
    .await;
}
