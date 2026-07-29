//! Core channel adapter contract.

use std::sync::Arc;

use bytes::Bytes;
use http::{HeaderMap, StatusCode};
use serde_json::Value;

use crate::context::{PrepareCtx, RefreshCtx, ShapeCtx, TransportKind};
use crate::disposition::Disposition;
use crate::error::ChannelError;
use crate::metadata::ChannelMetadata;
use crate::prepared::PreparedRequest;
use crate::transport::{ByteStreamDecoder as ChannelStreamDecoder, UpstreamClient};
use crate::usage::{RateLimitResetCreditConsumeResponse, UsageSnapshot};

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

    /// Metadata for runtime discovery and generic configuration UIs.
    fn metadata(&self) -> ChannelMetadata {
        ChannelMetadata::new(self.id(), self.provider_family())
    }

    /// The channel's explicit routing surface (ported from its capabilities).
    fn routing_table(&self) -> crate::routes::RouteList;

    /// Inject auth, resolve endpoint + method, set an ABSOLUTE upstream URL.
    /// Pure access — no transform/rules, no body mutation. Moves `ctx.body` in.
    fn prepare(&self, ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError>;

    /// Map an upstream response to the 5-state [`Disposition`]. Default is the
    /// generic HTTP-status mapping; override only for provider-specific signals.
    /// For streaming, `body` is empty (status + headers suffice).
    fn classify(&self, status: StatusCode, headers: &HeaderMap, _body: &Bytes) -> Disposition {
        Disposition::from_http(status, headers)
    }

    /// Whether a model-bound auth rejection (401/402/403) kills the WHOLE
    /// credential rather than only the exact (credential, model) pair. `true`
    /// for subscription-account channels (codex, claudecode) whose token is
    /// account-wide. Default: model-scoped.
    fn credential_wide_auth(&self) -> bool {
        false
    }

    /// Whether first-time cookie exchange must use the native browser profile.
    fn cookie_login_requires_browser(&self) -> bool {
        false
    }

    /// Whether refreshing this secret must use the native browser profile.
    fn refresh_requires_browser(&self, _secret: &Value) -> bool {
        false
    }

    /// Whether this model draws from the channel's account-wide MAIN quota
    /// pool. The main limit governs the whole account, so a 429 here cools the
    /// WHOLE credential (separate-limit models included). Models with an
    /// ADDITIONAL scoped limit on top of the main pool (codex spark, claude
    /// fable) return `false`: their own 429 means only the scoped limit is hit
    /// and stays model-scoped. Default: `false` (per-model quota, api-key
    /// channels).
    fn shares_account_quota(&self, _upstream_model_id: &str) -> bool {
        false
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

    /// A credential-scoped model catalogue discovered while authenticating or
    /// refreshing the secret. Unlike [`bundled_models`](Self::bundled_models),
    /// this hook is evaluated only after the credential has been decrypted and
    /// refreshed, so account-specific catalogues can be returned without an
    /// extra upstream model-list request. Default: none.
    fn credential_models(&self, _secret: &Value) -> Option<Bytes> {
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
        _ctx: RefreshCtx<'_>,
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

    /// Build a request to consume one earned rate-limit reset credit. Only
    /// channels whose upstream exposes this account action return a request.
    fn prepare_rate_limit_reset_credit_request(
        &self,
        _secret: &Value,
        _settings: &Value,
        _idempotency_key: &str,
    ) -> Result<Option<http::Request<Bytes>>, ChannelError> {
        Ok(None)
    }

    /// Parse the response from
    /// [`prepare_rate_limit_reset_credit_request`](Self::prepare_rate_limit_reset_credit_request).
    fn parse_rate_limit_reset_credit(
        &self,
        _status: StatusCode,
        _headers: &HeaderMap,
        _body: &Bytes,
    ) -> Option<RateLimitResetCreditConsumeResponse> {
        None
    }
}
