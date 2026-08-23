//! The two public execution tiers.
//!
//! Tier 1 (`invoke`) is the "SDK with pooled-credential discipline": one
//! chosen target, no routing. Tier 2 (`execute`) is the full engine:
//! classify → auth → resolve → admit → transform → channel → transport →
//! failover → funnel. `gproxy-app` serves its whole data plane through
//! these two calls and nothing else — there is no private third entry
//! (v2's `codex_service.rs` was that entry, and it ran unmetered).

use gproxy_channel_api::ChannelRegistry;

use crate::boundary::{ExecOutcome, RequestCtx};
use crate::control::{ControlPlane, Plan, Target};
use crate::error::CoreError;
use crate::host::Host;

/// The engine. Generic over the host so everything is statically
/// dispatched; an embedder's `Host` impl is the only wiring required.
#[expect(
    dead_code,
    reason = "interface draft; the engine body lands next round"
)]
pub struct Core<H: Host> {
    host: H,
    channels: ChannelRegistry,
}

impl<H: Host> Core<H> {
    pub fn new(host: H, channels: ChannelRegistry) -> Self {
        Self { host, channels }
    }

    /// Tier 1: send one request, in a wire shape the target's channel
    /// speaks natively, on one chosen credential. No routing, no
    /// transform, no failover — but refresh-on-expiry and the full funnel
    /// still apply. Service-surface forwards use exactly this.
    #[expect(unused_variables, reason = "interface draft; bodies land next round")]
    pub async fn invoke(
        &self,
        control: &impl ControlPlane,
        target: &Target,
        ctx: RequestCtx,
    ) -> Result<ExecOutcome, CoreError> {
        todo!("implementation round")
    }

    /// Tier 2: the full engine. Resolves a plan from the control plane,
    /// then behaves as [`Self::execute_planned`].
    #[expect(unused_variables, reason = "interface draft; bodies land next round")]
    pub async fn execute(
        &self,
        control: &impl ControlPlane,
        ctx: RequestCtx,
    ) -> Result<ExecOutcome, CoreError> {
        todo!("implementation round")
    }

    /// Tier 2 with a caller-built plan: the embedder's entry when it does
    /// its own routing. The engine still classifies, transforms, fails
    /// over inside the plan's budget, and settles through the funnel.
    #[expect(unused_variables, reason = "interface draft; bodies land next round")]
    pub async fn execute_planned(
        &self,
        control: &impl ControlPlane,
        ctx: RequestCtx,
        plan: Plan,
    ) -> Result<ExecOutcome, CoreError> {
        todo!("implementation round")
    }
}
