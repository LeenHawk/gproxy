use std::collections::BTreeMap;
use std::sync::OnceLock;

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::channel::{
    Channel, ChannelCredential, ChannelSettings, CommonChannelSettings, OAuthCredentialResult,
    OAuthFlow,
};
use crate::count_tokens::CountStrategy;
use crate::health::ModelCooldownHealth;
use crate::registry::ChannelRegistration;
use crate::request::PreparedRequest;
use crate::response::{ResponseClassification, UpstreamError};
use crate::routing::{RouteImplementation, RouteKey, RoutingTable};
use crate::utils::claude_cache_control as cache_control;
use crate::utils::claude_sampling;
use gproxy_protocol::kinds::{OperationFamily, ProtocolKind};
use tracing::Instrument;

/// Claude Code channel (Anthropic Messages API with OAuth).
pub struct ClaudeCodeChannel;

const DEFAULT_CLAUDECODE_VERSION: &str = "2.1.112";
const DEFAULT_CLAUDECODE_ENTRYPOINT: &str = "cli";
const DEFAULT_CLAUDECODE_USER_TYPE: &str = "external";
// Anthropic JS SDK (Stainless-generated) default header values, matching the
// ones observed on real `claude-cli` 2.1.112 + `@anthropic-ai/sdk@0.81.0`
// traffic. All are overridable via `ClaudeCodeSettings::fingerprint`.
const DEFAULT_STAINLESS_LANG: &str = "js";
const DEFAULT_STAINLESS_PACKAGE_VERSION: &str = "0.81.0";
const DEFAULT_STAINLESS_RUNTIME: &str = "node";
const DEFAULT_STAINLESS_RUNTIME_VERSION: &str = "v22.20.0";
const DEFAULT_STAINLESS_OS: &str = "Linux";
const DEFAULT_STAINLESS_ARCH: &str = "x64";
const DEFAULT_STAINLESS_TIMEOUT_SECS: &str = "600";
const BILLING_HEADER_PREFIX: &str = "x-anthropic-billing-header:";
const BILLING_HASH_SALT: &str = "59cf53e54c78";
const BILLING_VERSION_HASH_LEN: usize = 3;
const BILLING_VERSION_CHAR_OFFSETS: [usize; 3] = [4, 7, 20];
/// Session IDs rotate after this many milliseconds, approximating the median
/// lifetime of a real `claude` CLI process (~20 minutes of active use).
const SESSION_ID_TTL_MS: u64 = 20 * 60 * 1000;
const CLAUDECODE_CLAUDE_AI_BASE_URL: &str = "https://claude.ai";
const CLAUDECODE_REDIRECT_URI: &str = "https://platform.claude.com/oauth/code/callback";
const CLAUDECODE_OAUTH_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const CLAUDECODE_OAUTH_SCOPE: &str =
    "user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload";
const CLAUDECODE_OAUTH_BETA: &str = "oauth-2025-04-20";
const CLAUDECODE_API_VERSION: &str = "2023-06-01";
const CLAUDECODE_OAUTH_STATE_TTL_MS: u64 = 600_000;
const CLAUDECODE_TOKEN_UA: &str = "claude-cli/2.1.112 (external, cli)";
const CLAUDECODE_PROFILE_UA: &str = "claude-code/2.1.112";

/// Per-credential session ID cache.  Key = device_id, value = (session_id, created_at_ms).
/// Follows the same static-DashMap pattern used by `claudecode_oauth_states()`.
fn claudecode_session_cache() -> &'static DashMap<String, (String, u64)> {
    static CACHE: OnceLock<DashMap<String, (String, u64)>> = OnceLock::new();
    CACHE.get_or_init(DashMap::new)
}

fn claudecode_model_pricing() -> &'static [crate::billing::ModelPrice] {
    static PRICING: OnceLock<Vec<crate::billing::ModelPrice>> = OnceLock::new();
    PRICING.get_or_init(|| {
        crate::billing::parse_model_prices_json(include_str!("pricing/claudecode.json"))
    })
}

#[derive(Debug, Clone)]
struct ClaudeCodeOAuthState {
    code_verifier: String,
    redirect_uri: String,
    api_base_url: String,
    claude_ai_base_url: String,
    created_at_unix_ms: u64,
}

#[derive(Debug, Deserialize)]
struct ClaudeCodeTokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    #[serde(default)]
    rate_limit_tier: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ClaudeCodeOAuthProfileAccount {
    uuid: Option<String>,
    email: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ClaudeCodeOAuthProfileOrg {
    rate_limit_tier: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ClaudeCodeOAuthProfile {
    #[serde(default)]
    account: ClaudeCodeOAuthProfileAccount,
    #[serde(default)]
    organization: ClaudeCodeOAuthProfileOrg,
}

fn claudecode_oauth_states() -> &'static DashMap<String, ClaudeCodeOAuthState> {
    static STATES: OnceLock<DashMap<String, ClaudeCodeOAuthState>> = OnceLock::new();
    STATES.get_or_init(DashMap::new)
}

fn prune_claudecode_oauth_states(now_unix_ms: u64) {
    let expired = claudecode_oauth_states()
        .iter()
        .filter_map(|entry| {
            (now_unix_ms.saturating_sub(entry.value().created_at_unix_ms)
                > CLAUDECODE_OAUTH_STATE_TTL_MS)
                .then(|| entry.key().clone())
        })
        .collect::<Vec<_>>();
    for key in expired {
        claudecode_oauth_states().remove(key.as_str());
    }
}

fn build_claudecode_authorize_url(
    claude_ai_base_url: &str,
    redirect_uri: &str,
    scope: &str,
    code_challenge: &str,
    state: &str,
) -> String {
    let query = [
        ("code", "true".to_string()),
        ("client_id", CLAUDECODE_OAUTH_CLIENT_ID.to_string()),
        ("response_type", "code".to_string()),
        ("redirect_uri", redirect_uri.to_string()),
        ("scope", scope.to_string()),
        ("code_challenge", code_challenge.to_string()),
        ("code_challenge_method", "S256".to_string()),
        ("state", state.to_string()),
    ]
    .into_iter()
    .map(|(key, value)| format!("{key}={}", crate::utils::oauth::percent_encode(&value)))
    .collect::<Vec<_>>()
    .join("&");
    format!(
        "{}/oauth/authorize?{query}",
        claude_ai_base_url.trim_end_matches('/')
    )
}

async fn exchange_claudecode_code_for_tokens(
    client: &wreq::Client,
    api_base_url: &str,
    claude_ai_base_url: &str,
    redirect_uri: &str,
    code_verifier: &str,
    code: &str,
    state: &str,
) -> Result<ClaudeCodeTokenResponse, UpstreamError> {
    let body = format!(
        "grant_type=authorization_code&client_id={}&code={}&redirect_uri={}&code_verifier={}&state={}",
        crate::utils::oauth::percent_encode(CLAUDECODE_OAUTH_CLIENT_ID),
        crate::utils::oauth::percent_encode(code),
        crate::utils::oauth::percent_encode(redirect_uri),
        crate::utils::oauth::percent_encode(code_verifier),
        crate::utils::oauth::percent_encode(state),
    );
    let response = client
        .post(format!(
            "{}/v1/oauth/token",
            api_base_url.trim_end_matches('/')
        ))
        .header("anthropic-version", CLAUDECODE_API_VERSION)
        .header("anthropic-beta", CLAUDECODE_OAUTH_BETA)
        .header("content-type", "application/x-www-form-urlencoded")
        .header("accept", "application/json, text/plain, */*")
        .header("origin", claude_ai_base_url)
        .header("user-agent", CLAUDECODE_TOKEN_UA)
        .body(body)
        .send()
        .await
        .map_err(|e| UpstreamError::Http(format!("claudecode oauth token: {e}")))?;
    let status = response.status().as_u16();
    let bytes = response
        .bytes()
        .await
        .map_err(|e| UpstreamError::Http(format!("claudecode oauth body: {e}")))?;
    if !(200..300).contains(&status) {
        return Err(UpstreamError::Channel(format!(
            "claudecode oauth token endpoint status {status}: {}",
            String::from_utf8_lossy(&bytes)
        )));
    }
    serde_json::from_slice(&bytes)
        .map_err(|e| UpstreamError::Channel(format!("claudecode oauth token parse: {e}")))
}

async fn fetch_claudecode_oauth_profile(
    client: &wreq::Client,
    api_base_url: &str,
    access_token: &str,
) -> Result<ClaudeCodeOAuthProfile, UpstreamError> {
    let response = client
        .get(format!(
            "{}/api/oauth/profile",
            api_base_url.trim_end_matches('/')
        ))
        .header("authorization", format!("Bearer {access_token}"))
        .header("user-agent", CLAUDECODE_PROFILE_UA)
        .header("accept", "application/json")
        .header("anthropic-beta", CLAUDECODE_OAUTH_BETA)
        .send()
        .await
        .map_err(|e| UpstreamError::Http(format!("claudecode oauth profile: {e}")))?;
    let status = response.status().as_u16();
    let bytes = response
        .bytes()
        .await
        .map_err(|e| UpstreamError::Http(format!("claudecode oauth profile body: {e}")))?;
    if !(200..300).contains(&status) {
        return Err(UpstreamError::Channel(format!(
            "claudecode oauth profile status {status}: {}",
            String::from_utf8_lossy(&bytes)
        )));
    }
    serde_json::from_slice(&bytes)
        .map_err(|e| UpstreamError::Channel(format!("claudecode oauth profile parse: {e}")))
}

// ---------------------------------------------------------------------------
// Default-value helpers
// ---------------------------------------------------------------------------

fn default_claudecode_base_url() -> String {
    "https://api.anthropic.com".to_string()
}

fn default_claudecode_platform_base_url() -> String {
    "https://platform.claude.com".to_string()
}

fn default_claudecode_claude_ai_base_url() -> String {
    "https://claude.ai".to_string()
}

fn default_claudecode_device_id() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("device_id entropy should be available");
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

// ---------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClaudeCodeSettings {
    #[serde(default = "default_claudecode_base_url")]
    pub base_url: String,
    /// Base URL for the platform API (quota / usage endpoint).
    /// Defaults to `https://platform.claude.com`.
    #[serde(default = "default_claudecode_platform_base_url")]
    pub platform_base_url: String,
    /// Base URL for claude.ai (cookie auth, organization discovery).
    /// Defaults to `https://claude.ai`.
    #[serde(default = "default_claudecode_claude_ai_base_url")]
    pub claude_ai_base_url: String,
    #[serde(default)]
    pub enable_magic_cache: bool,
    /// Merge consecutive `system` text blocks into one before cache
    /// breakpoints are applied. Useful when clients split system into many
    /// small pieces that would otherwise fragment the cache.
    #[serde(default)]
    pub flatten_system_before_cache: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cache_breakpoints: Vec<cache_control::CacheBreakpointRule>,
    /// Optional system-prompt prelude injected as the first `system` block
    /// on every content-generation request. Useful for injecting
    /// organization-wide instructions or behavioral constraints.
    /// Skipped when the request already contains a matching prelude.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prelude_text: Option<String>,
    /// Additional `anthropic-beta` header values merged into every
    /// request. The OAuth beta token is always included regardless of
    /// this list. Values are deduplicated case-insensitively.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub extra_beta_headers: Vec<String>,
    /// Regex-based text sanitization rules applied to `system` and
    /// `messages[*].content` before forwarding upstream. See
    /// `utils::sanitize::SanitizeRule` for format.
    /// Common fields shared with every other channel: user_agent,
    /// max_retries_on_429, sanitize_rules, rewrite_rules. Flattened
    /// so the TOML / JSON wire format is unchanged.
    #[serde(flatten)]
    pub common: CommonChannelSettings,
    /// Optional overrides for the client fingerprint headers emitted on
    /// every upstream request (User-Agent components + the `x-stainless-*`
    /// family injected by `@anthropic-ai/sdk`). Leave fields `None` to
    /// fall back to the baked-in 2.1.112 / SDK 0.81.0 defaults.
    #[serde(default, skip_serializing_if = "ClaudeCodeFingerprint::is_empty")]
    pub fingerprint: ClaudeCodeFingerprint,
}

/// Overrides for the `claude-cli` / `@anthropic-ai/sdk` request fingerprint.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClaudeCodeFingerprint {
    /// CLI version embedded in the User-Agent, e.g. `"2.1.112"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cli_version: Option<String>,
    /// `USER_TYPE` segment of the User-Agent, e.g. `"external"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_type: Option<String>,
    /// Entrypoint segment of the User-Agent, e.g. `"cli"` or
    /// `"claude-vscode"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entrypoint: Option<String>,
    /// `x-stainless-lang` (default `"js"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stainless_lang: Option<String>,
    /// `x-stainless-package-version` — the `@anthropic-ai/sdk` npm version
    /// (default `"0.81.0"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stainless_package_version: Option<String>,
    /// `x-stainless-runtime` (default `"node"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stainless_runtime: Option<String>,
    /// `x-stainless-runtime-version` (default `"v22.20.0"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stainless_runtime_version: Option<String>,
    /// `x-stainless-os` (default `"Linux"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stainless_os: Option<String>,
    /// `x-stainless-arch` (default `"x64"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stainless_arch: Option<String>,
    /// `x-stainless-timeout` in seconds (default `"600"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stainless_timeout: Option<String>,
}

impl ClaudeCodeFingerprint {
    fn is_empty(&self) -> bool {
        self.cli_version.is_none()
            && self.user_type.is_none()
            && self.entrypoint.is_none()
            && self.stainless_lang.is_none()
            && self.stainless_package_version.is_none()
            && self.stainless_runtime.is_none()
            && self.stainless_runtime_version.is_none()
            && self.stainless_os.is_none()
            && self.stainless_arch.is_none()
            && self.stainless_timeout.is_none()
    }
}

impl ClaudeCodeSettings {
    fn cli_version(&self) -> &str {
        self.fingerprint
            .cli_version
            .as_deref()
            .unwrap_or(DEFAULT_CLAUDECODE_VERSION)
    }
    fn user_type(&self) -> &str {
        self.fingerprint
            .user_type
            .as_deref()
            .unwrap_or(DEFAULT_CLAUDECODE_USER_TYPE)
    }
    fn entrypoint(&self) -> &str {
        self.fingerprint
            .entrypoint
            .as_deref()
            .unwrap_or(DEFAULT_CLAUDECODE_ENTRYPOINT)
    }
    fn stainless_lang(&self) -> &str {
        self.fingerprint
            .stainless_lang
            .as_deref()
            .unwrap_or(DEFAULT_STAINLESS_LANG)
    }
    fn stainless_package_version(&self) -> &str {
        self.fingerprint
            .stainless_package_version
            .as_deref()
            .unwrap_or(DEFAULT_STAINLESS_PACKAGE_VERSION)
    }
    fn stainless_runtime(&self) -> &str {
        self.fingerprint
            .stainless_runtime
            .as_deref()
            .unwrap_or(DEFAULT_STAINLESS_RUNTIME)
    }
    fn stainless_runtime_version(&self) -> &str {
        self.fingerprint
            .stainless_runtime_version
            .as_deref()
            .unwrap_or(DEFAULT_STAINLESS_RUNTIME_VERSION)
    }
    fn stainless_os(&self) -> &str {
        self.fingerprint
            .stainless_os
            .as_deref()
            .unwrap_or(DEFAULT_STAINLESS_OS)
    }
    fn stainless_arch(&self) -> &str {
        self.fingerprint
            .stainless_arch
            .as_deref()
            .unwrap_or(DEFAULT_STAINLESS_ARCH)
    }
    fn stainless_timeout(&self) -> &str {
        self.fingerprint
            .stainless_timeout
            .as_deref()
            .unwrap_or(DEFAULT_STAINLESS_TIMEOUT_SECS)
    }
}

impl ChannelSettings for ClaudeCodeSettings {
    fn base_url(&self) -> &str {
        &self.base_url
    }
    fn common(&self) -> Option<&CommonChannelSettings> {
        Some(&self.common)
    }
}

// ---------------------------------------------------------------------------
// Credential
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClaudeCodeCredential {
    #[serde(default)]
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: String,
    #[serde(default)]
    pub expires_at_ms: u64,
    #[serde(default = "default_claudecode_device_id")]
    pub device_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account_uuid: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit_tier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cookie: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_email: Option<String>,
}

impl ChannelCredential for ClaudeCodeCredential {
    fn apply_update(&mut self, update: &serde_json::Value) -> bool {
        if let Some(token) = update.get("access_token").and_then(|v| v.as_str()) {
            self.access_token = token.to_string();
            if let Some(exp) = update.get("expires_at_ms").and_then(|v| v.as_u64()) {
                self.expires_at_ms = exp;
            }
            if let Some(rt) = update.get("refresh_token").and_then(|v| v.as_str()) {
                self.refresh_token = rt.to_string();
            }
            if let Some(account_uuid) = update.get("account_uuid").and_then(|v| v.as_str()) {
                self.account_uuid = Some(account_uuid.to_string());
            }
            if let Some(device_id) = update.get("device_id").and_then(|v| v.as_str()) {
                self.device_id = device_id.to_string();
            }
            true
        } else {
            false
        }
    }
}

// ---------------------------------------------------------------------------
// Body-mutation helpers
// ---------------------------------------------------------------------------

/// Build the `metadata.user_id` JSON string that Claude Code sends.
fn build_metadata_user_id(credential: &ClaudeCodeCredential, session_id: &str) -> String {
    // The value is itself a JSON-encoded string
    serde_json::json!({
        "device_id": credential.device_id.as_str(),
        "account_uuid": credential.account_uuid.as_deref().unwrap_or(""),
        "session_id": session_id,
    })
    .to_string()
}

/// Build the billing attribution text injected as the first system element.
///
/// Mirrors the real `claude-cli` 2.1.112 bundled behaviour: `cc_version` is
/// `VERSION.fingerprint` where
/// `fingerprint = sha256(SALT + msg[4] + msg[7] + msg[20] + VERSION)[..3]`
/// (see `upstream_docs/code/claude-code/src/utils/fingerprint.ts`), and
/// `cch` is the literal string `00000` — the compiled CLI hard-codes it
/// with no attestation post-processing, and the API accepts it as-is.
fn build_attribution(user_message: &str) -> String {
    let version_hash_input = format!(
        "{}{}{}",
        BILLING_HASH_SALT,
        sampled_message_chars(user_message),
        DEFAULT_CLAUDECODE_VERSION,
    );
    let version_hash = truncated_sha256_hex(&version_hash_input, BILLING_VERSION_HASH_LEN);

    format!(
        "x-anthropic-billing-header: cc_version={}.{}; cc_entrypoint={}; cch=00000;",
        DEFAULT_CLAUDECODE_VERSION, version_hash, DEFAULT_CLAUDECODE_ENTRYPOINT,
    )
}

fn request_session_id(
    request: &PreparedRequest,
    _body: &Value,
    credential: &ClaudeCodeCredential,
) -> String {
    // 1. Explicit session-id from upstream client takes priority.
    //    NOTE: `x-client-request-id` is intentionally NOT a fallback —
    //    it is per-request whereas session-id is process-lifetime in
    //    real Claude Code.
    if let Some(session_id) = request
        .headers
        .get("x-claude-code-session-id")
        .or_else(|| request.headers.get("session_id"))
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
    {
        return session_id.to_owned();
    }

    // 2. Credential-keyed v4 with TTL rotation.
    //    Real Claude Code generates one random v4 UUID at process start
    //    and reuses it for the process lifetime.  We approximate this
    //    with a 20-minute TTL keyed on `device_id`.
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let cache = claudecode_session_cache();
    let key = &credential.device_id;

    if let Some(entry) = cache.get(key) {
        let (ref sid, created) = *entry;
        if now_ms.saturating_sub(created) < SESSION_ID_TTL_MS {
            return sid.clone();
        }
    }

    let new_sid = Uuid::new_v4().to_string();
    cache.insert(key.clone(), (new_sid.clone(), now_ms));
    new_sid
}

fn truncated_sha256_hex(input: &str, hex_len: usize) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let hash = hasher.finalize();
    let mut hex = String::with_capacity(hash.len() * 2);
    for byte in hash {
        use std::fmt::Write as _;
        let _ = write!(&mut hex, "{byte:02x}");
    }
    hex.chars().take(hex_len).collect()
}

fn sampled_message_chars(user_message: &str) -> String {
    let chars: Vec<char> = user_message.chars().collect();
    BILLING_VERSION_CHAR_OFFSETS
        .iter()
        .map(|index| chars.get(*index).copied().unwrap_or('0'))
        .collect()
}

fn text_from_content_block(block: &Value) -> Option<String> {
    if let Some(text) = block.as_str() {
        return Some(text.to_owned());
    }

    let block_type = block.get("type").and_then(Value::as_str)?;
    if block_type != "text" {
        return None;
    }

    block
        .get("text")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
}

fn first_user_message_text(body: &Value) -> String {
    let Some(messages) = body.get("messages").and_then(Value::as_array) else {
        return String::new();
    };

    let Some(message) = messages.iter().find(|message| {
        message
            .get("role")
            .and_then(Value::as_str)
            .is_some_and(|role| role == "user")
    }) else {
        return String::new();
    };

    let Some(content) = message.get("content") else {
        return String::new();
    };

    match content {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter_map(text_from_content_block)
            .collect::<Vec<_>>()
            .join(""),
        _ => String::new(),
    }
}

fn normalize_claudecode_unsupported_fields(body: &mut Value) {
    let Some(map) = body.as_object_mut() else {
        return;
    };

    map.remove("speed");
}

/// Copy the token fields returned by `exchange_tokens_with_cookie` onto
/// an existing credential record, leaving untouched fields alone.
///
/// Shared by `refresh_credential` (the 401 retry path) and
/// `bootstrap_credential_from_cookie` (the admin upsert path) so both
/// stay in sync if new fields appear in `CookieTokenResponse`.
fn apply_cookie_exchange_tokens(
    credential: &mut ClaudeCodeCredential,
    tokens: crate::utils::claudecode_cookie::CookieTokenResponse,
) {
    let rate_limit = tokens.rate_limit_tier();
    if let Some(at) = tokens.access_token {
        credential.access_token = at;
    }
    if let Some(rt) = tokens.refresh_token {
        credential.refresh_token = rt;
    }
    if let Some(exp) = tokens.expires_in {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        credential.expires_at_ms = now_ms.saturating_add(exp.saturating_mul(1000));
    }
    if let Some(rlt) = rate_limit {
        credential.rate_limit_tier = Some(rlt);
    }
    if let Some(uuid) = tokens.account_uuid {
        credential.account_uuid = Some(uuid);
    }
    if let Some(email) = tokens.user_email {
        credential.user_email = Some(email);
    }
}

/// Bootstrap a claudecode credential on upsert by exchanging its
/// `cookie` for OAuth tokens before it lands in the DB.
///
/// The admin API calls this when a user creates or updates a claudecode
/// credential — if `cookie` is non-empty, we run the full claude.ai →
/// org discovery → authorize → token exchange flow here so the stored
/// credential already has a live `access_token` / `refresh_token` /
/// `expires_at_ms`. This avoids the "first request fails with 401, then
/// retries via refresh path" round-trip users would otherwise hit on
/// their very first request after creating a cookie-only credential.
///
/// Returns:
/// - `Ok(Some(updated_json))` when `cookie` was non-empty and the
///   exchange succeeded; `updated_json` is the credential to persist.
/// - `Ok(None)` when `cookie` is absent/empty — caller should store
///   the original JSON unchanged.
/// - `Err(...)` when the cookie was present but the exchange failed
///   (invalid cookie, Cloudflare blocked, Anthropic rejected, …).
///   The admin handler surfaces this as a `400 Bad Request` so the
///   operator sees the real cause immediately rather than after the
///   first chat request fails.
///
/// The exchange always runs when a cookie is present, even if
/// `access_token` is already populated. The user explicitly asked for
/// this — treating fresh upserts as authoritative keeps the stored
/// tokens aligned with the latest cookie rather than the possibly-stale
/// values the caller supplied.
pub async fn bootstrap_credential_from_cookie(
    http_client: &wreq::Client,
    spoof_client: Option<&wreq::Client>,
    credential_json: &Value,
) -> Result<
    (Option<Value>, Vec<crate::meta::UpstreamRequestMeta>),
    (UpstreamError, Vec<crate::meta::UpstreamRequestMeta>),
> {
    let mut credential: ClaudeCodeCredential = serde_json::from_value(credential_json.clone())
        .map_err(|e| {
            (
                UpstreamError::Channel(format!("invalid claudecode credential: {e}")),
                Vec::new(),
            )
        })?;

    let cookie = match credential.cookie.as_ref() {
        Some(c) if !c.is_empty() => c.clone(),
        _ => return Ok((None, Vec::new())),
    };

    // `exchange_tokens_with_cookie` talks to claude.ai, which is behind
    // Cloudflare and will reject plain wreq clients; prefer the
    // browser-impersonating spoof client when one is available and fall
    // back to the regular HTTP client otherwise. The spoof fallback
    // matches `ClaudeCodeChannel::needs_spoof_client`.
    let client = spoof_client.unwrap_or(http_client);

    tracing::info!("bootstrapping claudecode credential from cookie on upsert");
    let mut tracked = Vec::new();
    let tokens = match crate::utils::claudecode_cookie::exchange_tokens_with_cookie(
        client,
        &default_claudecode_base_url(),
        &default_claudecode_claude_ai_base_url(),
        &cookie,
        &mut tracked,
    )
    .await
    {
        Ok(tokens) => tokens,
        Err(e) => return Err((e, tracked)),
    };
    apply_cookie_exchange_tokens(&mut credential, tokens);

    let updated = serde_json::to_value(credential).map_err(|e| {
        (
            UpstreamError::Channel(format!("serialize bootstrapped credential: {e}")),
            tracked.clone(),
        )
    })?;
    Ok((Some(updated), tracked))
}

/// Prepend a prelude text block as the first element of the `system` array.
///
/// Skipped when the request body already contains a system block whose text
/// starts with the prelude (prevents double-injection on retries or when the
/// client already included it).
///
/// Handling mirrors `inject_system_attribution`: absent → create, string →
/// convert to array, array → insert(0).
fn apply_claudecode_prelude(body: &mut Value, prelude_text: &str) {
    let Some(map) = body.as_object_mut() else {
        return;
    };

    // Skip if any existing system block already starts with the prelude.
    if let Some(system) = map.get("system") {
        let starts_with_prelude = |v: &Value| -> bool {
            v.get("text")
                .and_then(Value::as_str)
                .is_some_and(|t| t.starts_with(prelude_text))
        };
        match system {
            Value::String(s) if s.starts_with(prelude_text) => return,
            Value::Array(blocks) if blocks.iter().any(starts_with_prelude) => return,
            _ => {}
        }
    }

    let prelude_block = serde_json::json!({ "type": "text", "text": prelude_text });

    match map.remove("system") {
        Some(Value::String(text)) => {
            let text_block = serde_json::json!({ "type": "text", "text": text });
            map.insert(
                "system".to_string(),
                Value::Array(vec![prelude_block, text_block]),
            );
        }
        Some(Value::Array(mut blocks)) => {
            blocks.insert(0, prelude_block);
            map.insert("system".to_string(), Value::Array(blocks));
        }
        Some(other) => {
            // Unexpected shape — keep as-is, don't inject.
            map.insert("system".to_string(), other);
        }
        None => {
            map.insert("system".to_string(), Value::Array(vec![prelude_block]));
        }
    }
}

/// Inject `metadata.user_id` into the body JSON.
fn inject_metadata_user_id(body: &mut Value, user_id_value: &str) {
    let metadata = body
        .as_object_mut()
        .expect("body must be an object")
        .entry("metadata")
        .or_insert_with(|| Value::Object(serde_json::Map::new()));
    if let Some(m) = metadata.as_object_mut() {
        m.insert(
            "user_id".to_string(),
            Value::String(user_id_value.to_string()),
        );
    }
}

/// Inject the billing attribution as the first element of the `system` array.
///
/// - If `system` is absent, create it as an array with the attribution text block.
/// - If `system` is a string, convert to an array: [attribution, original_text].
/// - If `system` is already an array, prepend the attribution text block.
fn inject_system_attribution(body: &mut Value, attribution: &str) {
    let attribution_block = serde_json::json!({
        "type": "text",
        "text": attribution,
    });

    let obj = body.as_object_mut().expect("body must be an object");

    match obj.get("system") {
        None => {
            obj.insert("system".to_string(), Value::Array(vec![attribution_block]));
        }
        Some(val) if val.is_string() => {
            let original_text = val.as_str().unwrap().to_string();
            if original_text.starts_with(BILLING_HEADER_PREFIX) {
                obj.insert("system".to_string(), Value::Array(vec![attribution_block]));
                return;
            }
            let original_block = serde_json::json!({
                "type": "text",
                "text": original_text,
            });
            obj.insert(
                "system".to_string(),
                Value::Array(vec![attribution_block, original_block]),
            );
        }
        Some(val) if val.is_array() => {
            let arr = obj.get_mut("system").unwrap().as_array_mut().unwrap();
            let first_is_billing = arr.first().is_some_and(|block| {
                block
                    .get("text")
                    .and_then(Value::as_str)
                    .or_else(|| block.as_str())
                    .is_some_and(|text| text.starts_with(BILLING_HEADER_PREFIX))
            });
            if first_is_billing {
                arr[0] = attribution_block;
            } else {
                arr.insert(0, attribution_block);
            }
        }
        _ => {
            // system is some other type – overwrite with array
            obj.insert("system".to_string(), Value::Array(vec![attribution_block]));
        }
    }
}

// ---------------------------------------------------------------------------
// Channel implementation
// ---------------------------------------------------------------------------

impl Channel for ClaudeCodeChannel {
    const ID: &'static str = "claudecode";
    type Settings = ClaudeCodeSettings;
    type Credential = ClaudeCodeCredential;
    type Health = ModelCooldownHealth;

    fn routing_table(&self) -> RoutingTable {
        let mut t = RoutingTable::new();
        let pass = |op: OperationFamily, proto: ProtocolKind| {
            (RouteKey::new(op, proto), RouteImplementation::Passthrough)
        };
        let xform = |op: OperationFamily,
                     proto: ProtocolKind,
                     dst_op: OperationFamily,
                     dst_proto: ProtocolKind| {
            (
                RouteKey::new(op, proto),
                RouteImplementation::TransformTo {
                    destination: RouteKey::new(dst_op, dst_proto),
                },
            )
        };

        let routes = vec![
            pass(OperationFamily::ModelList, ProtocolKind::Claude),
            xform(
                OperationFamily::ModelList,
                ProtocolKind::OpenAi,
                OperationFamily::ModelList,
                ProtocolKind::Claude,
            ),
            xform(
                OperationFamily::ModelList,
                ProtocolKind::Gemini,
                OperationFamily::ModelList,
                ProtocolKind::Claude,
            ),
            pass(OperationFamily::ModelGet, ProtocolKind::Claude),
            xform(
                OperationFamily::ModelGet,
                ProtocolKind::OpenAi,
                OperationFamily::ModelGet,
                ProtocolKind::Claude,
            ),
            xform(
                OperationFamily::ModelGet,
                ProtocolKind::Gemini,
                OperationFamily::ModelGet,
                ProtocolKind::Claude,
            ),
            pass(OperationFamily::CountToken, ProtocolKind::Claude),
            xform(
                OperationFamily::CountToken,
                ProtocolKind::OpenAi,
                OperationFamily::CountToken,
                ProtocolKind::Claude,
            ),
            xform(
                OperationFamily::CountToken,
                ProtocolKind::Gemini,
                OperationFamily::CountToken,
                ProtocolKind::Claude,
            ),
            pass(OperationFamily::GenerateContent, ProtocolKind::Claude),
            xform(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiChatCompletion,
                OperationFamily::GenerateContent,
                ProtocolKind::Claude,
            ),
            xform(
                OperationFamily::GenerateContent,
                ProtocolKind::OpenAiResponse,
                OperationFamily::GenerateContent,
                ProtocolKind::Claude,
            ),
            xform(
                OperationFamily::GenerateContent,
                ProtocolKind::Gemini,
                OperationFamily::GenerateContent,
                ProtocolKind::Claude,
            ),
            pass(OperationFamily::StreamGenerateContent, ProtocolKind::Claude),
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiChatCompletion,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Claude,
            ),
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::OpenAiResponse,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Claude,
            ),
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Gemini,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Claude,
            ),
            xform(
                OperationFamily::StreamGenerateContent,
                ProtocolKind::GeminiNDJson,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Claude,
            ),
            xform(
                OperationFamily::GeminiLive,
                ProtocolKind::Gemini,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Claude,
            ),
            xform(
                OperationFamily::OpenAiResponseWebSocket,
                ProtocolKind::OpenAi,
                OperationFamily::StreamGenerateContent,
                ProtocolKind::Claude,
            ),
            xform(
                OperationFamily::Compact,
                ProtocolKind::OpenAi,
                OperationFamily::GenerateContent,
                ProtocolKind::Claude,
            ),
            // Files API
            pass(OperationFamily::FileUpload, ProtocolKind::Claude),
            pass(OperationFamily::FileList, ProtocolKind::Claude),
            pass(OperationFamily::FileContent, ProtocolKind::Claude),
            pass(OperationFamily::FileGet, ProtocolKind::Claude),
            pass(OperationFamily::FileDelete, ProtocolKind::Claude),
        ];

        for (key, imp) in routes {
            t.set(key, imp);
        }
        t
    }

    fn model_pricing(&self) -> &'static [crate::billing::ModelPrice] {
        claudecode_model_pricing()
    }

    fn prepare_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
        request: &PreparedRequest,
    ) -> Result<http::Request<Vec<u8>>, UpstreamError> {
        let is_file_op = crate::file_operation::is_file_operation(request.route.operation);

        // For file operations, pass body through as-is (may be multipart or empty).
        // For normal operations, parse JSON to inject metadata.
        let (body, session_id) = if is_file_op {
            (request.body.clone(), String::new())
        } else {
            let mut body_json: Value = serde_json::from_slice(&request.body)
                .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?;
            let sid = request_session_id(request, &body_json, credential);
            let user_id_value = build_metadata_user_id(credential, &sid);
            inject_metadata_user_id(&mut body_json, &user_id_value);
            let b = serde_json::to_vec(&body_json)
                .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?;
            (b, sid)
        };

        // -- 2. Build the User-Agent ------------------------------------
        let user_agent = match settings.user_agent() {
            Some(ua) => ua.to_string(),
            None => format!(
                "claude-cli/{} ({}, {})",
                settings.cli_version(),
                settings.user_type(),
                settings.entrypoint()
            ),
        };

        // -- 3. Assemble the HTTP request -------------------------------
        //    Header order mirrors what the Anthropic JS SDK (Stainless,
        //    @anthropic-ai/sdk@0.81.0 on undici) emits on real 2.1.112
        //    traffic. HTTP/2 header order is not semantic but some
        //    fingerprinters inspect HPACK sequence, so we preserve it.
        let mut url = format!(
            "{}{}",
            settings.base_url(),
            claudecode_request_path(request)?
        );
        crate::utils::url::append_query(&mut url, request.query.as_deref());
        let mut builder = http::Request::builder()
            .method(request.method.clone())
            .uri(&url)
            .header("accept", "application/json")
            .header("x-stainless-retry-count", "0")
            .header("x-stainless-timeout", settings.stainless_timeout())
            .header("x-stainless-lang", settings.stainless_lang())
            .header(
                "x-stainless-package-version",
                settings.stainless_package_version(),
            )
            .header("x-stainless-os", settings.stainless_os())
            .header("x-stainless-arch", settings.stainless_arch())
            .header("x-stainless-runtime", settings.stainless_runtime())
            .header(
                "x-stainless-runtime-version",
                settings.stainless_runtime_version(),
            )
            .header("anthropic-dangerous-direct-browser-access", "true")
            .header("anthropic-version", CLAUDECODE_API_VERSION)
            .header("x-app", "cli")
            .header("User-Agent", &user_agent);

        if !is_file_op {
            builder = builder.header("X-Claude-Code-Session-Id", &session_id);
        }

        builder = builder.header(
            "Authorization",
            format!("Bearer {}", credential.access_token),
        );
        if !is_file_op {
            builder = builder.header("Content-Type", "application/json");
        }

        // Forward any additional headers from the prepared request —
        // this carries the `anthropic-beta` header (merged by
        // `finalize_request` to include the OAuth beta plus the files
        // beta for file ops) and Content-Type for file uploads.
        for (key, value) in request.headers.iter() {
            builder = builder.header(key, value);
        }
        crate::utils::http_headers::replace_header(
            &mut builder,
            "Authorization",
            format!("Bearer {}", credential.access_token),
        )?;
        crate::utils::http_headers::replace_header(
            &mut builder,
            "anthropic-version",
            CLAUDECODE_API_VERSION,
        )?;
        crate::utils::http_headers::replace_header(&mut builder, "x-app", "cli")?;
        crate::utils::http_headers::replace_header(&mut builder, "User-Agent", &user_agent)?;
        crate::utils::http_headers::replace_header(&mut builder, "accept", "application/json")?;
        crate::utils::http_headers::replace_header(
            &mut builder,
            "anthropic-dangerous-direct-browser-access",
            "true",
        )?;
        crate::utils::http_headers::replace_header(&mut builder, "x-stainless-retry-count", "0")?;
        crate::utils::http_headers::replace_header(
            &mut builder,
            "x-stainless-timeout",
            settings.stainless_timeout(),
        )?;
        crate::utils::http_headers::replace_header(
            &mut builder,
            "x-stainless-lang",
            settings.stainless_lang(),
        )?;
        crate::utils::http_headers::replace_header(
            &mut builder,
            "x-stainless-package-version",
            settings.stainless_package_version(),
        )?;
        crate::utils::http_headers::replace_header(
            &mut builder,
            "x-stainless-os",
            settings.stainless_os(),
        )?;
        crate::utils::http_headers::replace_header(
            &mut builder,
            "x-stainless-arch",
            settings.stainless_arch(),
        )?;
        crate::utils::http_headers::replace_header(
            &mut builder,
            "x-stainless-runtime",
            settings.stainless_runtime(),
        )?;
        crate::utils::http_headers::replace_header(
            &mut builder,
            "x-stainless-runtime-version",
            settings.stainless_runtime_version(),
        )?;
        crate::utils::http_headers::replace_header(&mut builder, "accept-language", "*")?;
        crate::utils::http_headers::replace_header(&mut builder, "sec-fetch-mode", "cors")?;
        crate::utils::http_headers::replace_header(
            &mut builder,
            "accept-encoding",
            "gzip, deflate",
        )?;
        if !is_file_op {
            crate::utils::http_headers::replace_header(
                &mut builder,
                "X-Claude-Code-Session-Id",
                &session_id,
            )?;
            crate::utils::http_headers::replace_header(
                &mut builder,
                "Content-Type",
                "application/json",
            )?;
        }

        builder
            .body(body)
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))
    }

    fn finalize_request(
        &self,
        settings: &Self::Settings,
        mut request: PreparedRequest,
    ) -> Result<PreparedRequest, UpstreamError> {
        // File operations: ensure both the OAuth and files-api beta
        // tokens are present in the `anthropic-beta` header (preserving
        // any values already set by the client or an earlier layer),
        // and skip JSON body normalization.
        if crate::file_operation::is_file_operation(request.route.operation) {
            crate::utils::anthropic_beta::ensure_anthropic_beta_tokens(
                &mut request.headers,
                &[CLAUDECODE_OAUTH_BETA, "files-api-2025-04-14"],
            )?;
            return Ok(request);
        }

        let mut body_json: Value = serde_json::from_slice(&request.body)
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?;

        claude_sampling::strip_sampling_params(&mut body_json);
        normalize_claudecode_unsupported_fields(&mut body_json);

        // Prelude injection: prepend organization-wide instructions as
        // the first system block. Runs before cache control so the
        // prelude can participate in cache breakpoint budget.
        if let Some(prelude) = settings.prelude_text.as_deref().filter(|s| !s.is_empty()) {
            apply_claudecode_prelude(&mut body_json, prelude);
        }

        if settings.enable_magic_cache {
            cache_control::apply_magic_string_cache_control_triggers(&mut body_json);
        }
        if !settings.cache_breakpoints.is_empty() {
            cache_control::ensure_cache_breakpoint_rules(
                &mut body_json,
                &settings.cache_breakpoints,
            );
        }
        if settings.flatten_system_before_cache {
            cache_control::flatten_system_text_blocks(&mut body_json);
        }
        cache_control::sanitize_claude_body(&mut body_json);

        let attribution = build_attribution(&first_user_message_text(&body_json));
        inject_system_attribution(&mut body_json, &attribution);
        // OAuth-authenticated claudecode requests require
        // `anthropic-beta: oauth-2025-04-20`; without it Anthropic
        // returns `401 OAuth authentication is currently not supported`.
        // Merge the token into any existing `anthropic-beta` value
        // instead of overwriting so client-supplied betas survive.
        crate::utils::anthropic_beta::ensure_anthropic_beta_tokens(
            &mut request.headers,
            &[CLAUDECODE_OAUTH_BETA],
        )?;
        // Drop default-on betas that upstream rejects on the OAuth path.
        // Operators can opt back in via `extra_beta_headers` below.
        crate::utils::anthropic_beta::strip_anthropic_beta_tokens(
            &mut request.headers,
            &["context-1m-2025-08-07"],
        )?;
        // Merge any extra beta values from settings (e.g. feature flags
        // the operator wants on every request to this provider).
        if !settings.extra_beta_headers.is_empty() {
            let refs: Vec<&str> = settings
                .extra_beta_headers
                .iter()
                .map(String::as_str)
                .collect();
            crate::utils::anthropic_beta::ensure_anthropic_beta_tokens(
                &mut request.headers,
                &refs,
            )?;
        }
        // Session id is computed in `prepare_request` per credential
        // attempt — it needs to land on the HTTP request alongside the
        // per-attempt `x-client-request-id`, not in the generic
        // `PreparedRequest.headers` map (which `prepare_request` also
        // forwards verbatim, leading to a duplicated header on the wire).
        request.body = serde_json::to_vec(&body_json)
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?;
        Ok(request)
    }

    fn classify_response(
        &self,
        status: u16,
        headers: &http::HeaderMap,
        _body: &[u8],
    ) -> ResponseClassification {
        match status {
            200..=299 => ResponseClassification::Success,
            401 | 403 => ResponseClassification::AuthDead,
            429 => {
                let retry_after_ms = parse_claudecode_rate_limit(headers);
                ResponseClassification::RateLimited { retry_after_ms }
            }
            529 => ResponseClassification::TransientError,
            500..=599 => ResponseClassification::TransientError,
            _ => ResponseClassification::PermanentError,
        }
    }

    fn count_strategy(&self) -> CountStrategy {
        CountStrategy::UpstreamApi
    }

    fn needs_spoof_client(&self, credential: &Self::Credential) -> bool {
        credential.cookie.as_ref().is_some_and(|c| !c.is_empty())
    }

    fn prepare_quota_request(
        &self,
        credential: &Self::Credential,
        settings: &Self::Settings,
    ) -> Result<Option<http::Request<Vec<u8>>>, UpstreamError> {
        let url = format!(
            "{}/api/oauth/usage",
            settings.platform_base_url.trim_end_matches('/')
        );
        let user_agent = match settings.user_agent() {
            Some(ua) => ua.to_string(),
            None => format!(
                "claude-cli/{} ({}, {})",
                DEFAULT_CLAUDECODE_VERSION,
                DEFAULT_CLAUDECODE_USER_TYPE,
                DEFAULT_CLAUDECODE_ENTRYPOINT
            ),
        };
        let req = http::Request::builder()
            .method(http::Method::GET)
            .uri(&url)
            .header(
                "Authorization",
                format!("Bearer {}", credential.access_token),
            )
            .header("Accept", "application/json")
            .header("Content-Type", "application/json")
            .header("User-Agent", &user_agent)
            .header("anthropic-beta", "oauth-2025-04-20")
            .body(Vec::new())
            .map_err(|e| UpstreamError::RequestBuild(e.to_string()))?;
        Ok(Some(req))
    }

    fn needs_refresh(&self, credential: &Self::Credential) -> bool {
        if credential.access_token.trim().is_empty() {
            return true;
        }
        // Refresh when within a 60s skew window of `expires_at_ms`.
        // `expires_at_ms == 0` means "unknown" and is treated as valid
        // (the normal 401 → refresh path still covers stale tokens).
        if credential.expires_at_ms == 0 {
            return false;
        }
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        credential.expires_at_ms <= now_ms.saturating_add(60_000)
    }

    fn refresh_credential<'a>(
        &'a self,
        client: &'a wreq::Client,
        credential: &'a mut Self::Credential,
    ) -> impl std::future::Future<Output = Result<bool, UpstreamError>> + Send + 'a {
        let client = client.clone();
        let span = tracing::info_span!("refresh_credential", channel = "claudecode");
        async move {
            // Path 1: Anthropic OAuth `refresh_token` grant.
            //
            // We do NOT use the generic `oauth2_refresh::refresh_oauth2_token`
            // helper here — Anthropic's `/v1/oauth/token` endpoint rejects
            // refresh requests that omit `client_id` or the
            // `anthropic-version` / `anthropic-beta` / CLI `user-agent`
            // headers with `invalid_request_error: Invalid request format`.
            // See `exchange_tokens_with_refresh_token` for the required
            // shape. A credential with only a `refresh_token` (no cookie)
            // would otherwise silently stay dead forever.
            if !credential.refresh_token.is_empty() {
                match crate::utils::claudecode_cookie::exchange_tokens_with_refresh_token(
                    &client,
                    &default_claudecode_base_url(),
                    &credential.refresh_token,
                    &mut Vec::new(),
                )
                .await
                {
                    Ok(tokens) => {
                        apply_cookie_exchange_tokens(credential, tokens);
                        tracing::info!("credential refreshed via refresh_token grant");
                        return Ok(true);
                    }
                    Err(e) if credential.cookie.as_ref().is_some_and(|c| !c.is_empty()) => {
                        tracing::info!(
                            error = %e,
                            "refresh_token grant failed, falling back to cookie"
                        );
                        // Fall through to cookie path
                    }
                    Err(e) => return Err(e),
                }
            }

            // Path 2: Cookie-to-token exchange (fallback)
            let cookie = match &credential.cookie {
                Some(c) if !c.is_empty() => c.clone(),
                _ => return Ok(false),
            };
            let tokens = crate::utils::claudecode_cookie::exchange_tokens_with_cookie(
                &client,
                &default_claudecode_base_url(),
                &default_claudecode_claude_ai_base_url(),
                &cookie,
                &mut Vec::new(),
            )
            .await?;
            apply_cookie_exchange_tokens(credential, tokens);
            tracing::info!("credential refreshed via cookie exchange");
            Ok(true)
        }
        .instrument(span)
    }
    fn oauth_start<'a>(
        &'a self,
        _client: &'a wreq::Client,
        settings: &'a Self::Settings,
        params: &'a BTreeMap<String, String>,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<OAuthFlow>, UpstreamError>> + Send + 'a>,
    > {
        Box::pin(async move {
            let now_unix_ms = crate::utils::oauth::current_unix_ms();
            prune_claudecode_oauth_states(now_unix_ms);

            let redirect_uri = crate::utils::oauth::parse_query_value(params, "redirect_uri")
                .unwrap_or_else(|| CLAUDECODE_REDIRECT_URI.to_string());
            let scope = crate::utils::oauth::parse_query_value(params, "scope")
                .unwrap_or_else(|| CLAUDECODE_OAUTH_SCOPE.to_string());
            let api_base_url = if settings.base_url().trim().is_empty() {
                "https://api.anthropic.com".to_string()
            } else {
                settings.base_url().to_string()
            };
            let claude_ai_base_url =
                crate::utils::oauth::parse_query_value(params, "claude_ai_base_url")
                    .unwrap_or_else(|| CLAUDECODE_CLAUDE_AI_BASE_URL.to_string());
            let state = crate::utils::oauth::generate_state();
            let code_verifier = crate::utils::oauth::generate_code_verifier();
            let code_challenge = crate::utils::oauth::generate_code_challenge(&code_verifier);
            let authorize_url = build_claudecode_authorize_url(
                &claude_ai_base_url,
                &redirect_uri,
                &scope,
                &code_challenge,
                &state,
            );

            claudecode_oauth_states().insert(
                state.clone(),
                ClaudeCodeOAuthState {
                    code_verifier,
                    redirect_uri: redirect_uri.clone(),
                    api_base_url,
                    claude_ai_base_url,
                    created_at_unix_ms: now_unix_ms,
                },
            );

            Ok(Some(OAuthFlow {
                authorize_url,
                state,
                redirect_uri: Some(redirect_uri),
                verification_uri: None,
                user_code: None,
                mode: Some("authorization_code".to_string()),
                scope: Some(scope),
                instructions: Some(
                    "Open authorize_url and complete authorization, then call oauth_finish with code/state or callback_url."
                        .to_string(),
                ),
            }))
        })
    }

    fn oauth_finish<'a>(
        &'a self,
        client: &'a wreq::Client,
        _settings: &'a Self::Settings,
        params: &'a BTreeMap<String, String>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<Option<OAuthCredentialResult<Self::Credential>>, UpstreamError>,
                > + Send
                + 'a,
        >,
    > {
        Box::pin(async move {
            if let Some(error) = crate::utils::oauth::parse_query_value(params, "error") {
                let detail = crate::utils::oauth::parse_query_value(params, "error_description")
                    .unwrap_or(error);
                return Err(UpstreamError::Channel(detail));
            }

            prune_claudecode_oauth_states(crate::utils::oauth::current_unix_ms());
            let (code, state_param) = crate::utils::oauth::resolve_code_and_state(params)
                .map_err(|e| UpstreamError::Channel(format!("claudecode oauth callback: {e}")))?;
            let state_id = state_param.ok_or_else(|| {
                UpstreamError::Channel("claudecode oauth callback: missing state".to_string())
            })?;
            let (_, oauth_state) = claudecode_oauth_states()
                .remove(state_id.as_str())
                .ok_or_else(|| {
                    UpstreamError::Channel("claudecode oauth callback: missing state".to_string())
                })?;

            let token = exchange_claudecode_code_for_tokens(
                client,
                &oauth_state.api_base_url,
                &oauth_state.claude_ai_base_url,
                &oauth_state.redirect_uri,
                &oauth_state.code_verifier,
                &code,
                &state_id,
            )
            .await?;
            let access_token = token
                .access_token
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    UpstreamError::Channel(
                        "claudecode oauth callback: missing access_token".to_string(),
                    )
                })?
                .to_string();
            let refresh_token = token
                .refresh_token
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    UpstreamError::Channel(
                        "claudecode oauth callback: missing refresh_token".to_string(),
                    )
                })?
                .to_string();
            let profile =
                fetch_claudecode_oauth_profile(client, &oauth_state.api_base_url, &access_token)
                    .await
                    .ok();
            let rate_limit_tier = token.rate_limit_tier.or_else(|| {
                profile
                    .as_ref()
                    .and_then(|profile| profile.organization.rate_limit_tier.clone())
            });
            let user_email = profile
                .as_ref()
                .and_then(|profile| profile.account.email.clone());
            let account_uuid = profile
                .as_ref()
                .and_then(|profile| profile.account.uuid.clone());
            let expires_at_ms = crate::utils::oauth::current_unix_ms()
                .saturating_add(token.expires_in.unwrap_or(3600).saturating_mul(1000));

            Ok(Some(OAuthCredentialResult {
                credential: ClaudeCodeCredential {
                    access_token: access_token.clone(),
                    refresh_token,
                    expires_at_ms,
                    device_id: default_claudecode_device_id(),
                    account_uuid: account_uuid.clone(),
                    rate_limit_tier,
                    cookie: None,
                    user_email: user_email.clone(),
                },
                details: json!({
                    "access_token": access_token,
                    "account_uuid": account_uuid,
                    "user_email": user_email,
                    "expires_at_ms": expires_at_ms,
                }),
            }))
        })
    }
}

fn claudecode_request_path(request: &PreparedRequest) -> Result<String, UpstreamError> {
    match request.route.operation {
        OperationFamily::FileUpload => Ok("/v1/files".to_string()),
        OperationFamily::FileList => Ok("/v1/files".to_string()),
        OperationFamily::FileContent => Ok(format!(
            "/v1/files/{}/content",
            serde_json::from_slice::<Value>(&request.body)
                .ok()
                .and_then(|v| v
                    .pointer("/path/file_id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned))
                .unwrap_or_default()
        )),
        OperationFamily::FileGet | OperationFamily::FileDelete => Ok(format!(
            "/v1/files/{}",
            serde_json::from_slice::<Value>(&request.body)
                .ok()
                .and_then(|v| v
                    .pointer("/path/file_id")
                    .and_then(Value::as_str)
                    .map(ToOwned::to_owned))
                .unwrap_or_default()
        )),
        OperationFamily::ModelList => Ok("/v1/models".to_string()),
        OperationFamily::ModelGet => Ok(format!(
            "/v1/models/{}",
            request.model.as_deref().unwrap_or_default()
        )),
        OperationFamily::CountToken => Ok("/v1/messages/count_tokens".to_string()),
        OperationFamily::GenerateContent | OperationFamily::StreamGenerateContent => {
            Ok("/v1/messages".to_string())
        }
        _ => Err(UpstreamError::Channel(format!(
            "unsupported claudecode request route: ({}, {})",
            request.route.operation, request.route.protocol
        ))),
    }
}

/// Parse Anthropic unified rate-limit headers into a single `retry_after_ms`.
///
/// Priority:
/// 1. `anthropic-ratelimit-unified-reset` — the server-chosen reset timestamp
///    for the representative (most constrained) window.
/// 2. `retry-after` — standard HTTP header (seconds).
///
/// Falls back to `None` if neither header is present / parseable.
fn parse_claudecode_rate_limit(headers: &http::HeaderMap) -> Option<u64> {
    // Prefer the unified reset header — it reflects the actual window that
    // triggered the rejection (5h, 7d, overage, etc.).
    if let Some(reset_ms) = parse_unix_reset_header(headers, "anthropic-ratelimit-unified-reset") {
        return Some(reset_ms);
    }
    // Fallback: standard retry-after (seconds).
    parse_retry_after_secs(headers)
}

/// Convert a unix-seconds reset header to a delay in milliseconds from now.
/// Returns `None` if the header is absent, unparseable, or already in the past.
fn parse_unix_reset_header(headers: &http::HeaderMap, name: &str) -> Option<u64> {
    let reset_secs = headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let reset_ms = reset_secs.saturating_mul(1000);
    if reset_ms > now_ms {
        Some(reset_ms - now_ms)
    } else {
        None
    }
}

/// Parse the standard `retry-after` header (integer seconds) into milliseconds.
fn parse_retry_after_secs(headers: &http::HeaderMap) -> Option<u64> {
    headers
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .map(|secs| secs * 1000)
}

fn claudecode_routing_table() -> RoutingTable {
    ClaudeCodeChannel.routing_table()
}

inventory::submit! { ChannelRegistration::new(ClaudeCodeChannel::ID, claudecode_routing_table) }
