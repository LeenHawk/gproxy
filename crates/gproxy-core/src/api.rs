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
    #[error(
        "channel `{channel}` declares resource affinity but the host provides no binding store"
    )]
    ResourceAffinityWithoutBindings { channel: &'static str },
    #[error("channel `{channel}` requires a process-local continuation store")]
    ContinuationsUnavailable { channel: &'static str },
    #[error("channel `{channel}` requires a native background spawner")]
    ContinuationSpawnerUnavailable { channel: &'static str },
    #[error("channel `{channel}` declares a long-lived session without a trusted meter")]
    SessionMeterUnavailable { channel: &'static str },
}

/// The engine. Generic over the host so everything is statically
/// dispatched; an embedder's `Host` impl is the only wiring required.
pub struct Core<H: Host> {
    pub(crate) host: crate::Shared<H>,
    pub(crate) channels: ChannelRegistry,
}

impl<H: Host> Clone for Core<H> {
    fn clone(&self) -> Self {
        Self {
            host: self.host.clone(),
            channels: self.channels.clone(),
        }
    }
}

impl<H: Host> Core<H> {
    /// Returns whether a host request path is a declared operation or channel
    /// service surface. Hosts use this before stripping a named-route prefix so
    /// routing syntax never grows a second list of protocol paths.
    pub fn matches_ingress(&self, method: &http::Method, path: &str, upgrade: bool) -> bool {
        let operation = gproxy_protocol::match_ingress_for(method, path, None)
            .is_some_and(|matched| matched.upgrade == upgrade);
        operation
            || self.channels.iter().any(|channel| {
                channel.surfaces().0.iter().any(|entry| {
                    if entry.method != method
                        || gproxy_protocol::match_path(entry.pattern, path).is_none()
                    {
                        return false;
                    }
                    let websocket = match &entry.action {
                        gproxy_channel_api::SurfaceAction::ForwardWebSocket(_) => true,
                        gproxy_channel_api::SurfaceAction::OperationAlias { canonical_path } => {
                            gproxy_protocol::match_ingress_for(entry.method, canonical_path, None)
                                .is_some_and(|matched| matched.upgrade)
                        }
                        _ => false,
                    };
                    websocket == upgrade
                })
            })
    }

    pub fn channels(&self) -> impl Iterator<Item = &dyn gproxy_channel_api::Channel> + '_ {
        self.channels.iter()
    }

    pub fn channel_descriptors(
        &self,
    ) -> impl Iterator<Item = &'static gproxy_channel_api::ChannelDescriptor> + '_ {
        self.channels.iter().map(|channel| channel.descriptor())
    }

    /// Assemble the engine. Fails loudly at startup when a registered
    /// channel declares a service-surface table but the host provides no
    /// [`gproxy_channel_api::BindingStore`] — stateful surfaces and
    /// resource-affinity operations silently degrading (or fragmenting across
    /// instances) is exactly the class of bug this constructor refuses to ship.
    pub fn new(host: H, channels: ChannelRegistry) -> Result<Self, InitError> {
        if let Some(channel) = channels.iter().find(|channel| {
            channel.session_preparer().is_none()
                && channel.descriptor().supports.iter().any(|support| {
                    support.target.operation.spec().settle
                        == gproxy_protocol::SettleMode::OnSessionEnd
                })
        }) {
            return Err(InitError::SessionMeterUnavailable {
                channel: channel.descriptor().id,
            });
        }
        if host.bindings().is_none()
            && let Some(channel) = channels
                .iter()
                .find(|channel| !channel.surfaces().0.is_empty())
        {
            return Err(InitError::SurfacesWithoutBindings {
                channel: channel.descriptor().id,
            });
        }
        if let Some(channel) = channels
            .iter()
            .find(|channel| channel.requires_continuations())
        {
            if host.continuations().is_none() {
                return Err(InitError::ContinuationsUnavailable {
                    channel: channel.descriptor().id,
                });
            }
            if host.spawner().is_none() {
                return Err(InitError::ContinuationSpawnerUnavailable {
                    channel: channel.descriptor().id,
                });
            }
        }
        if host.bindings().is_none()
            && let Some(channel) = channels.iter().find(|channel| {
                channel.descriptor().supports.iter().any(|support| {
                    matches!(
                        support.source.operation.spec().affinity,
                        gproxy_protocol::Affinity::Resource(_)
                    )
                })
            })
        {
            return Err(InitError::ResourceAffinityWithoutBindings {
                channel: channel.descriptor().id,
            });
        }
        Ok(Self {
            host: crate::Shared::new(host),
            channels,
        })
    }

    /// Tier 1: send one request, in a wire shape the target's channel
    /// speaks natively, on one chosen credential. No routing, no
    /// transform, no failover — but refresh-on-expiry and the full funnel
    /// still apply. Service-surface forwards use exactly this.
    pub async fn invoke(
        &self,
        control: &dyn ControlPlane,
        target: &Target,
        ctx: RequestCtx,
    ) -> Result<ExecOutcome, CoreError> {
        crate::execution::invoke::run(self, control, target, ctx).await
    }

    /// Tier 2: the full engine. Resolves a plan from the control plane,
    /// then behaves as [`Self::execute_planned`].
    pub async fn execute(
        &self,
        control: &dyn ControlPlane,
        ctx: RequestCtx,
    ) -> Result<ExecOutcome, CoreError> {
        let classified = crate::execution::request::classify(&ctx);
        match crate::surface::dispatch(self, control, ctx, None, classified).await {
            crate::surface::Dispatch::Outcome(result) => result,
            crate::surface::Dispatch::Continue {
                ctx,
                classified,
                identity,
                plan,
                started,
            } => {
                crate::execution::resolved(self, control, *ctx, plan, classified, identity, started)
                    .await
            }
        }
    }

    /// Tier 2 with a caller-built plan: the embedder's entry when it does
    /// its own routing. The engine still classifies, transforms, fails
    /// over inside the plan's budget, and settles through the funnel.
    pub async fn execute_planned(
        &self,
        control: &dyn ControlPlane,
        ctx: RequestCtx,
        plan: Plan,
    ) -> Result<ExecOutcome, CoreError> {
        let classified = crate::execution::request::classify(&ctx);
        match crate::surface::dispatch(self, control, ctx, Some(&plan), classified).await {
            crate::surface::Dispatch::Outcome(result) => result,
            crate::surface::Dispatch::Continue {
                ctx,
                classified,
                identity,
                plan,
                started,
            } => {
                crate::execution::resolved(self, control, *ctx, plan, classified, identity, started)
                    .await
            }
        }
    }
}
