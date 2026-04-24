use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;

use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

use crate::health::CredentialHealth;
use crate::request::PreparedRequest;
use crate::response::{ResponseClassification, UpstreamError};
use crate::routing::RoutingTable;

/// Boxed future type for async OAuth methods.
type OAuthFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<Option<T>, UpstreamError>> + Send + 'a>>;

/// Core abstraction for an upstream LLM API provider channel.
///
/// Each channel (OpenAI, Anthropic, Gemini, etc.) implements this trait once.
/// Registration is automatic via `inventory::submit!`.
pub trait Channel: Send + Sync + 'static {
    /// Unique channel identifier (e.g. "openai", "anthropic").
    const ID: &'static str;

    /// Channel-specific configuration.
    type Settings: ChannelSettings;
    /// Channel-specific credential (API key, OAuth tokens, etc.).
    type Credential: ChannelCredential;
    /// Channel-specific health tracking shape.
    type Health: CredentialHealth;

    /// Default routing table mapping (operation, protocol) → route strategy.
    fn routing_table(&self) -> RoutingTable;

    /// Channel-owned default pricing table.
    fn model_pricing(&self) -> &'static [crate::billing::ModelPrice] {
        &[]
    }

    /// Build an HTTP request from credential + settings + prepared request.
    fn prepare_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
        request: &PreparedRequest,
    ) -> Result<http::Request<Vec<u8>>, UpstreamError>;

    /// Finalize the semantic upstream request after protocol transform but
    /// before credential selection and HTTP transport wrapping.
    ///
    /// This is the right place for protocol/body normalization that should be
    /// visible to routing or cache-affinity logic. Transport-specific wrapping
    /// (auth headers, private envelopes, request ids) should remain in
    /// `prepare_request`.
    fn finalize_request(
        &self,
        _settings: &Self::Settings,
        request: PreparedRequest,
    ) -> Result<PreparedRequest, UpstreamError> {
        Ok(request)
    }

    /// Classify an upstream response to decide retry behavior.
    fn classify_response(
        &self,
        status: u16,
        headers: &http::HeaderMap,
        body: &[u8],
    ) -> ResponseClassification;

    /// Normalize the upstream response body (fix non-standard fields, etc.).
    /// Called before usage extraction and protocol transform.
    /// Default: no-op, return body as-is.
    fn normalize_response(&self, _request: &PreparedRequest, body: Vec<u8>) -> Vec<u8> {
        body
    }

    /// Token counting strategy for this channel.
    /// Default: local (tiktoken for GPT, DeepSeek fallback for others).
    fn count_strategy(&self) -> crate::count_tokens::CountStrategy {
        crate::count_tokens::CountStrategy::Local
    }

    /// Handle a local route (no upstream call). Returns None if not supported.
    ///
    /// `query` carries the downstream URL query string verbatim (no leading
    /// `?`) — local handlers use this for pagination and other URL-level
    /// parameters. `model` is the target model identifier (populated from
    /// path params / body / alias resolution by the handler). `body` carries
    /// the HTTP body.
    fn handle_local(
        &self,
        _operation: OperationFamily,
        _protocol: ProtocolKind,
        _model: Option<&str>,
        _query: Option<&str>,
        _body: &[u8],
    ) -> Option<Result<Vec<u8>, UpstreamError>> {
        None
    }

    /// Whether this credential requires the spoof (browser-impersonating) HTTP
    /// client.  Channels that use cookie-based auth (e.g. Claude Code with a
    /// session cookie) return `true` for those credentials.
    /// Default: `false`.
    fn needs_spoof_client(&self, _credential: &Self::Credential) -> bool {
        false
    }

    /// Extra headers to add to WebSocket handshake requests.
    /// Override for channels that require beta headers for WS (e.g. Codex).
    /// Default: empty.
    fn ws_extra_headers(&self) -> http::HeaderMap {
        http::HeaderMap::new()
    }

    /// Whether a credential is known-stale and should be refreshed
    /// **before** the next request goes out, instead of waiting for the
    /// upstream 401 → refresh → retry round-trip.
    ///
    /// Channels that track absolute token expiry (e.g. claudecode's
    /// `expires_at_ms`) override this to return `true` when the token
    /// is empty, already past the expiry, or within a small skew
    /// window — the retry loop then calls `refresh_credential`
    /// pre-flight.
    ///
    /// Default: `false` (always send optimistically, react to 401).
    fn needs_refresh(&self, _credential: &Self::Credential) -> bool {
        false
    }

    /// Attempt to refresh a credential after an auth failure (401/403).
    /// Called when upstream returns AuthDead. Returns `true` if the credential
    /// was updated and the request should be retried once more.
    /// Default: no refresh capability, returns `false`.
    fn refresh_credential<'a>(
        &'a self,
        _client: &'a wreq::Client,
        _credential: &'a mut Self::Credential,
    ) -> impl Future<Output = Result<bool, UpstreamError>> + Send + 'a {
        async { Ok(false) }
    }

    /// Build an HTTP request to query the upstream provider's quota / usage
    /// information for a given credential.
    ///
    /// Returns `None` for channels that do not expose a quota endpoint (the
    /// default).  OAuth channels typically override this.
    fn prepare_quota_request(
        &self,
        _credential: &Self::Credential,
        _settings: &Self::Settings,
    ) -> Result<Option<http::Request<Vec<u8>>>, UpstreamError> {
        Ok(None)
    }

    /// Start an OAuth flow (optional, most channels return None).
    fn oauth_start<'a>(
        &'a self,
        _client: &'a wreq::Client,
        _settings: &'a Self::Settings,
        _params: &'a BTreeMap<String, String>,
    ) -> OAuthFuture<'a, OAuthFlow> {
        Box::pin(async { Ok(None) })
    }

    fn oauth_finish<'a>(
        &'a self,
        _client: &'a wreq::Client,
        _settings: &'a Self::Settings,
        _params: &'a BTreeMap<String, String>,
    ) -> OAuthFuture<'a, OAuthCredentialResult<Self::Credential>> {
        Box::pin(async { Ok(None) })
    }
}

/// Common fields shared by every `ChannelSettings` implementation.
///
/// Every concrete channel needs to configure the same four things:
/// a request user-agent override, a 429-retry cap, a set of regex
/// sanitize rules, and a set of JSON-path rewrite rules. Historically
/// each of the 14 channels declared those four fields inline in its own
/// `XxxSettings` struct and wrote four identical `ChannelSettings` trait
/// method overrides — ~100 lines of boilerplate duplicated across the
/// crate.
///
/// `CommonChannelSettings` is the single home for those fields.
/// Channels embed it via `#[serde(flatten)]` so the TOML / JSON wire
/// format is unchanged from before the split, and implement the new
/// [`ChannelSettings::common`] hook to return a reference. The default
/// impls of `user_agent` / `max_retries_on_429` / `sanitize_rules` /
/// `rewrite_rules` then delegate to it automatically.
///
/// `base_url` is **not** part of this struct because every channel has
/// a different default URL and needs its own `#[serde(default = "...")]`
/// hook — it stays on the outer per-channel struct.
/// Credential selection strategy across a provider's credential list.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RotationStrategy {
    /// Always start from the first available credential. Falls through to
    /// the next one only when the chosen credential is unavailable or
    /// errors out. Effectively a primary-with-hot-backups mode.
    Sticky,
    /// Deterministic round-robin across all available credentials (the
    /// default). Each request advances a shared cursor by one.
    #[default]
    RoundRobin,
    /// Round-robin base ordering, but steer each request to the credential
    /// that most recently served a similar prompt prefix (maximises upstream
    /// prompt-cache hits). Requires the destination protocol to support
    /// cache affinity hinting.
    CacheAffinity,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CommonChannelSettings {
    /// Override the User-Agent header. `None` = use wreq's default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,

    /// Max retries per credential on 429 responses that omit
    /// `retry-after`. `None` = use the trait default (currently 3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_retries_on_429: Option<u32>,

    /// Credential rotation strategy. See [`RotationStrategy`].
    #[serde(default)]
    pub rotation_strategy: RotationStrategy,

    /// Regex-based body-text sanitization rules applied after
    /// `finalize_request`. See [`crate::utils::sanitize`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sanitize_rules: Vec<crate::utils::sanitize::SanitizeRule>,

    /// JSON-path body rewrite rules applied after sanitize. See
    /// [`crate::utils::rewrite`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rewrite_rules: Vec<crate::utils::rewrite::RewriteRule>,
}

/// Channel configuration (base URL, user agent, retry, etc.).
pub trait ChannelSettings:
    Send + Sync + Clone + Default + Serialize + DeserializeOwned + 'static
{
    fn base_url(&self) -> &str;

    /// Return the embedded [`CommonChannelSettings`] block, if any.
    ///
    /// Channels that use the `#[serde(flatten)] pub common:
    /// CommonChannelSettings` pattern override this to return
    /// `Some(&self.common)`, and the trait's default impls of
    /// [`user_agent`], [`max_retries_on_429`], [`sanitize_rules`], and
    /// [`rewrite_rules`] delegate to it automatically.
    ///
    /// Channels that pre-date the `CommonChannelSettings` refactor (or
    /// that want per-field custom behaviour) can leave this at its
    /// default `None` return and keep overriding individual getters
    /// directly.
    fn common(&self) -> Option<&CommonChannelSettings> {
        None
    }

    fn user_agent(&self) -> Option<&str> {
        self.common().and_then(|c| c.user_agent.as_deref())
    }
    /// Max retries per credential on 429 without retry-after header.
    fn max_retries_on_429(&self) -> u32 {
        self.common()
            .and_then(|c| c.max_retries_on_429)
            .unwrap_or(3)
    }
    /// Credential selection strategy. Defaults to the value in
    /// [`CommonChannelSettings::rotation_strategy`] (round-robin when the
    /// channel has no common block).
    fn rotation_strategy(&self) -> RotationStrategy {
        self.common()
            .map(|c| c.rotation_strategy)
            .unwrap_or_default()
    }
    /// Regex-based sanitization rules applied to request body text
    /// before forwarding upstream. The engine calls this after
    /// `finalize_request` and dispatches to the correct protocol
    /// walker based on the destination protocol.
    fn sanitize_rules(&self) -> &[crate::utils::sanitize::SanitizeRule] {
        self.common()
            .map(|c| c.sanitize_rules.as_slice())
            .unwrap_or(&[])
    }
    /// JSON-path rewrite rules applied to the request body before
    /// `finalize_request`. Rules are executed in declaration order.
    fn rewrite_rules(&self) -> &[crate::utils::rewrite::RewriteRule] {
        self.common()
            .map(|c| c.rewrite_rules.as_slice())
            .unwrap_or(&[])
    }
}

/// Channel credential (API key, OAuth token, etc.).
pub trait ChannelCredential: Send + Sync + Clone + Serialize + DeserializeOwned + 'static {
    /// Apply an upstream credential update (e.g. OAuth token refresh).
    /// Returns true if the update was applied.
    fn apply_update(&mut self, _update: &serde_json::Value) -> bool {
        false
    }
}

/// Placeholder for OAuth flow data.
#[derive(Debug, Clone)]
pub struct OAuthFlow {
    pub authorize_url: String,
    pub state: String,
    pub redirect_uri: Option<String>,
    pub verification_uri: Option<String>,
    pub user_code: Option<String>,
    pub mode: Option<String>,
    pub scope: Option<String>,
    pub instructions: Option<String>,
}

#[derive(Debug, Clone)]
pub struct OAuthCredentialResult<C> {
    pub credential: C,
    pub details: Value,
}
