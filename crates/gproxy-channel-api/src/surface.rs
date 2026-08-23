//! Declared service surfaces: emulated vendor control-plane endpoints.
//!
//! A surface is a table, not code in a gateway: each entry names its route,
//! its credential affinity, and what happens — forward on one credential or
//! synthesize. Both exits pass the engine's funnel, so a surface cannot
//! leak unmetered traffic the way v2's hand-rolled service files did.
//!
//! Synthesizers act through [`SurfaceServices`]: narrow capabilities, no
//! transport, no raw persistence. The only way upstream is
//! [`SurfaceInvoke`] (tier 1, funneled); durable state is the host's
//! [`BindingStore`] — shared persistence, because bindings must survive
//! restarts and be visible to every instance. There is deliberately no
//! in-memory default: a store that fragments across instances is worse
//! than a loud startup error.

use bytes::Bytes;
use gproxy_protocol::PathPattern;
use rust_decimal::Decimal;
use serde_json::Value;

use crate::BoxFuture;
use crate::channel::ChannelError;
use crate::wire::{ByteStream, CredentialId, TransportError};

pub struct SurfaceTable(pub &'static [SurfaceEntry]);

pub struct SurfaceEntry {
    pub method: &'static http::Method,
    pub pattern: PathPattern,
    /// How the engine pins a credential before acting on this entry.
    pub affinity: SurfaceAffinity,
    pub action: SurfaceAction,
}

/// Declarative credential stickiness — v2 hand-rolled four cache-key
/// schemes for these in the HTTP layer; here they are rows, not code.
#[derive(Debug, Clone, Copy)]
pub enum SurfaceAffinity {
    None,
    /// Pin by a request header value (`"mcp-session-id"`).
    Header(&'static str),
    /// Pin by a JSON body field (`"server_id"`).
    BodyField(&'static str),
    /// Pin by a matched path param (`"task_id"`) through the binding
    /// store — durable resource ownership.
    Binding {
        kind: &'static str,
        param: &'static str,
    },
}

pub enum SurfaceAction {
    /// Forward to the provider on the pinned/selected credential (tier 1).
    Forward(ForwardSpec),
    /// Answer locally, optionally orchestrating upstream calls.
    Synthesize {
        handler: &'static dyn Synthesizer,
        /// Declares whether the handler receives [`SurfaceInvoke`] access.
        /// On the table so entries that can produce upstream traffic are
        /// visible by inspection, not by reading handler code.
        upstream: bool,
    },
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

/// What a synthesizer may read from the inbound request.
pub struct SynthCtx<'a> {
    pub method: &'a http::Method,
    pub path: &'a str,
    pub query: Option<&'a str>,
    pub headers: &'a http::HeaderMap,
    pub body: &'a Bytes,
    /// Captured pattern params, in pattern order.
    pub params: &'a [(&'static str, String)],
}

/// Narrow capabilities a synthesizer acts through.
pub struct SurfaceServices<'a> {
    /// `Some` iff the entry declared `upstream: true`.
    pub invoke: Option<&'a dyn SurfaceInvoke>,
    pub bindings: &'a dyn BindingStore,
    pub identity: &'a CallerIdentity,
    pub provider: &'a ProviderView<'a>,
    pub usage: &'a dyn UsageView,
}

pub trait Synthesizer: Send + Sync {
    fn respond<'a>(
        &'a self,
        ctx: SynthCtx<'a>,
        services: SurfaceServices<'a>,
    ) -> BoxFuture<'a, Result<SurfaceReply, ChannelError>>;
}

/// A surface response; forwards and synthesizers produce the same shape.
pub struct SurfaceReply {
    pub status: http::StatusCode,
    pub headers: http::HeaderMap,
    pub body: SurfaceBody,
}

pub enum SurfaceBody {
    Full(Bytes),
    Stream(ByteStream),
}

/// The only way upstream from a synthesizer. Every call is a tier-1
/// invoke: prepared by the channel, classified, and settled through the
/// funnel before the reply comes back.
pub trait SurfaceInvoke {
    fn invoke<'a>(
        &'a self,
        request: SurfaceRequest,
    ) -> BoxFuture<'a, Result<SurfaceReply, TransportError>>;

    /// Pre-signed side request (blob PUT to a storage URL from a prior
    /// response): no credential injection, different host — but still
    /// captured and counted. v2 reached for the raw client here; this is
    /// the fenced replacement.
    fn fetch_presigned<'a>(
        &'a self,
        request: http::Request<Bytes>,
    ) -> BoxFuture<'a, Result<SurfaceReply, TransportError>>;
}

/// An upstream call a synthesizer asks for, in the provider's own wire
/// shape. The engine picks the credential (or honors an explicit binding)
/// and runs channel prepare.
pub struct SurfaceRequest {
    pub label: &'static str,
    pub method: http::Method,
    pub upstream_path: String,
    pub query: Option<String>,
    pub headers: http::HeaderMap,
    pub body: Bytes,
    /// Land on this credential (from a binding) instead of balancing.
    pub credential: Option<CredentialId>,
}

/// Durable resource → credential ownership, namespaced per provider.
/// Host-provided over shared persistence — bindings must survive restarts
/// and be visible to every instance, so there is no default impl.
pub trait BindingStore {
    fn save<'a>(
        &'a self,
        kind: &'static str,
        id: &'a str,
        credential: CredentialId,
        summary: Value,
    ) -> BoxFuture<'a, Result<(), StateError>>;
    fn find<'a>(
        &'a self,
        kind: &'static str,
        id: &'a str,
    ) -> BoxFuture<'a, Result<Option<Binding>, StateError>>;
    fn delete<'a>(
        &'a self,
        kind: &'static str,
        id: &'a str,
    ) -> BoxFuture<'a, Result<(), StateError>>;
    fn list<'a>(
        &'a self,
        kind: &'static str,
        page: Page,
    ) -> BoxFuture<'a, Result<Vec<Binding>, StateError>>;
}

#[derive(Debug, Clone)]
pub struct Binding {
    pub kind: String,
    pub id: String,
    pub credential: CredentialId,
    /// Resource metadata for local list synthesis (v2 answered
    /// `/tasks/list` from this without fanning out upstream).
    pub summary: Value,
    pub created_at_unix: i64,
}

#[derive(Debug, Clone, Default)]
pub struct Page {
    pub cursor: Option<String>,
    pub limit: u32,
}

/// Who is asking (read-only).
#[derive(Debug, Clone)]
pub struct CallerIdentity {
    pub user_id: i64,
    pub user_key_id: i64,
    pub org_id: Option<i64>,
    pub team_id: Option<i64>,
}

/// The provider this surface serves (read-only).
pub struct ProviderView<'a> {
    pub id: i64,
    pub name: &'a str,
    pub settings: &'a Value,
}

/// Read-only local usage aggregates, scoped to (caller, provider) — what
/// virtual quota-window synthesis reads. Fixed methods on purpose; grows
/// only when a real synthesizer needs more.
pub trait UsageView {
    fn window<'a>(&'a self, since_unix: i64) -> BoxFuture<'a, Result<UsageWindow, StateError>>;
}

#[derive(Debug, Clone, Default)]
pub struct UsageWindow {
    pub cost: Decimal,
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// Failures from host-provided surface state (bindings, usage views).
#[derive(Debug, thiserror::Error)]
#[error("surface state: {0}")]
pub struct StateError(pub String);
