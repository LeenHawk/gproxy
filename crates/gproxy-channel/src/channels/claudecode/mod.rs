//! Claude Code: a Claude.ai subscription used through the OAuth surface of
//! `api.anthropic.com`, the way the Claude Code CLI uses it.
//!
//! Wire facts follow the v3 channel (`crates/gproxy-channels/src/claudecode`
//! on `main`) and the Claude Code CLI (`samples/claude-code-2.1.252`):
//! PKCE authorization at `claude.com/cai/oauth/authorize`, token exchange and
//! refresh at `platform.claude.com/v1/oauth/token` (JSON bodies), a cookie
//! login that mints the same OAuth tokens through `claude.ai/api/bootstrap`
//! and `api.anthropic.com/v1/oauth/{org}/authorize`, Messages at
//! `{base}/v1/messages?beta=true` with the `oauth-2025-04-20` beta, the
//! CLI's identity headers and its request hygiene (billing system block,
//! `metadata.user_id`, cache-control repair, sampling and prefill fixes),
//! account windows at `{base}/api/oauth/usage` and in the
//! `anthropic-ratelimit-unified-*` response headers. The CLI's other OAuth
//! calls (`/api/oauth/**`, bootstrap, policy limits, organization
//! resources) are `ChannelServices` in `services.rs`.
//!
//! The credential secret is an `OAuthCredential`; account facts a login
//! discovers (`account_uuid`, `device_id`, plan tiers, email) travel in
//! `provider_fields` and, once the host persists them, in the credential's
//! metadata. A cookie login additionally keeps the normalized `cookie` at the
//! top level of the secret so a refresh can re-mint tokens without a refresh
//! token.

mod cookie;
mod cch;
mod hygiene;
mod quota;
mod reset;
mod services;

pub use services::{KIND_FILE, KIND_PLUGIN, KIND_SKILL, service_routes};

use crate::OutboundClient;
use crate::channel::{
    AuthorizationCode, AuthorizationRequest, AuthorizationStart, BaseChannel, ChannelCapabilities,
    ChannelDescriptor, ChannelError, ChannelHeaders, ChannelServices, ConfigKey, ConfigKeyKind,
    CookieLogin, CredentialRefresh, CredentialUpdate, CredentialView, HOST_CONFIG_KEYS,
    HeaderAllowlist, LoginContext, LoginMode, OAuthAuthorizationCode, OAuthCredential,
    OperationContext, OperationFuture, PrepareContext, ProviderView, QuotaHeaders, QuotaModel,
    QuotaQuery, QuotaReset, RefreshContext, forwardable,
};
use crate::channels::shared::{cache, claude_fallback};
pub use crate::channels::shared::claude_fallback::FallbackMode;
use futures_util::StreamExt;
use gproxy_client::{Backend, ConnectionConfig};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse, capability::StateWrite,
    connection::Bytes,
};
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

pub const ID: &str = "claudecode";
pub const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
/// The claude.ai origin the cookie login bootstraps against (v3 `auth.rs`).
pub const DEFAULT_CLAUDE_AI_URL: &str = "https://claude.ai";
/// v3 `login.rs` and the CLI's `AUTHORIZE_URL`.
pub const DEFAULT_AUTHORIZE_URL: &str = "https://claude.com/cai/oauth/authorize";
/// v3 `auth.rs` `TOKEN_URL`; the CLI moved here from api.anthropic.com.
pub const DEFAULT_TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
/// The manual-callback redirect the CLI registers (v3 `auth.rs`).
pub const DEFAULT_REDIRECT_URI: &str = "https://platform.claude.com/oauth/code/callback";
pub const DEFAULT_CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
/// Default refresh scopes verified against CLI 2.1.292, including plugins.
pub const OAUTH_SCOPE: &str = "user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload user:plugins";
/// Interactive login adds `org:create_api_key` (CLI 2.1.292).
pub const LOGIN_SCOPE: &str = concat!(
    "org:create_api_key ",
    "user:profile user:inference user:sessions:claude_code user:mcp_servers user:file_upload user:plugins"
);
pub const OAUTH_BETA: &str = "oauth-2025-04-20";
/// The CLI version the channel impersonates; audited in
/// `design/claudecode-2.1.292.md` against the installed binary and local capture.
pub const CLI_VERSION: &str = "2.1.292";
pub const CLI_USER_AGENT: &str = "claude-cli/2.1.292 (external, cli)";
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
/// The CLI marks a request it sends in low-priority mode with this header
/// (CLI 2.1.283).
const USAGE_LIMIT_HEADER: &str = "anthropic-usage-limit";
const LOW_PRIORITY: &str = "slow";
/// Optional scopes a refresh preserves when the credential already has them.
const PRESERVED_SCOPES: &[&str] = &["user:projects:read", "user:projects:write"];
/// Bodies the channel reads itself (login, refresh, usage) are small.
const MAX_SERVICE_BODY: usize = 1024 * 1024;
const DEFAULT_EXPIRES_IN_SECS: i64 = 3600;
/// Twenty-minute buckets for the derived session id (v3 `auth.rs`).
const SESSION_WINDOW_MS: i64 = 20 * 60 * 1000;

/// Provider `config` JSON understood by this channel. Unknown keys are ignored.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct ClaudecodeConfig {
    pub authorize_url: String,
    pub token_url: String,
    /// The claude.ai origin the cookie login talks to.
    pub claude_ai_url: String,
    /// Static headers added to every backend request.
    pub headers: BTreeMap<String, String>,
    /// Place `cache_control` where a client embeds a magic cache string in
    /// Messages and count_tokens bodies (`channels::shared::cache`). Off by
    /// default; the strings are stripped either way.
    pub enable_claude_magic_cache: bool,
    /// Server-side fallback when a Messages request supplies none.
    pub fallback_mode: FallbackMode,
    /// Ordered fallback models; at most three are sent.
    pub fallback_models: Vec<String>,
    /// Ask for the CLI's low-priority mode on every Messages call
    /// (`anthropic-usage-limit: slow`) and keep a full five-hour window from
    /// blocking the credential. The server decides per account whether to
    /// serve it (`anthropic-ratelimit-unified-slow-status`); the weekly
    /// windows still block.
    pub low_priority: bool,
}

impl Default for ClaudecodeConfig {
    fn default() -> Self {
        Self {
            authorize_url: DEFAULT_AUTHORIZE_URL.into(),
            token_url: DEFAULT_TOKEN_URL.into(),
            claude_ai_url: DEFAULT_CLAUDE_AI_URL.into(),
            headers: BTreeMap::new(),
            enable_claude_magic_cache: false,
            fallback_mode: FallbackMode::Off,
            fallback_models: Vec::new(),
            low_priority: false,
        }
    }
}

impl ClaudecodeConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Claudecode;

/// Ordinary API requests use wreq without an emulation profile.
pub fn default_connection() -> ConnectionConfig {
    ConnectionConfig {
        backend: Backend::Wreq,
        emulation: None,
        gzip: true,
        brotli: true,
        deflate: true,
        zstd: true,
        ..ConnectionConfig::default()
    }
}

fn invalid_config(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidConfig(message.into())
}

fn invalid_response(message: impl Into<String>) -> ChannelError {
    ChannelError::InvalidResponse(message.into())
}

fn header_value(value: &str) -> Result<HeaderValue, ChannelError> {
    HeaderValue::from_str(value).map_err(|_| ChannelError::InvalidCredential)
}

fn base_url(provider: ProviderView<'_>) -> String {
    provider
        .base_url
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(DEFAULT_BASE_URL)
        .trim_end_matches('/')
        .to_owned()
}

pub(super) fn unix_now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn non_empty(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// A public account fact: host metadata first, then the secret's
/// `provider_fields`, then the v3 flat secret layout.
fn fact<'a>(credential: &CredentialView<'a>, name: &str) -> Option<&'a str> {
    non_empty(credential.metadata.get(name))
        .or_else(|| {
            non_empty(
                credential
                    .secret
                    .pointer(&format!("/provider_fields/{name}")),
            )
        })
        .or_else(|| non_empty(credential.secret.get(name)))
}

/// The stable device id the CLI reports in `metadata.user_id`; derived once
/// from the account (or, before a profile is known, the tokens) exactly as v3
/// `auth.rs` did, so an existing credential keeps its id.
fn derive_device_id(fields: &BTreeMap<String, Value>, secret: &Value) -> String {
    let seed = non_empty(fields.get("account_uuid"))
        .or_else(|| non_empty(secret.get("account_uuid")))
        .or_else(|| non_empty(secret.get("refresh_token")))
        .or_else(|| non_empty(secret.get("access_token")))
        .unwrap_or_default();
    hex(Sha256::digest(format!("claudecode-device:{seed}").as_bytes()).as_slice())
}

/// Facts about the account from the secret and the host-persisted metadata.
struct Account<'a> {
    access_token: &'a str,
    device_id: String,
    account_uuid: Option<String>,
}

fn account<'a>(credential: &CredentialView<'a>) -> Result<Account<'a>, ChannelError> {
    let access_token =
        non_empty(credential.secret.get("access_token")).ok_or(ChannelError::InvalidCredential)?;
    let account_uuid = fact(credential, "account_uuid").map(str::to_owned);
    let device_id = fact(credential, "device_id")
        .map(str::to_owned)
        .unwrap_or_else(|| derive_device_id(&BTreeMap::new(), credential.secret));
    Ok(Account {
        access_token,
        device_id,
        account_uuid,
    })
}

/// A v4 session id: the client's own when it sends one, otherwise a UUID
/// derived from the device and a twenty-minute window (v3 `auth.rs`).
fn session_id(device_id: &str, explicit: Option<&str>) -> String {
    if let Some(explicit) = explicit.map(str::trim).filter(|value| !value.is_empty()) {
        return explicit.to_owned();
    }
    let window = unix_now_ms() / SESSION_WINDOW_MS;
    derived_uuid(&format!("claudecode-session:{device_id}:{window}"))
}

/// A stable, UUID-shaped id from a seed: the first sixteen SHA-256 bytes
/// with the version and variant bits of a random UUID.
pub(super) fn derived_uuid(seed: &str) -> String {
    let digest = Sha256::digest(seed);
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{}-{}-{}-{}-{}",
        hex(&bytes[..4]),
        hex(&bytes[4..6]),
        hex(&bytes[6..8]),
        hex(&bytes[8..10]),
        hex(&bytes[10..])
    )
}

/// The client's own session id, either spelling the CLI has used.
fn explicit_session<'a>(
    headers: &'a HeaderMap,
    allowlist: Option<&HeaderAllowlist>,
) -> Option<&'a str> {
    client_header(headers, allowlist, "x-claude-code-session-id")
        .or_else(|| client_header(headers, allowlist, "session_id"))
}

/// Where a session's last upstream `request-id` lives in channel state.
fn prev_req_key(session: &str) -> String {
    format!("session:{session}:prev_req")
}

// --------------------------------------------------------------- headers

/// What the Claude Code CLI itself sends and the channel reads as hints (its
/// beta list, session id in either spelling, its user agent). A provider
/// allow-list never hides these from the channel.
pub const CLI_HEADERS: ChannelHeaders = ChannelHeaders {
    names: &[
        "anthropic-beta",
        "x-claude-code-session-id",
        "session_id",
        "user-agent",
    ],
    prefixes: &[],
};

/// Headers the channel sets itself; a client cannot supply them. Cookies are
/// claude.ai identity and never belong on an api.anthropic.com call.
const CHANNEL_HEADERS: &[&str] = &[
    "anthropic-version",
    "anthropic-beta",
    "anthropic-dangerous-direct-browser-access",
    "x-app",
    "x-claude-code-session-id",
    "session_id",
    "user-agent",
    "accept",
    "accept-encoding",
    "x-stainless-arch",
    "x-stainless-lang",
    "x-stainless-os",
    "x-stainless-package-version",
    "x-stainless-retry-count",
    "x-stainless-runtime",
    "x-stainless-runtime-version",
    "x-stainless-timeout",
    "cookie",
];

/// The CLI's static identity headers (captured in
/// `samples/claude-code-2.1.252/messages-oauth-wire.json`, v3 `auth.rs`).
const STATIC_HEADERS: &[(&str, &str)] = &[
    ("anthropic-dangerous-direct-browser-access", "true"),
    ("x-app", "cli"),
    ("x-stainless-retry-count", "0"),
    ("x-stainless-timeout", "600"),
    ("x-stainless-lang", "js"),
    ("x-stainless-package-version", "0.128.0"),
    ("x-stainless-runtime", "node"),
    ("x-stainless-runtime-version", "v26.3.0"),
];

#[cfg(target_arch = "wasm32")]
fn stainless_os() -> &'static str {
    "Linux"
}

#[cfg(not(target_arch = "wasm32"))]
fn stainless_os() -> &'static str {
    match std::env::consts::OS {
        "ios" => "iOS",
        "android" => "Android",
        "macos" => "MacOS",
        "windows" => "Windows",
        "freebsd" => "FreeBSD",
        "openbsd" => "OpenBSD",
        "linux" => "Linux",
        _ => "Unknown",
    }
}

#[cfg(target_arch = "wasm32")]
fn stainless_arch() -> &'static str {
    "x64"
}

#[cfg(not(target_arch = "wasm32"))]
fn stainless_arch() -> &'static str {
    match std::env::consts::ARCH {
        "x86_64" => "x64",
        "aarch64" => "arm64",
        "x86" => "x32",
        "arm" => "arm",
        _ => "unknown",
    }
}

/// A client user agent is honoured only when it is the impersonated CLI
/// version with a plausible entrypoint, e.g. `claude-cli/2.1.292 (external,
/// sdk-cli)`; anything else becomes the CLI's own (v3 `auth.rs`).
fn valid_cli_user_agent(value: &str) -> bool {
    value
        .strip_prefix("claude-cli/")
        .and_then(|value| value.split_once(" (external, "))
        .filter(|(version, _)| *version == CLI_VERSION)
        .map(|(_, entrypoint)| entrypoint)
        .and_then(|value| value.strip_suffix(')'))
        .is_some_and(|entrypoint| {
            !entrypoint.is_empty()
                && entrypoint
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        })
}

/// `oauth-2025-04-20` first, then every other beta already on the request.
fn merge_beta(headers: &HeaderMap) -> String {
    let mut values = vec![OAUTH_BETA];
    let existing = headers
        .get("anthropic-beta")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    for value in existing.split(',').map(str::trim).filter(|v| !v.is_empty()) {
        if !values.contains(&value) {
            values.push(value);
        }
    }
    values.join(",")
}

/// Headers every Messages-API call carries: bearer token, the OAuth beta
/// merged ahead of the request's betas, the CLI identity set and any static
/// config headers.
fn apply_headers(
    headers: &mut HeaderMap,
    config: &ClaudecodeConfig,
    access_token: &str,
    session_id: &str,
    client_user_agent: Option<&str>,
) -> Result<(), ChannelError> {
    headers.insert(
        header::AUTHORIZATION,
        header_value(&format!("Bearer {access_token}"))?,
    );
    headers.insert(
        HeaderName::from_static("anthropic-version"),
        HeaderValue::from_static(ANTHROPIC_VERSION),
    );
    let beta = merge_beta(headers);
    headers.insert(
        HeaderName::from_static("anthropic-beta"),
        header_value(&beta)?,
    );
    for (name, value) in STATIC_HEADERS {
        headers.insert(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        );
    }
    headers.insert(
        HeaderName::from_static("x-claude-code-session-id"),
        header_value(session_id)?,
    );
    headers.insert(
        HeaderName::from_static("x-stainless-os"),
        HeaderValue::from_static(stainless_os()),
    );
    headers.insert(
        HeaderName::from_static("x-stainless-arch"),
        HeaderValue::from_static(stainless_arch()),
    );
    let user_agent = client_user_agent
        .filter(|value| valid_cli_user_agent(value))
        .unwrap_or(CLI_USER_AGENT);
    headers.insert(header::USER_AGENT, header_value(user_agent)?);
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        header::ACCEPT_ENCODING,
        HeaderValue::from_static("gzip, deflate, br, zstd"),
    );
    for (name, value) in &config.headers {
        headers.insert(
            HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| invalid_config(format!("header `{name}`")))?,
            HeaderValue::from_str(value).map_err(|_| invalid_config(format!("header `{name}`")))?,
        );
    }
    Ok(())
}

// --------------------------------------------------------------- prepare

fn is_messages(operation: Operation) -> bool {
    matches!(
        operation,
        Operation::GenerateContent | Operation::StreamGenerateContent
    )
}

fn unsupported(operation: Operation) -> ChannelError {
    ChannelError::UnsupportedOperation(gproxy_protocol::OperationKey {
        operation,
        dialect: Dialect::Claude,
    })
}

/// The native path (v3 `prepare.rs`). The client's `/v1/models/{model}`
/// path is kept for GetModel because the model id lives only there.
fn operation_path(operation: Operation, request_path: &str) -> Result<String, ChannelError> {
    Ok(match operation {
        Operation::ListModels => "/v1/models".into(),
        Operation::GetModel => {
            let model = request_path
                .trim_end_matches('/')
                .rsplit('/')
                .next()
                .filter(|segment| !segment.is_empty())
                .ok_or_else(|| unsupported(operation))?;
            format!("/v1/models/{model}")
        }
        Operation::CountTokens => "/v1/messages/count_tokens".into(),
        Operation::GenerateContent | Operation::StreamGenerateContent => "/v1/messages".into(),
        other => return Err(unsupported(other)),
    })
}

/// Client query minus any `key` credential, with `beta=true` in front for
/// Messages and count_tokens as the CLI sends it (v3 `prepare.rs`).
fn query(source: Option<String>, operation: Operation) -> Option<String> {
    let mut kept = source
        .unwrap_or_default()
        .split('&')
        .filter(|pair| !pair.is_empty() && pair.split('=').next() != Some("key"))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if (is_messages(operation) || operation == Operation::CountTokens)
        && !kept
            .iter()
            .any(|pair| pair.split('=').next() == Some("beta"))
    {
        kept.insert(0, "beta=true".into());
    }
    (!kept.is_empty()).then(|| kept.join("&"))
}

fn client_header<'a>(
    headers: &'a HeaderMap,
    allowlist: Option<&HeaderAllowlist>,
    name: &'static str,
) -> Option<&'a str> {
    let name = HeaderName::from_static(name);
    if allowlist.is_some_and(|list| !list.allows(&name)) {
        return None;
    }
    headers.get(&name)?.to_str().ok()
}

impl BaseChannel for Claudecode {
    fn id(&self) -> &'static str {
        ID
    }

    /// A Claude.ai subscription through the CLI: PKCE or a claude.ai cookie to
    /// log in, a refreshable token, unified rate-limit windows, and the CLI's
    /// own non-protocol endpoints.
    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "Claude Code (Claude.ai)",
            login_modes: vec![LoginMode::AuthorizationCode, LoginMode::Cookie],
            capabilities: ChannelCapabilities {
                refresh: true,
                quota_query: true,
                quota_reset: true,
                services: true,
                websocket: false,
            },
            config_keys: [
                ConfigKey::optional(
                    "base_url",
                    ConfigKeyKind::String,
                    "Anthropic API origin; defaults to https://api.anthropic.com. Provider column, not config JSON.",
                ).with_placeholder(DEFAULT_BASE_URL),
                ConfigKey::optional(
                    "authorize_url",
                    ConfigKeyKind::String,
                    "OAuth authorization endpoint for the browser step.",
                ).with_placeholder(DEFAULT_AUTHORIZE_URL),
                ConfigKey::optional(
                    "token_url",
                    ConfigKeyKind::String,
                    "OAuth token endpoint used by the code exchange and by refresh.",
                ).with_placeholder(DEFAULT_TOKEN_URL),
                ConfigKey::optional(
                    "claude_ai_url",
                    ConfigKeyKind::String,
                    "The claude.ai origin the cookie login talks to.",
                ).with_placeholder(DEFAULT_CLAUDE_AI_URL),
                ConfigKey::optional(
                    "headers",
                    ConfigKeyKind::HeaderList,
                    "Static headers added to every backend request.",
                ),
                ConfigKey::optional(
                    "fallback_mode",
                    ConfigKeyKind::String,
                    "Server-side fallback for Messages requests that name none: off (default), default, or models.",
                ),
                ConfigKey::optional(
                    "fallback_models",
                    ConfigKeyKind::Json,
                    "The ordered chain fallback_mode models installs; at most three are sent.",
                ),
                ConfigKey::optional(
                    "enable_claude_magic_cache",
                    ConfigKeyKind::Bool,
                    "Turn a client's magic cache string in a Messages or count_tokens body into cache_control.",
                ),
                ConfigKey::optional(
                    "low_priority",
                    ConfigKeyKind::Bool,
                    "Send Messages calls in low-priority mode and keep routing to an account whose 5h window is full; the server decides whether the account is served.",
                ),
            ]
            .into_iter()
            .chain(HOST_CONFIG_KEYS)
            .collect(),
        }
    }

    fn default_connection(&self) -> Option<ConnectionConfig> {
        Some(default_connection())
    }

    fn default_connection_for(&self, purpose: crate::channel::ConnectionPurpose) -> Option<ConnectionConfig> {
        Some(match purpose {
            crate::channel::ConnectionPurpose::Request => default_connection(),
            crate::channel::ConnectionPurpose::CookieLogin => super::shared::browser_connection(),
        })
    }

    fn native_dialects(&self, _provider: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::GenerateContent
            | Operation::StreamGenerateContent
            | Operation::CountTokens
            | Operation::ListModels
            | Operation::GetModel => vec![Dialect::Claude],
            _ => Vec::new(),
        }
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        self.prepare_with(ctx, None)
    }

    /// Messages calls read and record the session's previous `request-id`
    /// around the plain HTTP path, so the billing block can carry
    /// `cc_prev_req` like the CLI does.
    fn generate_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(self.messages_http(Operation::GenerateContent, context))
    }

    fn stream_generate_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(self.messages_http(Operation::StreamGenerateContent, context))
    }

    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        Some(self)
    }
    fn oauth_authorization_code(&self) -> Option<&dyn OAuthAuthorizationCode> {
        Some(self)
    }
    fn cookie_login(&self) -> Option<&dyn CookieLogin> {
        Some(self)
    }
    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        Some(self)
    }
    fn quota_reset(&self) -> Option<&dyn QuotaReset> { Some(self) }
    fn quota_model(&self) -> Option<&dyn QuotaModel> {
        Some(self)
    }
    fn quota_headers(&self) -> Option<&dyn QuotaHeaders> {
        Some(self)
    }
    fn services(&self) -> Option<&dyn ChannelServices> {
        Some(self)
    }
}

impl Claudecode {
    fn prepare_with(
        &self,
        ctx: PrepareContext<'_>,
        prev_req: Option<&str>,
    ) -> Result<http::Request<HttpBody>, ChannelError> {
        let config = ClaudecodeConfig::from_view(ctx.provider)?;
        let account = account(&ctx.credential)?;
        let operation = ctx.operation.operation;
        let allowlist = HeaderAllowlist::from_view_for(ctx.provider, CLI_HEADERS)?;
        let WireRequest {
            method,
            path,
            query: source_query,
            headers: source,
            body,
        } = ctx.request;

        let url = match ctx.endpoint_override {
            Some(url) => url.to_owned(),
            None => format!(
                "{}{}",
                base_url(ctx.provider),
                operation_path(operation, &path)?
            ),
        };
        let uri = match query(source_query, operation) {
            Some(q) if url.contains('?') => format!("{url}&{q}"),
            Some(q) => format!("{url}?{q}"),
            None => url,
        };

        // Hints the client may give: its session id, its CLI user agent and
        // its own beta list.
        let session = session_id(
            &account.device_id,
            explicit_session(&source, allowlist.as_ref()),
        );
        let client_user_agent = client_header(&source, allowlist.as_ref(), "user-agent");
        let client_beta = client_header(&source, allowlist.as_ref(), "anthropic-beta")
            .and_then(|value| HeaderValue::from_str(value).ok());
        let mut headers = forwardable(&source, allowlist.as_ref(), CHANNEL_HEADERS);
        if let Some(beta) = client_beta {
            headers.insert(HeaderName::from_static("anthropic-beta"), beta);
        }

        let body = match body {
            HttpBody::Bytes(bytes) if is_messages(operation) => {
                match hygiene::json_object(&bytes) {
                    Some(mut value) => {
                        hygiene::messages(
                            &mut value,
                            &mut headers,
                            config.enable_claude_magic_cache,
                        );
                        claude_fallback::fallbacks(
                            &mut value,
                            &mut headers,
                            &config.fallback_mode,
                            &config.fallback_models,
                        );
                        hygiene::inject_billing(
                            &mut value,
                            &account.device_id,
                            account.account_uuid.as_deref().unwrap_or_default(),
                            &session,
                            prev_req,
                        );
                        HttpBody::Bytes(Bytes::from(cch::serialize(&value)))
                    }
                    None => HttpBody::Bytes(bytes),
                }
            }
            HttpBody::Bytes(bytes) if operation == Operation::CountTokens => {
                if let Some(value) = hygiene::json_object(&bytes) {
                    hygiene::count_tokens(&value, &mut headers);
                }
                // Rewritten only when a magic cache string is present.
                HttpBody::Bytes(cache::shape(
                    bytes,
                    config
                        .enable_claude_magic_cache
                        .then_some(cache::Rules::Claude),
                ))
            }
            other => other,
        };

        apply_headers(
            &mut headers,
            &config,
            account.access_token,
            &session,
            client_user_agent,
        )?;
        if config.low_priority && is_messages(operation) {
            headers.insert(
                HeaderName::from_static(USAGE_LIMIT_HEADER),
                HeaderValue::from_static(LOW_PRIORITY),
            );
        }
        let mut builder = http::Request::builder().method(method).uri(uri);
        if let Some(map) = builder.headers_mut() {
            *map = headers;
        }
        builder
            .body(body)
            .map_err(|error| invalid_config(error.to_string()))
    }

    async fn messages_http(
        &self,
        operation: Operation,
        ctx: OperationContext<'_>,
    ) -> Result<WireResponse<HttpBody>, ChannelError> {
        let account = account(&ctx.credential)?;
        let allowlist = HeaderAllowlist::from_view_for(ctx.provider, CLI_HEADERS)?;
        let session = session_id(
            &account.device_id,
            explicit_session(&ctx.request.headers, allowlist.as_ref()),
        );
        let key = prev_req_key(&session);
        // A host without channel state simply never carries `cc_prev_req`.
        let previous = ctx.state.get(&key).await.ok().flatten();
        let prev_req = previous
            .as_ref()
            .and_then(|entry| std::str::from_utf8(&entry.payload).ok())
            .filter(|value| hygiene::is_request_id(value))
            .map(str::to_owned);
        let request = self.prepare_with(
            PrepareContext {
                provider: ctx.provider,
                credential: ctx.credential,
                operation: OperationKey {
                    operation,
                    dialect: ctx.dialect,
                },
                request: ctx.request,
                endpoint_override: ctx.endpoint_override,
            },
            prev_req.as_deref(),
        )?;
        let response = ctx.client.send(request).await?;
        let request_id = response
            .headers
            .get("request-id")
            .and_then(|value| value.to_str().ok())
            .filter(|value| hygiene::is_request_id(value));
        if let Some(request_id) = request_id
            && prev_req.as_deref() != Some(request_id)
        {
            let expires_at = std::time::SystemTime::UNIX_EPOCH
                + std::time::Duration::from_millis(
                    u64::try_from(unix_now_ms() + hygiene::PREV_REQ_TTL_MS).unwrap_or(0),
                );
            // Losing the race to a concurrent call in the same session, or a
            // host without state, only loses one `cc_prev_req`.
            let _ = ctx
                .state
                .compare_exchange(
                    &key,
                    previous.map(|entry| entry.version),
                    Some(StateWrite {
                        payload: Bytes::copy_from_slice(request_id.as_bytes()),
                        expires_at: Some(expires_at),
                    }),
                )
                .await;
        }
        Ok(response)
    }
}

// ---------------------------------------------------------------- tokens

pub(super) async fn read_body(body: HttpBody) -> Result<Bytes, ChannelError> {
    match body {
        HttpBody::Bytes(bytes) => Ok(bytes),
        HttpBody::Stream(mut stream) => {
            let mut out = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| invalid_response(e.to_string()))?;
                out.extend_from_slice(&chunk);
                if out.len() > MAX_SERVICE_BODY {
                    return Err(invalid_response("service response exceeds the read limit"));
                }
            }
            Ok(Bytes::from(out))
        }
    }
}

pub(super) async fn send(
    client: &dyn OutboundClient,
    method: Method,
    url: &str,
    headers: HeaderMap,
    body: Option<Vec<u8>>,
) -> Result<(StatusCode, HeaderMap, Bytes), ChannelError> {
    let mut builder = http::Request::builder().method(method).uri(url);
    if let Some(map) = builder.headers_mut() {
        *map = headers;
    }
    let request = builder
        .body(HttpBody::Bytes(body.map(Bytes::from).unwrap_or_default()))
        .map_err(|error| invalid_config(error.to_string()))?;
    let WireResponse {
        status,
        headers,
        body,
    } = client.send(request).await?;
    Ok((status, headers, read_body(body).await?))
}

fn json_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    headers
}

/// Percent-encode a query or form component; spaces become `%20` (v3
/// `shared/http.rs`), so the space-separated scope survives both uses.
pub(super) fn encode_component(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut output = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            output.push(char::from(byte));
        } else {
            output.push('%');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    output
}

pub(super) fn form_encode(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(name, value)| format!("{}={}", encode_component(name), encode_component(value)))
        .collect::<Vec<_>>()
        .join("&")
}

#[derive(Deserialize)]
pub(super) struct TokenResponse {
    pub access_token: Option<String>,
    pub refresh_token: Option<String>,
    pub expires_in: Option<i64>,
    /// Sent by the token endpoint on refresh (CLI 2.1.252 route inventory).
    pub refresh_token_expires_in: Option<i64>,
    pub scope: Option<String>,
}

fn expiry_ms(seconds: Option<i64>, default: Option<i64>) -> Option<i64> {
    seconds
        .or(default)
        .map(|secs| unix_now_ms().saturating_add(secs.max(0).saturating_mul(1000)))
}

/// A credential from a token response. `refresh_token` and `scopes` fall
/// back to what the previous secret had; `fields` are the account facts.
pub(super) fn credential_from_tokens(
    tokens: TokenResponse,
    previous_refresh: Option<String>,
    previous_scopes: Vec<String>,
    mut fields: BTreeMap<String, Value>,
) -> Result<OAuthCredential, ChannelError> {
    let access_token = tokens
        .access_token
        .filter(|t| !t.trim().is_empty())
        .ok_or_else(|| invalid_response("token response has no access_token"))?;
    let refresh_token = tokens
        .refresh_token
        .filter(|t| !t.is_empty())
        .or(previous_refresh);
    let scopes = match tokens.scope {
        Some(scope) => scope.split_whitespace().map(str::to_owned).collect(),
        None => previous_scopes,
    };
    let expires_at_ms = expiry_ms(tokens.expires_in, Some(DEFAULT_EXPIRES_IN_SECS));
    let refresh_expires_at_ms = expiry_ms(tokens.refresh_token_expires_in, None);
    if !fields.contains_key("device_id") {
        let seed = json!({
            "refresh_token": refresh_token,
            "access_token": access_token,
        });
        fields.insert(
            "device_id".into(),
            Value::String(derive_device_id(&fields, &seed)),
        );
    }
    Ok(OAuthCredential {
        access_token,
        refresh_token,
        id_token: None,
        token_type: Some("Bearer".into()),
        scopes,
        expires_at_ms,
        refresh_expires_at_ms,
        provider_fields: fields,
        provider_secrets: BTreeMap::new(),
    })
}

/// The secret persisted for this channel: the OAuth credential plus, after a
/// cookie login, the normalized claude.ai cookie.
pub(super) fn secret_json(
    credential: &OAuthCredential,
    cookie: Option<&str>,
) -> Result<Value, ChannelError> {
    let mut secret =
        serde_json::to_value(credential).map_err(|e| invalid_response(e.to_string()))?;
    if let (Some(cookie), Some(object)) = (cookie, secret.as_object_mut()) {
        object.insert("cookie".into(), Value::String(cookie.to_owned()));
    }
    Ok(secret)
}

/// Public account facts from `GET {base}/api/oauth/profile` (v3
/// `account.rs`). Best effort: a failure leaves the fields untouched.
pub(super) async fn enrich_profile(
    client: &dyn OutboundClient,
    base: &str,
    access_token: &str,
    fields: &mut BTreeMap<String, Value>,
) {
    let mut headers = HeaderMap::new();
    let Ok(bearer) = header_value(&format!("Bearer {access_token}")) else {
        return;
    };
    headers.insert(header::AUTHORIZATION, bearer);
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(
        header::ACCEPT,
        HeaderValue::from_static("application/json, text/plain, */*"),
    );
    headers.insert(
        header::ACCEPT_ENCODING,
        HeaderValue::from_static("gzip, compress, deflate, br"),
    );
    // The CLI fetches its profile through axios, not the Anthropic SDK.
    headers.insert(header::USER_AGENT, HeaderValue::from_static("axios/1.13.6"));
    let Ok((status, _, bytes)) = send(
        client,
        Method::GET,
        &format!("{base}/api/oauth/profile"),
        headers,
        None,
    )
    .await
    else {
        return;
    };
    if !status.is_success() {
        return;
    }
    let Ok(profile) = serde_json::from_slice::<Value>(&bytes) else {
        return;
    };
    merge_profile(fields, &profile);
}

fn merge_profile(fields: &mut BTreeMap<String, Value>, profile: &Value) {
    let account = profile.get("account");
    let organization = profile.get("organization");
    let strings = [
        ("user_email", account.and_then(|v| v.get("email"))),
        ("account_uuid", account.and_then(|v| v.get("uuid"))),
        (
            "organization_uuid",
            organization.and_then(|v| v.get("uuid")),
        ),
        (
            "organization_type",
            organization.and_then(|v| v.get("organization_type")),
        ),
        (
            "rate_limit_tier",
            organization.and_then(|v| v.get("rate_limit_tier")),
        ),
        ("seat_tier", organization.and_then(|v| v.get("seat_tier"))),
        (
            "billing_type",
            organization.and_then(|v| v.get("billing_type")),
        ),
    ];
    for (name, value) in strings {
        if let Some(value) = non_empty(value) {
            fields.insert(name.into(), Value::String(value.into()));
        }
    }
    if let Some(enabled) = organization
        .and_then(|v| v.get("has_extra_usage_enabled"))
        .and_then(Value::as_bool)
    {
        fields.insert("has_extra_usage_enabled".into(), Value::Bool(enabled));
    }
}

fn previous_fields(secret: &Value) -> BTreeMap<String, Value> {
    let mut fields = BTreeMap::new();
    if let Some(Value::Object(previous)) = secret.get("provider_fields") {
        for (key, value) in previous {
            fields.insert(key.clone(), value.clone());
        }
    }
    // v3 kept these facts flat on the secret.
    for name in [
        "device_id",
        "account_uuid",
        "user_email",
        "organization_uuid",
        "organization_type",
        "rate_limit_tier",
        "seat_tier",
        "billing_type",
    ] {
        if let Some(value) = non_empty(secret.get(name))
            && !fields.contains_key(name)
        {
            fields.insert(name.into(), Value::String(value.into()));
        }
    }
    fields
}

fn previous_scopes(secret: &Value) -> Vec<String> {
    secret
        .get("scopes")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

/// The token endpoint's error code, at `error` (string), `error.code` or
/// `error.type`.
fn token_error_code(body: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    match value.get("error")? {
        Value::String(code) => Some(code.clone()),
        Value::Object(error) => error
            .get("code")
            .or_else(|| error.get("type"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        _ => None,
    }
}

impl OAuthAuthorizationCode for Claudecode {
    /// `GET https://claude.com/cai/oauth/authorize?code=true&...` with the
    /// full interactive scope (v3 `login.rs`).
    fn authorize<'a>(
        &'a self,
        context: LoginContext<'a>,
        request: AuthorizationRequest<'a>,
    ) -> OperationFuture<'a, AuthorizationStart> {
        Box::pin(async move {
            let config = ClaudecodeConfig::from_view(context.provider)?;
            let redirect_uri = if request.redirect_uri.trim().is_empty() {
                DEFAULT_REDIRECT_URI
            } else {
                request.redirect_uri
            };
            let query = form_encode(&[
                ("code", "true"),
                ("client_id", DEFAULT_CLIENT_ID),
                ("response_type", "code"),
                ("redirect_uri", redirect_uri),
                ("scope", LOGIN_SCOPE),
                ("code_challenge", request.code_challenge),
                ("code_challenge_method", "S256"),
                ("state", request.state),
            ]);
            Ok(AuthorizationStart {
                authorize_url: format!("{}?{query}", config.authorize_url.trim_end_matches('?')),
                redirect_uri: redirect_uri.to_owned(),
                // The exchange needs nothing this call learned.
                provider_state: BTreeMap::new(),
            })
        })
    }

    /// JSON `authorization_code` grant at the token endpoint, including the
    /// original `state`, then the profile for account facts (v3 `login.rs`).
    fn exchange<'a>(
        &'a self,
        context: LoginContext<'a>,
        grant: AuthorizationCode<'a>,
    ) -> OperationFuture<'a, OAuthCredential> {
        Box::pin(async move {
            let config = ClaudecodeConfig::from_view(context.provider)?;
            let body = json!({
                "grant_type": "authorization_code",
                "client_id": DEFAULT_CLIENT_ID,
                "code": grant.code,
                "redirect_uri": grant.redirect_uri,
                "code_verifier": grant.code_verifier,
                "state": grant.state,
            });
            let (status, _, bytes) = send(
                context.client,
                Method::POST,
                &config.token_url,
                json_headers(),
                Some(body.to_string().into_bytes()),
            )
            .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse {
                    status,
                    body: bytes,
                });
            }
            let tokens: TokenResponse =
                serde_json::from_slice(&bytes).map_err(|e| invalid_response(e.to_string()))?;
            if tokens.refresh_token.as_deref().is_none_or(str::is_empty) {
                return Err(invalid_response("token response has no refresh_token"));
            }
            let mut credential = credential_from_tokens(tokens, None, Vec::new(), BTreeMap::new())?;
            enrich_profile(
                context.client,
                &base_url(context.provider),
                &credential.access_token,
                &mut credential.provider_fields,
            )
            .await;
            Ok(credential)
        })
    }
}

impl CredentialRefresh for Claudecode {
    fn connection_purpose(&self, credential: &CredentialView<'_>) -> crate::channel::ConnectionPurpose {
        if non_empty(credential.secret.get("refresh_token")).is_none() && non_empty(credential.secret.get("cookie")).is_some() {
            crate::channel::ConnectionPurpose::CookieLogin
        } else {
            crate::channel::ConnectionPurpose::Request
        }
    }

    /// JSON `refresh_token` grant with the default scope plus any optional
    /// scopes the credential already holds (v3 `auth.rs`). A credential that
    /// only has a claude.ai cookie re-mints its tokens through the cookie.
    fn refresh<'a>(&'a self, context: RefreshContext<'a>) -> OperationFuture<'a, CredentialUpdate> {
        Box::pin(async move {
            let config = ClaudecodeConfig::from_view(context.provider)?;
            let secret = context.credential.secret;
            let cookie = non_empty(secret.get("cookie"));
            let refresh_token = match non_empty(secret.get("refresh_token")) {
                Some(token) => token,
                None if cookie.is_some() => {
                    return cookie::refresh(context, &config, cookie.unwrap_or_default()).await;
                }
                None => {
                    return Err(ChannelError::RefreshRejected(
                        "credential has no refresh token".into(),
                    ));
                }
            };
            let stored = previous_scopes(secret);
            let mut scopes = OAUTH_SCOPE.split(' ').collect::<Vec<_>>();
            for scope in stored.iter().map(String::as_str) {
                if PRESERVED_SCOPES.contains(&scope) && !scopes.contains(&scope) {
                    scopes.push(scope);
                }
            }
            let body = json!({
                "grant_type": "refresh_token",
                "client_id": DEFAULT_CLIENT_ID,
                "refresh_token": refresh_token,
                "scope": scopes.join(" "),
            });
            let (status, _, bytes) = send(
                context.client,
                Method::POST,
                &config.token_url,
                json_headers(),
                Some(body.to_string().into_bytes()),
            )
            .await?;
            if !status.is_success() {
                let code = token_error_code(&bytes).map(|c| c.to_ascii_lowercase());
                // The CLI treats `invalid_grant` as "log in again"
                // (`samples/claude-code-2.1.252/ANALYSIS.md`); a bare 401
                // means the refresh token itself was refused.
                let definitive = status == StatusCode::UNAUTHORIZED
                    || (status.is_client_error() && code.as_deref() == Some("invalid_grant"));
                if definitive {
                    return Err(ChannelError::RefreshRejected(
                        code.unwrap_or_else(|| format!("http {}", status.as_u16())),
                    ));
                }
                return Err(ChannelError::UpstreamResponse {
                    status,
                    body: bytes,
                });
            }
            let tokens: TokenResponse =
                serde_json::from_slice(&bytes).map_err(|e| invalid_response(e.to_string()))?;
            let credential = credential_from_tokens(
                tokens,
                Some(refresh_token.to_owned()),
                stored,
                previous_fields(secret),
            )?;
            Ok(CredentialUpdate {
                expires_at_ms: credential.expires_at_ms,
                secret: secret_json(&credential, cookie)?,
            })
        })
    }
}

impl CookieLogin for Claudecode {
    fn exchange_cookie<'a>(
        &'a self,
        context: LoginContext<'a>,
        cookie: &'a str,
    ) -> OperationFuture<'a, crate::channel::AcquiredCredential> {
        Box::pin(async move {
            let config = ClaudecodeConfig::from_view(context.provider)?;
            let (credential, cookie) =
                cookie::exchange(context.client, context.provider, &config, cookie).await?;
            let metadata = Value::Object(
                credential
                    .provider_fields
                    .iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect::<Map<_, _>>(),
            );
            Ok(crate::channel::AcquiredCredential {
                expires_at_ms: credential.expires_at_ms,
                secret: secret_json(&credential, Some(&cookie))?,
                metadata,
            })
        })
    }
}
