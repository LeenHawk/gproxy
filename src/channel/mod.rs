//! Channel layer — pure upstream *access* (§6.1, §6.3).
//!
//! A `Channel` injects auth, resolves the endpoint, and declares transport
//! capability. It does **no** protocol transform and **no** rule rewriting
//! (those are the transform/process layers) and never mutates the request body.

pub mod bulletins;
pub mod disposition;
pub mod envelope;
pub mod http_util;
pub mod login;
pub mod oauth;
pub mod prepared;
pub mod registry;
pub mod resolve;
pub mod routes;
pub mod shaping;
pub mod usage;

use std::sync::Arc;

use bytes::Bytes;
use http::{HeaderMap, StatusCode};
use serde_json::Value;

use crate::http::client::UpstreamClient;

pub use disposition::Disposition;
pub use login::{AuthCodeStart, ChannelLogin, DeviceInit, DevicePoll};
pub use prepared::PreparedRequest;
pub use usage::{UsageCredits, UsageSnapshot, UsageWindow};

/// Declared upstream transport, for capability-based degradation (§7.4).
/// M1 uses `Http` only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportKind {
    Http,
    Ws,
}

/// A per-channel byte-stream decoder spliced BEFORE the M2 protocol transform.
/// Used by envelope/binary channels (code-assist per-frame unwrap, kiro Smithy
/// → SSE). Sync core, mirrors the protocol `SseTransformer`
/// ([`crate::transform::stream_adapter::SseTransformer`]): `push` per upstream
/// chunk, `finish` at EOF. Streaming is native-only, but the trait is defined
/// on both targets (the hook returns `None` everywhere by default) — the `Send`
/// bound is harmless on wasm since the decoder is never held across an await.
pub trait ChannelStreamDecoder: Send {
    /// Feed one raw upstream chunk; return decoded bytes (possibly empty while a
    /// frame is still buffering).
    fn push(&mut self, chunk: &[u8]) -> Vec<u8>;
    /// Flush any trailing buffered state at end of stream.
    fn finish(&mut self) -> Vec<u8>;
}

/// Per-call inputs the channel needs to build the upstream request.
///
/// The `'a` fields borrow snapshot-owned data; `body` is owned and is **moved**
/// into the constructed request by `prepare` (channel impls must move it).
pub struct PrepareCtx<'a> {
    /// Decrypted secret material (M1: plaintext; envelope decryption in M6).
    pub secret: &'a Value,
    /// Provider settings (`base_url`, channel toggles, …).
    pub provider_settings: &'a Value,
    /// Member rewrite target. PATH construction only (path-templated
    /// providers); body model rewrite happens in the pipeline transform
    /// step before prepare.
    pub upstream_model_id: &'a str,
    pub method: http::Method,
    /// Inbound, provider-relative path (`/v1/...`); scoped mode already stripped
    /// of the leading `/{provider}`.
    pub path: &'a str,
    pub query: Option<&'a str>,
    /// Inbound headers (the channel sanitizes + injects its own auth).
    pub headers: &'a HeaderMap,
    /// Effective upstream body (post-transform/process; verbatim on
    /// passthrough).
    pub body: Bytes,
}

/// Minimal context for upstream-body shaping (整形). Carries just enough for a
/// channel to dispatch per-operation without coupling to the request/pipeline
/// internals (v1 passed the whole request; v2 passes only this).
#[derive(Debug, Clone, Copy)]
pub struct ShapeCtx {
    /// The routed upstream (target) operation + protocol family.
    pub op: crate::protocol::OperationKey,
    /// Inbound client stream intent (response shaping only).
    pub stream: bool,
    /// Upstream status (response shaping only; `OK` for request shaping).
    pub status: StatusCode,
    /// Provider `enable_magic_cache` setting — request shaping only; gates the
    /// opt-in magic-string cache triggers on Claude-format bodies.
    pub enable_magic_cache: bool,
}

/// Pure upstream access adapter (§6.3). Implementors provide `id`,
/// `provider_family`, `routing_table` and `prepare`; the rest have sensible
/// defaults.
#[cfg_attr(not(target_arch = "wasm32"), async_trait::async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait::async_trait(?Send))]
pub trait Channel: Send + Sync {
    /// Stable channel id used as the registry key (matches `Provider.channel`).
    fn id(&self) -> &'static str;

    /// The provider family this channel's upstream belongs to (billing/usage).
    fn provider_family(&self) -> crate::protocol::Provider;

    /// The channel's explicit routing surface (ported from its capabilities).
    fn routing_table(&self) -> crate::channel::routes::RouteList;

    /// Inject auth, resolve endpoint + method, set an ABSOLUTE upstream URL.
    /// Pure access — no transform/rules, no body mutation. Moves `ctx.body` in.
    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError>;

    /// Map an upstream response to the 5-state [`Disposition`]. Default is the
    /// generic HTTP-status mapping; override only for provider-specific signals.
    /// For streaming, `body` is empty (status + headers suffice).
    fn classify(&self, status: StatusCode, headers: &HeaderMap, _body: &Bytes) -> Disposition {
        Disposition::from_http(status, headers)
    }

    /// Channel-specific REQUEST-body shaping (整形): runs after protocol
    /// transform + process rules, before [`prepare`](Channel::prepare). Pure
    /// field hygiene (strip unsupported fields, cap/rename, role/tools
    /// normalize, remove header tokens). Default: identity.
    fn shape_request(&self, body: Bytes, _headers: &mut HeaderMap, _ctx: &ShapeCtx) -> Bytes {
        body
    }

    /// Channel-specific RESPONSE-body shaping (整形) on the raw buffered upstream
    /// body, before protocol transform. Operation-aware via `ctx` so a channel
    /// can reshape model lists, fix non-standard fields, unwrap envelopes, etc.
    /// Runs on ALL statuses (error bodies included). Default: identity.
    fn shape_response(&self, body: Bytes, _ctx: &ShapeCtx) -> Bytes {
        body
    }

    /// A channel-bundled static model catalogue, for channels whose upstream
    /// exposes no model-list endpoint (e.g. vertexexpress). When `Some`, the
    /// admin model-pull returns it directly — no credential / upstream call. The
    /// body is in the channel family's canonical model-list wire shape. Default:
    /// none.
    fn bundled_models(&self) -> Option<Bytes> {
        None
    }

    /// Optional channel-specific stream decoder (envelope unwrap / binary →
    /// SSE), applied to the raw upstream byte stream before any protocol
    /// transform. Default: none (passthrough).
    fn stream_decoder(&self) -> Option<Box<dyn ChannelStreamDecoder>> {
        None
    }

    /// Whether the DECRYPTED secret must be refreshed before use (e.g. OAuth
    /// access token near expiry). Default: never.
    fn needs_refresh(&self, _secret: &Value) -> bool {
        false
    }

    /// Refresh the credential against the provider, returning the new PLAINTEXT
    /// secret Value. The pipeline re-seals + persists + publishes — the channel
    /// never touches cipher/persistence (purity §6.3). Default: unsupported.
    async fn refresh(
        &self,
        _client: &Arc<dyn UpstreamClient>,
        _secret: &Value,
    ) -> Result<Value, ChannelError> {
        Err(ChannelError::Unsupported("refresh"))
    }

    fn transport(&self) -> TransportKind {
        TransportKind::Http
    }

    /// Build a request to this channel's per-credential upstream usage / quota
    /// endpoint, given an already-fresh decrypted `secret` and provider
    /// `settings`. `None` (the default) means the channel exposes no usage
    /// endpoint (api-key / vertex channels). The driver sends it through the
    /// credential's resolved client (same proxy + TLS profile as traffic) and
    /// feeds the response to [`parse_usage`](Channel::parse_usage). Pure access:
    /// no persistence, no body shaping beyond what the endpoint needs.
    fn prepare_usage_request(
        &self,
        _secret: &Value,
        _settings: &Value,
    ) -> Result<Option<http::Request<Bytes>>, ChannelError> {
        Ok(None)
    }

    /// Parse this channel's usage-endpoint response into the normalized
    /// [`UsageSnapshot`]. Called only with the response to the request from
    /// [`prepare_usage_request`](Channel::prepare_usage_request). `None` on a
    /// non-success status or an unparseable body.
    fn parse_usage(
        &self,
        _status: StatusCode,
        _headers: &HeaderMap,
        _body: &Bytes,
    ) -> Option<UsageSnapshot> {
        None
    }

    /// Built-in TLS + HTTP/2 impersonation profile for this channel (§7.4),
    /// applied when no DB `tls_fingerprint` (credential/provider) overrides it.
    /// `None` (the default) means no built-in profile — the default client.
    /// Impersonation channels build it from `wreq` typed options in their own
    /// `fingerprint.rs`. Native + `upstream-wreq` only.
    #[cfg(all(not(target_arch = "wasm32"), feature = "upstream-wreq"))]
    fn default_emulation(&self) -> Option<wreq::Emulation> {
        None
    }
}

/// Errors raised while preparing an upstream request.
#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    #[error("missing setting: {0}")]
    MissingSetting(&'static str),
    #[error("invalid credential: {0}")]
    InvalidCredential(String),
    #[error("unsupported: {0}")]
    Unsupported(&'static str),
    #[error("build error: {0}")]
    Build(String),
}
