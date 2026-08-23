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

/// Constructor-time refusals. These are configuration errors, not request
/// errors: they surface at startup, never mid-traffic.
#[derive(Debug, thiserror::Error)]
pub enum InitError {
    #[error("channel `{channel}` declares service surfaces but the host provides no binding store")]
    SurfacesWithoutBindings { channel: &'static str },
}

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
    /// Assemble the engine. Fails loudly at startup when a registered
    /// channel declares a service-surface table but the host provides no
    /// [`gproxy_channel_api::BindingStore`] — stateful surfaces silently
    /// degrading (or fragmenting across instances) is exactly the class
    /// of bug this constructor refuses to ship.
    pub fn new(host: H, channels: ChannelRegistry) -> Result<Self, InitError> {
        if host.bindings().is_none()
            && let Some(channel) = channels
                .iter()
                .find(|channel| !channel.surfaces().0.is_empty())
        {
            return Err(InitError::SurfacesWithoutBindings {
                channel: channel.descriptor().id,
            });
        }
        Ok(Self { host, channels })
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
