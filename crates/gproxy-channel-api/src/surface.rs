//! Declared service surfaces: emulated vendor control-plane endpoints.
//!
//! A surface is a table, not code in a gateway: each entry names its route
//! and what happens — synthesize locally or forward on one credential
//! (tier 1). Both exits pass the engine's funnel, so a surface cannot leak
//! unmetered traffic the way v2's hand-rolled service files did.

use bytes::Bytes;
use gproxy_protocol::PathPattern;
use serde_json::Value;

use crate::BoxFuture;
use crate::channel::ChannelError;

pub struct SurfaceTable(pub &'static [SurfaceEntry]);

pub struct SurfaceEntry {
    pub method: &'static http::Method,
    pub pattern: PathPattern,
    pub action: SurfaceAction,
}

pub enum SurfaceAction {
    /// Forward to the provider on one selected credential (tier 1 invoke).
    Forward(ForwardSpec),
    /// Answer locally.
    Synthesize(&'static dyn Synthesizer),
}

/// Upstream mapping for a forwarded surface route.
pub struct ForwardSpec {
    /// Audit label; also the telemetry tag. Never a control-flow switch
    /// (v2 overloaded labels as both, and a string constant in the HTTP
    /// layer changed channel behaviour).
    pub label: &'static str,
    /// Upstream path template with `{param}` placeholders filled from the
    /// matched pattern (`{rest}` for a `Seg::Rest` capture).
    pub upstream_template: &'static str,
}

/// What a synthesizer may read. Stateful synths (v2's task bindings hit
/// persistence) will additionally need a services handle — the shape of
/// that handle is an open question for round 3, tracked in
/// design/architecture.md.
pub struct SynthCtx<'a> {
    pub method: &'a http::Method,
    pub path: &'a str,
    pub query: Option<&'a str>,
    pub headers: &'a http::HeaderMap,
    pub body: &'a Bytes,
    /// Captured pattern params, in pattern order.
    pub params: &'a [(&'static str, String)],
    pub provider_settings: &'a Value,
}

pub struct SynthReply {
    pub status: http::StatusCode,
    pub headers: http::HeaderMap,
    pub body: Bytes,
}

pub trait Synthesizer: Send + Sync {
    fn respond<'a>(&'a self, ctx: SynthCtx<'a>) -> BoxFuture<'a, Result<SynthReply, ChannelError>>;
}
