use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;

use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};

use crate::dispatch::DispatchTable;
use crate::health::CredentialHealth;
use crate::request::PreparedRequest;
use crate::response::{ResponseClassification, UpstreamError};

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

    /// Default dispatch table mapping (operation, protocol) → route strategy.
    fn dispatch_table(&self) -> DispatchTable;

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
    fn handle_local(
        &self,
        _operation: OperationFamily,
        _protocol: ProtocolKind,
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

/// Channel configuration (base URL, user agent, retry, etc.).
pub trait ChannelSettings:
    Send + Sync + Clone + Default + Serialize + DeserializeOwned + 'static
{
    fn base_url(&self) -> &str;
    fn user_agent(&self) -> Option<&str> {
        None
    }
    /// Max retries per credential on 429 without retry-after header.
    fn max_retries_on_429(&self) -> u32 {
        3
    }
    /// Enable cache affinity for this channel.
    /// When true, credentials are selected randomly and then pinned via the
    /// affinity pool so that requests with similar prompts reuse the same
    /// credential (maximising upstream prompt-cache hits).
    /// When false (default), credentials are selected by round-robin.
    fn enable_cache_affinity(&self) -> bool {
        false
    }
    /// Regex-based sanitization rules applied to request body text
    /// before forwarding upstream. The engine calls this after
    /// `finalize_request` and dispatches to the correct protocol
    /// walker based on the destination protocol.
    fn sanitize_rules(&self) -> &[crate::utils::sanitize::SanitizeRule] {
        &[]
    }
    /// Whether model suffix processing (thinking, speed, effort, etc.) is
    /// enabled for this channel. When `false`, no suffix matching, stripping,
    /// or rewriting happens. Default: `false`.
    fn enable_suffix(&self) -> bool {
        false
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
