//! OpenAI Codex: a ChatGPT account used through the Codex backend.
//!
//! Wire facts follow the Codex CLI (`samples/codex`): OAuth at
//! `auth.openai.com` (authorization code with PKCE, and the device-code flow
//! under `/api/accounts/deviceauth`), refresh at `/oauth/token`, generation
//! at `{base}/responses` over HTTP SSE or WebSocket, account limits in the
//! `x-<limit>-primary/secondary-*` header families and at
//! `/backend-api/wham/usage`. The CLI's other backend calls (plugins, MCP,
//! settings, files, remote control, `whoami`) are `ChannelServices` in
//! `services.rs`. The credential secret is an `OAuthCredential`;
//! the account id and plan discovered at login travel in `provider_fields`
//! and, once the host persists them, in the credential's metadata.

mod agent;
mod services;

pub use agent::CLI_VERSION;

pub use services::{
    KIND_ENVIRONMENT, KIND_FILE, KIND_PLUGIN, KIND_REMOTE_SERVER, KIND_TASK, service_routes,
};

use crate::OutboundClient;
use crate::channel::{
    AuthorizationCode, AuthorizationRequest, AuthorizationStart, BaseChannel, ChannelError,
    ChannelServices, CredentialContext, CredentialRefresh, CredentialUpdate, CredentialView,
    DeviceAuthorization, DevicePoll, HeaderAllowlist, LoginContext, NormalizedUsage,
    OAuthAuthorizationCode, OAuthCredential, OAuthDeviceCode, OperationFuture, PrepareContext,
    ProviderView, QuotaAllowance, QuotaBalance, QuotaDimension, QuotaEntry, QuotaHeaderContext,
    QuotaHeaders, QuotaMetric, QuotaModel, QuotaQuery, QuotaResetBehavior, QuotaScope,
    QuotaSnapshot, QuotaSubject, QuotaTracking, QuotaValue, QuotaWindow, RefreshContext,
    ResponseView, UsageCompleteness, UsageContext, UsageExtractor, UsageFrame, UsageObserver,
    UsageStream, UsageStreamContext, UsageStreamEnd, UsageTransport, forwardable,
};
use base64::Engine;
use futures_util::StreamExt;
use gproxy_client::{Backend, ConnectionConfig};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, WireRequest, WireResponse,
    codec::{CodecLimits, SseDecoder, SseFrame},
    connection::{Bytes, StreamFraming, WsFrame},
};
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header};
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const ID: &str = "codex";
pub const DEFAULT_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";
pub const DEFAULT_ISSUER: &str = "https://auth.openai.com";
pub const DEFAULT_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
pub const DEFAULT_ORIGINATOR: &str = "codex_cli_rs";
const OAUTH_SCOPE: &str =
    "openid profile email offline_access api.connectors.read api.connectors.invoke";
const RESPONSES_WS_BETA: &str = "responses_websockets=2026-02-06";
/// Responses bodies the channel reads itself (refresh, login, usage) are small.
const MAX_SERVICE_BODY: usize = 1024 * 1024;
/// Bounds for watching a Responses SSE stream; the host enforces the real
/// transfer limits, this only keeps the observer's buffers finite.
const SSE_LIMITS: CodecLimits = CodecLimits {
    max_buffer_bytes: 4 * 1024 * 1024,
    max_value_bytes: 4 * 1024 * 1024,
    max_body_bytes: u64::MAX,
    max_line_bytes: 4 * 1024 * 1024,
    max_part_bytes: 0,
    max_parts: 0,
};
const PRIMARY_WINDOW_SECS: i64 = 5 * 60 * 60;
const SECONDARY_WINDOW_SECS: i64 = 7 * 24 * 60 * 60;

/// Provider `config` JSON understood by this channel. Unknown keys are ignored.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct CodexConfig {
    /// OAuth issuer for login and refresh.
    pub issuer: String,
    pub client_id: String,
    /// The `originator` the backend sees; Codex CLI's by default.
    pub originator: String,
    /// Replaces the CLI-shaped `User-Agent`
    /// (`codex_cli_rs/<version> (<os> <version>; <arch>) <terminal>`).
    pub user_agent: Option<String>,
    /// Static headers added to every backend request.
    pub headers: BTreeMap<String, String>,
}

impl Default for CodexConfig {
    fn default() -> Self {
        Self {
            issuer: DEFAULT_ISSUER.into(),
            client_id: DEFAULT_CLIENT_ID.into(),
            originator: DEFAULT_ORIGINATOR.into(),
            user_agent: None,
            headers: BTreeMap::new(),
        }
    }
}

impl CodexConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Codex;

/// The Codex CLI's transport: reqwest 0.12 with its default features, so
/// native TLS (OpenSSL on Linux) for HTTP with h2's stock SETTINGS, no
/// response decompression, redirects followed; WebSocket over rustls
/// (`tokio-tungstenite`), which is what the pool's WebSocket client of this
/// backend is. The default client for providers that name no connection
/// profile. Host profiles override it whole.
pub fn default_connection() -> ConnectionConfig {
    ConnectionConfig {
        backend: Backend::ReqwestNative,
        emulation: None,
        redirect_max_hops: 10,
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

/// The Codex API origin (`.../backend-api/codex`) and the ChatGPT backend it
/// hangs off (`.../backend-api`), which serves account endpoints.
fn base_urls(provider: ProviderView<'_>) -> (String, String) {
    let base = provider
        .base_url
        .unwrap_or(DEFAULT_BASE_URL)
        .trim_end_matches('/')
        .to_owned();
    let backend = base
        .strip_suffix("/codex")
        .map(str::to_owned)
        .unwrap_or_else(|| base.clone());
    (base, backend)
}

/// Facts about the account from the secret and the host-persisted metadata.
struct Account<'a> {
    access_token: &'a str,
    account_id: Option<String>,
}

fn account<'a>(credential: &CredentialView<'a>) -> Result<Account<'a>, ChannelError> {
    let access_token = credential
        .secret
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|t| !t.is_empty())
        .ok_or(ChannelError::InvalidCredential)?;
    let account_id = credential
        .metadata
        .get("chatgpt_account_id")
        .or_else(|| {
            credential
                .secret
                .pointer("/provider_fields/chatgpt_account_id")
        })
        .and_then(Value::as_str)
        .map(str::to_owned);
    Ok(Account {
        access_token,
        account_id,
    })
}

fn plan_type(credential: &CredentialView<'_>) -> Option<String> {
    credential
        .metadata
        .get("plan_type")
        .or_else(|| {
            credential
                .secret
                .pointer("/provider_fields/chatgpt_plan_type")
        })
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// Headers every backend call carries: bearer token, account, originator,
/// static config headers. Source authentication and the channel's own
/// identity headers are never forwarded; `config.allowed_headers` narrows
/// the rest.
fn backend_headers(
    config: &CodexConfig,
    account: &Account<'_>,
    source: Option<(&HeaderMap, Option<&HeaderAllowlist>)>,
) -> Result<HeaderMap, ChannelError> {
    let mut headers = match source {
        Some((source, allowlist)) => forwardable(
            source,
            allowlist,
            &["chatgpt-account-id", "originator", "openai-beta"],
        ),
        None => HeaderMap::new(),
    };
    headers.insert(
        header::AUTHORIZATION,
        header_value(&format!("Bearer {}", account.access_token))?,
    );
    if let Some(id) = &account.account_id {
        headers.insert(
            HeaderName::from_static("chatgpt-account-id"),
            header_value(id)?,
        );
    }
    headers.insert(
        HeaderName::from_static("originator"),
        header_value(&config.originator)?,
    );
    let agent = match &config.user_agent {
        Some(agent) => agent.clone(),
        None => agent::user_agent(&config.originator),
    };
    headers.insert(header::USER_AGENT, header_value(&agent)?);
    for (name, value) in &config.headers {
        headers.insert(
            HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| invalid_config(format!("header `{name}`")))?,
            HeaderValue::from_str(value).map_err(|_| invalid_config(format!("header `{name}`")))?,
        );
    }
    Ok(headers)
}

/// The backend path for an operation; the client's native path is not
/// trusted because the Codex backend mounts Responses under its own prefix.
fn operation_path(operation: Operation) -> Result<&'static str, ChannelError> {
    Ok(match operation {
        Operation::GenerateContent | Operation::StreamGenerateContent => "/responses",
        Operation::CompactContent => "/responses/compact",
        Operation::SummarizeMemory => "/memories/trace_summarize",
        Operation::CreateRealtimeCall => "/realtime/calls",
        other => {
            return Err(ChannelError::UnsupportedOperation(
                gproxy_protocol::OperationKey {
                    operation: other,
                    dialect: Dialect::OpenAi,
                },
            ));
        }
    })
}

impl Codex {
    fn build<B>(
        &self,
        ctx: PrepareContext<'_, B>,
        websocket: bool,
    ) -> Result<(http::request::Builder, WireRequest<B>), ChannelError> {
        let config = CodexConfig::from_view(ctx.provider)?;
        let account = account(&ctx.credential)?;
        let mut request = ctx.request;
        let url = match ctx.endpoint_override {
            Some(url) => url.to_owned(),
            None => {
                let (base, _) = base_urls(ctx.provider);
                format!("{base}{}", operation_path(ctx.operation.operation)?)
            }
        };
        let url = if websocket {
            url.replacen("https://", "wss://", 1)
                .replacen("http://", "ws://", 1)
        } else {
            url
        };
        let query = request.query.take().filter(|q| !q.is_empty());
        let uri = match query {
            Some(q) => format!("{url}?{q}"),
            None => url,
        };
        let allowlist = HeaderAllowlist::from_view(ctx.provider)?;
        let mut headers = backend_headers(
            &config,
            &account,
            Some((&request.headers, allowlist.as_ref())),
        )?;
        if websocket {
            headers.insert(
                HeaderName::from_static("openai-beta"),
                HeaderValue::from_static(RESPONSES_WS_BETA),
            );
        }
        let mut builder = http::Request::builder()
            .method(request.method.clone())
            .uri(uri);
        if let Some(map) = builder.headers_mut() {
            *map = headers;
        }
        Ok((builder, request))
    }
}

impl BaseChannel for Codex {
    fn id(&self) -> &'static str {
        ID
    }

    fn default_connection(&self) -> Option<ConnectionConfig> {
        Some(default_connection())
    }

    fn native_dialects(&self, _provider: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::GenerateContent | Operation::StreamGenerateContent => {
                vec![Dialect::OpenAi, Dialect::OpenAiResponsesWebSocket]
            }
            Operation::CompactContent
            | Operation::SummarizeMemory
            | Operation::CreateRealtimeCall => vec![Dialect::OpenAi],
            _ => Vec::new(),
        }
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        let (builder, request) = self.build(ctx, false)?;
        builder
            .body(request.body)
            .map_err(|error| invalid_config(error.to_string()))
    }

    fn prepare_connect(
        &self,
        ctx: PrepareContext<'_, ()>,
    ) -> Result<http::Request<()>, ChannelError> {
        let (builder, _) = self.build(ctx, true)?;
        builder
            .body(())
            .map_err(|error| invalid_config(error.to_string()))
    }

    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        Some(self)
    }
    fn oauth_authorization_code(&self) -> Option<&dyn OAuthAuthorizationCode> {
        Some(self)
    }
    fn oauth_device_code(&self) -> Option<&dyn OAuthDeviceCode> {
        Some(self)
    }
    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        Some(self)
    }
    fn quota_model(&self) -> Option<&dyn QuotaModel> {
        Some(self)
    }
    fn quota_headers(&self) -> Option<&dyn QuotaHeaders> {
        Some(self)
    }
    fn usage_extractor(&self) -> Option<&dyn UsageExtractor> {
        Some(self)
    }
    fn usage_stream(&self) -> Option<&dyn UsageStream> {
        Some(self)
    }
    fn services(&self) -> Option<&dyn ChannelServices> {
        Some(self)
    }
}

// ---------------------------------------------------------------- tokens

/// Decode a JWT payload without verifying it: the issuer's tokens are
/// trusted material already; the claims only supply account facts.
fn jwt_claims(token: &str) -> Option<Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn jwt_expiry_ms(token: &str) -> Option<i64> {
    jwt_claims(token)?
        .get("exp")?
        .as_i64()
        .and_then(|secs| secs.checked_mul(1000))
}

/// Account facts from the id token: the ChatGPT account, plan and user under
/// the `https://api.openai.com/auth` claim, the email under `profile`.
fn provider_fields(id_token: Option<&str>) -> BTreeMap<String, Value> {
    let mut fields = BTreeMap::new();
    let Some(claims) = id_token.and_then(jwt_claims) else {
        return fields;
    };
    if let Some(auth) = claims.get("https://api.openai.com/auth") {
        for (from, to) in [
            ("chatgpt_account_id", "chatgpt_account_id"),
            ("chatgpt_plan_type", "chatgpt_plan_type"),
            ("chatgpt_user_id", "chatgpt_user_id"),
        ] {
            if let Some(value) = auth.get(from).filter(|v| !v.is_null()) {
                fields.insert(to.to_owned(), value.clone());
            }
        }
        if !fields.contains_key("chatgpt_user_id")
            && let Some(user) = auth.get("user_id").filter(|v| !v.is_null())
        {
            fields.insert("chatgpt_user_id".to_owned(), user.clone());
        }
    }
    let email = claims
        .pointer("/https:~1~1api.openai.com~1profile/email")
        .or_else(|| claims.get("email"))
        .filter(|v| !v.is_null());
    if let Some(email) = email {
        fields.insert("email".to_owned(), email.clone());
    }
    fields
}

fn credential_from_tokens(
    access_token: String,
    refresh_token: Option<String>,
    id_token: Option<String>,
) -> OAuthCredential {
    OAuthCredential {
        expires_at_ms: jwt_expiry_ms(&access_token),
        provider_fields: provider_fields(id_token.as_deref()),
        access_token,
        refresh_token,
        id_token,
        token_type: Some("Bearer".into()),
        scopes: OAUTH_SCOPE.split(' ').map(str::to_owned).collect(),
        refresh_expires_at_ms: None,
    }
}

async fn read_body(body: HttpBody) -> Result<Bytes, ChannelError> {
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

async fn send_json(
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
        .body(match body {
            Some(bytes) => HttpBody::Bytes(Bytes::from(bytes)),
            None => HttpBody::Bytes(Bytes::new()),
        })
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
    headers
}

fn form_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/x-www-form-urlencoded"),
    );
    headers
}

fn form_encode(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={}", form_urlencoded_encode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn form_urlencoded_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            b' ' => out.push('+'),
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    id_token: Option<String>,
}

/// The token endpoint's error code, at `error.code`, `error` or `code`.
fn token_error_code(body: &[u8]) -> Option<String> {
    let value: Value = serde_json::from_slice(body).ok()?;
    match value.get("error") {
        Some(Value::Object(error)) => error.get("code").and_then(Value::as_str).map(str::to_owned),
        Some(Value::String(code)) => Some(code.clone()),
        _ => value.get("code").and_then(Value::as_str).map(str::to_owned),
    }
}

async fn exchange_code(
    client: &dyn OutboundClient,
    config: &CodexConfig,
    code: &str,
    redirect_uri: &str,
    code_verifier: &str,
) -> Result<OAuthCredential, ChannelError> {
    let url = format!("{}/oauth/token", config.issuer.trim_end_matches('/'));
    let body = form_encode(&[
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", redirect_uri),
        ("client_id", &config.client_id),
        ("code_verifier", code_verifier),
    ]);
    let (status, _, bytes) = send_json(
        client,
        Method::POST,
        &url,
        form_headers(),
        Some(body.into_bytes()),
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
    let access_token = tokens
        .access_token
        .filter(|t| !t.is_empty())
        .ok_or_else(|| invalid_response("token exchange returned no access_token"))?;
    Ok(credential_from_tokens(
        access_token,
        tokens.refresh_token,
        tokens.id_token,
    ))
}

impl OAuthAuthorizationCode for Codex {
    fn authorize<'a>(
        &'a self,
        context: LoginContext<'a>,
        request: AuthorizationRequest<'a>,
    ) -> OperationFuture<'a, AuthorizationStart> {
        Box::pin(async move {
            let config = CodexConfig::from_view(context.provider)?;
            let query = form_encode(&[
                ("response_type", "code"),
                ("client_id", &config.client_id),
                ("redirect_uri", request.redirect_uri),
                ("scope", OAUTH_SCOPE),
                ("code_challenge", request.code_challenge),
                ("code_challenge_method", "S256"),
                ("id_token_add_organizations", "true"),
                ("codex_cli_simplified_flow", "true"),
                ("state", request.state),
                ("originator", &config.originator),
            ]);
            Ok(AuthorizationStart {
                authorize_url: format!(
                    "{}/oauth/authorize?{query}",
                    config.issuer.trim_end_matches('/')
                ),
                redirect_uri: request.redirect_uri.to_owned(),
            })
        })
    }

    fn exchange<'a>(
        &'a self,
        context: LoginContext<'a>,
        grant: AuthorizationCode<'a>,
    ) -> OperationFuture<'a, OAuthCredential> {
        Box::pin(async move {
            let config = CodexConfig::from_view(context.provider)?;
            exchange_code(
                context.client,
                &config,
                grant.code,
                grant.redirect_uri,
                grant.code_verifier,
            )
            .await
        })
    }
}

#[derive(Deserialize)]
struct UserCodeResponse {
    device_auth_id: String,
    #[serde(alias = "usercode")]
    user_code: String,
    #[serde(default, deserialize_with = "lenient_u64")]
    interval: u64,
}

fn lenient_u64<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<u64, D::Error> {
    let value = Value::deserialize(deserializer)?;
    Ok(match value {
        Value::Number(n) => n.as_u64().unwrap_or(5),
        Value::String(s) => s.trim().parse().unwrap_or(5),
        _ => 5,
    })
}

#[derive(Deserialize)]
struct DeviceTokenResponse {
    authorization_code: String,
    code_verifier: String,
}

impl OAuthDeviceCode for Codex {
    fn start<'a>(&'a self, context: LoginContext<'a>) -> OperationFuture<'a, DeviceAuthorization> {
        Box::pin(async move {
            let config = CodexConfig::from_view(context.provider)?;
            let issuer = config.issuer.trim_end_matches('/');
            let (status, _, bytes) = send_json(
                context.client,
                Method::POST,
                &format!("{issuer}/api/accounts/deviceauth/usercode"),
                json_headers(),
                Some(
                    json!({"client_id": config.client_id})
                        .to_string()
                        .into_bytes(),
                ),
            )
            .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse {
                    status,
                    body: bytes,
                });
            }
            let response: UserCodeResponse =
                serde_json::from_slice(&bytes).map_err(|e| invalid_response(e.to_string()))?;
            Ok(DeviceAuthorization {
                device_code: response.device_auth_id,
                user_code: response.user_code,
                verification_uri: format!("{issuer}/codex/device"),
                verification_uri_complete: None,
                // The issuer allows fifteen minutes; the host knows the clock.
                expires_at_ms: None,
                interval_secs: response.interval.max(1),
                provider_state: BTreeMap::new(),
            })
        })
    }

    fn poll<'a>(
        &'a self,
        context: LoginContext<'a>,
        authorization: &'a DeviceAuthorization,
    ) -> OperationFuture<'a, DevicePoll> {
        Box::pin(async move {
            let config = CodexConfig::from_view(context.provider)?;
            let issuer = config.issuer.trim_end_matches('/');
            let (status, _, bytes) = send_json(
                context.client,
                Method::POST,
                &format!("{issuer}/api/accounts/deviceauth/token"),
                json_headers(),
                Some(
                    json!({
                        "device_auth_id": authorization.device_code,
                        "user_code": authorization.user_code,
                    })
                    .to_string()
                    .into_bytes(),
                ),
            )
            .await?;
            match status {
                StatusCode::FORBIDDEN | StatusCode::NOT_FOUND => return Ok(DevicePoll::Pending),
                StatusCode::TOO_MANY_REQUESTS => {
                    return Ok(DevicePoll::SlowDown {
                        interval_secs: authorization.interval_secs.saturating_mul(2).max(5),
                    });
                }
                StatusCode::GONE => return Ok(DevicePoll::Expired),
                status if !status.is_success() => {
                    return Err(ChannelError::UpstreamResponse {
                        status,
                        body: bytes,
                    });
                }
                _ => {}
            }
            let response: DeviceTokenResponse =
                serde_json::from_slice(&bytes).map_err(|e| invalid_response(e.to_string()))?;
            let credential = exchange_code(
                context.client,
                &config,
                &response.authorization_code,
                &format!("{issuer}/deviceauth/callback"),
                &response.code_verifier,
            )
            .await?;
            Ok(DevicePoll::Ready(credential))
        })
    }
}

impl CredentialRefresh for Codex {
    fn refresh<'a>(&'a self, context: RefreshContext<'a>) -> OperationFuture<'a, CredentialUpdate> {
        Box::pin(async move {
            let config = CodexConfig::from_view(context.provider)?;
            let secret = context.credential.secret;
            let refresh_token = secret
                .get("refresh_token")
                .and_then(Value::as_str)
                .filter(|t| !t.is_empty())
                .ok_or_else(|| {
                    ChannelError::RefreshRejected("credential has no refresh token".into())
                })?;
            let url = format!("{}/oauth/token", config.issuer.trim_end_matches('/'));
            let body = json!({
                "client_id": config.client_id,
                "grant_type": "refresh_token",
                "refresh_token": refresh_token,
            });
            let (status, _, bytes) = send_json(
                context.client,
                Method::POST,
                &url,
                json_headers(),
                Some(body.to_string().into_bytes()),
            )
            .await?;
            if !status.is_success() {
                let code = token_error_code(&bytes).map(|c| c.to_ascii_lowercase());
                let definitive = status == StatusCode::UNAUTHORIZED
                    || matches!(
                        code.as_deref(),
                        Some(
                            "refresh_token_expired"
                                | "refresh_token_reused"
                                | "refresh_token_invalidated"
                        )
                    )
                    || (status == StatusCode::BAD_REQUEST
                        && code.as_deref() == Some("invalid_grant"));
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
            let access_token = tokens
                .access_token
                .filter(|t| !t.is_empty())
                .ok_or_else(|| invalid_response("refresh returned no access_token"))?;
            let refresh_token = tokens
                .refresh_token
                .filter(|t| !t.is_empty())
                .or_else(|| Some(refresh_token.to_owned()));
            let id_token = tokens.id_token.filter(|t| !t.is_empty()).or_else(|| {
                secret
                    .get("id_token")
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            });
            let mut credential = credential_from_tokens(access_token, refresh_token, id_token);
            // Keep account facts a login recorded that the new id token lacks.
            if let Some(Value::Object(previous)) = secret.get("provider_fields") {
                for (key, value) in previous {
                    credential
                        .provider_fields
                        .entry(key.clone())
                        .or_insert_with(|| value.clone());
                }
            }
            let expires_at_ms = credential.expires_at_ms;
            let secret =
                serde_json::to_value(&credential).map_err(|e| invalid_response(e.to_string()))?;
            Ok(CredentialUpdate {
                secret,
                expires_at_ms,
            })
        })
    }
}

// ----------------------------------------------------------------- quota

fn window_dimension(id: &str, label: &str, seconds: i64) -> QuotaDimension {
    QuotaDimension {
        id: id.to_owned(),
        label: Some(label.to_owned()),
        scope: QuotaScope::All,
        operations: None,
        metric: QuotaMetric::Unit("percent".into()),
        window: QuotaWindow::Rolling { seconds },
        limit: Some(Decimal::ONE_HUNDRED),
        tracking: QuotaTracking::Reported,
    }
}

impl QuotaModel for Codex {
    /// Every ChatGPT plan has the account's 5-hour and 7-day windows; feature
    /// limits (`x-<feature>-*`, `additional_rate_limits`) are observed as
    /// cycles under their own ids without a declared dimension.
    fn dimensions(
        &self,
        _: ProviderView<'_>,
        credential: CredentialView<'_>,
    ) -> Vec<QuotaDimension> {
        let plan = plan_type(&credential).unwrap_or_else(|| "unknown".into());
        vec![
            window_dimension(
                "codex_primary",
                &format!("{plan} 5h window"),
                PRIMARY_WINDOW_SECS,
            ),
            window_dimension(
                "codex_secondary",
                &format!("{plan} 7d window"),
                SECONDARY_WINDOW_SECS,
            ),
        ]
    }
}

fn percent_entry(
    id: String,
    label: Option<String>,
    used_percent: Decimal,
    window_seconds: Option<i64>,
    reset_at_secs: Option<i64>,
) -> QuotaEntry {
    let period_end_ms = reset_at_secs.and_then(|s| s.checked_mul(1000));
    let period_start_ms = match (period_end_ms, window_seconds) {
        (Some(end), Some(seconds)) => seconds.checked_mul(1000).map(|w| end - w),
        _ => None,
    };
    QuotaEntry {
        source_id: id.clone(),
        id,
        label,
        subject: QuotaSubject::Account,
        model_scope: QuotaScope::All,
        value: QuotaValue::Window(QuotaAllowance {
            used: Some(used_percent),
            limit: Some(Decimal::ONE_HUNDRED),
            remaining: Some((Decimal::ONE_HUNDRED - used_percent).max(Decimal::ZERO)),
            used_percent: Some(used_percent),
            unlimited: None,
            unit: Some("percent".into()),
            period_start_ms,
            period_end_ms,
            reset_behavior: QuotaResetBehavior::Periodic,
        }),
    }
}

fn credits_entry(has_credits: bool, unlimited: bool, balance: Option<&str>) -> QuotaEntry {
    QuotaEntry {
        id: "codex_credits".into(),
        source_id: "codex_credits".into(),
        label: Some("credits".into()),
        subject: QuotaSubject::Account,
        model_scope: QuotaScope::All,
        value: QuotaValue::Balance(QuotaBalance {
            remaining: if unlimited {
                None
            } else if has_credits {
                balance.and_then(|b| b.trim().parse().ok())
            } else {
                Some(Decimal::ZERO)
            },
            unit: Some("credits".into()),
        }),
    }
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name)?.to_str().ok().map(str::trim)
}

fn header_decimal(headers: &HeaderMap, name: &str) -> Option<Decimal> {
    header_str(headers, name)?.parse().ok()
}

fn header_i64(headers: &HeaderMap, name: &str) -> Option<i64> {
    header_str(headers, name)?.parse().ok()
}

fn header_bool(headers: &HeaderMap, name: &str) -> Option<bool> {
    match header_str(headers, name)? {
        v if v.eq_ignore_ascii_case("true") || v == "1" => Some(true),
        v if v.eq_ignore_ascii_case("false") || v == "0" => Some(false),
        _ => None,
    }
}

impl QuotaHeaders for Codex {
    /// `x-<limit>-{primary,secondary}-{used-percent,window-minutes,reset-at}`
    /// for every limit family present (`codex` is the account, other names
    /// are metered features), plus `x-codex-credits-*`.
    fn observe(&self, context: QuotaHeaderContext<'_>) -> Result<Vec<QuotaEntry>, ChannelError> {
        let headers = context.headers;
        let mut families: Vec<String> = headers
            .keys()
            .filter_map(|name| {
                name.as_str()
                    .strip_suffix("-primary-used-percent")?
                    .strip_prefix("x-")
                    .map(str::to_owned)
            })
            .collect();
        families.sort();
        families.dedup();
        let mut entries = Vec::new();
        for family in families {
            let limit_id = family.replace('-', "_");
            let label = header_str(headers, &format!("x-{family}-limit-name"))
                .filter(|l| !l.is_empty())
                .map(str::to_owned);
            for window in ["primary", "secondary"] {
                let Some(used) =
                    header_decimal(headers, &format!("x-{family}-{window}-used-percent"))
                else {
                    continue;
                };
                let minutes = header_i64(headers, &format!("x-{family}-{window}-window-minutes"));
                let reset = header_i64(headers, &format!("x-{family}-{window}-reset-at"));
                entries.push(percent_entry(
                    format!("{limit_id}_{window}"),
                    label.clone(),
                    used,
                    minutes.map(|m| m * 60),
                    reset,
                ));
            }
        }
        if let (Some(has), Some(unlimited)) = (
            header_bool(headers, "x-codex-credits-has-credits"),
            header_bool(headers, "x-codex-credits-unlimited"),
        ) {
            entries.push(credits_entry(
                has,
                unlimited,
                header_str(headers, "x-codex-credits-balance"),
            ));
        }
        Ok(entries)
    }
}

#[derive(Deserialize, Default)]
struct UsagePayload {
    #[serde(default)]
    rate_limit: Option<RateLimitDetails>,
    #[serde(default)]
    credits: Option<CreditDetails>,
    #[serde(default)]
    additional_rate_limits: Option<Vec<AdditionalRateLimit>>,
}
#[derive(Deserialize, Default)]
struct RateLimitDetails {
    #[serde(default)]
    primary_window: Option<WindowSnapshot>,
    #[serde(default)]
    secondary_window: Option<WindowSnapshot>,
}
#[derive(Deserialize)]
struct WindowSnapshot {
    used_percent: Decimal,
    #[serde(default)]
    limit_window_seconds: Option<i64>,
    #[serde(default)]
    reset_at: Option<i64>,
}
#[derive(Deserialize)]
struct AdditionalRateLimit {
    limit_name: String,
    metered_feature: String,
    #[serde(default)]
    rate_limit: Option<RateLimitDetails>,
}
#[derive(Deserialize)]
struct CreditDetails {
    has_credits: bool,
    unlimited: bool,
    #[serde(default)]
    balance: Option<String>,
}

fn usage_entries(payload: &UsagePayload) -> Vec<QuotaEntry> {
    let mut entries = Vec::new();
    let mut push = |id: &str, label: Option<&str>, details: &RateLimitDetails| {
        for (window, snapshot) in [
            ("primary", &details.primary_window),
            ("secondary", &details.secondary_window),
        ] {
            if let Some(snapshot) = snapshot {
                entries.push(percent_entry(
                    format!("{id}_{window}"),
                    label.map(str::to_owned),
                    snapshot.used_percent,
                    snapshot.limit_window_seconds,
                    snapshot.reset_at,
                ));
            }
        }
    };
    if let Some(details) = &payload.rate_limit {
        push("codex", None, details);
    }
    for extra in payload.additional_rate_limits.iter().flatten() {
        if let Some(details) = &extra.rate_limit {
            push(
                &extra.metered_feature.to_ascii_lowercase().replace('-', "_"),
                Some(&extra.limit_name),
                details,
            );
        }
    }
    if let Some(credits) = &payload.credits {
        entries.push(credits_entry(
            credits.has_credits,
            credits.unlimited,
            credits.balance.as_deref(),
        ));
    }
    entries
}

impl QuotaQuery for Codex {
    fn query<'a>(&'a self, context: CredentialContext<'a>) -> OperationFuture<'a, QuotaSnapshot> {
        Box::pin(async move {
            let config = CodexConfig::from_view(context.provider)?;
            let account = account(&context.credential)?;
            let (_, backend) = base_urls(context.provider);
            let headers = backend_headers(&config, &account, None)?;
            let (status, _, bytes) = send_json(
                context.client,
                Method::GET,
                &format!("{backend}/wham/usage"),
                headers,
                None,
            )
            .await?;
            if !status.is_success() {
                return Err(ChannelError::UpstreamResponse {
                    status,
                    body: bytes,
                });
            }
            let payload: UsagePayload =
                serde_json::from_slice(&bytes).map_err(|e| invalid_response(e.to_string()))?;
            Ok(QuotaSnapshot {
                // The host stamps receipt; the payload carries no observation time.
                observed_at_ms: 0,
                entries: usage_entries(&payload),
            })
        })
    }
}

// ----------------------------------------------------------------- usage

/// Responses `usage` -> normalized. Reported input counts include cached
/// tokens; the normalized input excludes them.
fn usage_from_value(usage: &Value) -> Option<NormalizedUsage> {
    let input = usage.get("input_tokens")?.as_u64()?;
    let output = usage.get("output_tokens").and_then(Value::as_u64);
    let cached = usage
        .pointer("/input_tokens_details/cached_tokens")
        .and_then(Value::as_u64);
    let reasoning = usage
        .pointer("/output_tokens_details/reasoning_tokens")
        .and_then(Value::as_u64);
    let mut normalized = NormalizedUsage::default();
    normalized.tokens.input_tokens = Some(input.saturating_sub(cached.unwrap_or(0)));
    normalized.tokens.output_tokens = output;
    normalized.tokens.cached_input_tokens = cached;
    normalized.tokens.reasoning_tokens = reasoning;
    normalized.completeness = UsageCompleteness::Complete;
    Some(normalized)
}

fn usage_from_response_json(body: &[u8]) -> Option<NormalizedUsage> {
    let value: Value = serde_json::from_slice(body).ok()?;
    let usage = value
        .get("usage")
        .or_else(|| value.pointer("/response/usage"))?;
    usage_from_value(usage)
}

impl UsageExtractor for Codex {
    fn extract(&self, ctx: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        if !ctx.response.status.is_success() {
            return Ok(None);
        }
        Ok(usage_from_response_json(ctx.response.body))
    }
}

/// Watches a Responses stream for its terminal event's `usage`, over SSE
/// data frames or WebSocket text messages.
struct ResponsesUsageObserver {
    sse: Option<SseDecoder>,
    usage: Option<NormalizedUsage>,
}

impl ResponsesUsageObserver {
    fn see_event(&mut self, data: &str) {
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            return;
        };
        if matches!(
            value.get("type").and_then(Value::as_str),
            Some("response.completed" | "response.incomplete" | "response.done")
        ) && let Some(usage) = value.pointer("/response/usage").and_then(usage_from_value)
        {
            self.usage = Some(usage);
        }
    }
}

impl UsageObserver for ResponsesUsageObserver {
    fn observe(&mut self, frame: UsageFrame<'_>) -> Result<(), ChannelError> {
        match frame {
            UsageFrame::HttpChunk(chunk) => {
                let Some(decoder) = self.sse.as_mut() else {
                    return Ok(());
                };
                let frames = decoder
                    .push(chunk)
                    .map_err(|e| invalid_response(e.to_string()))?;
                for frame in frames {
                    if let SseFrame::Event(event) = frame {
                        self.see_event(&event.data);
                    }
                }
            }
            UsageFrame::WebSocket(WsFrame::Text(text)) => self.see_event(text),
            UsageFrame::WebSocket(_) => {}
        }
        Ok(())
    }
    fn snapshot(&self) -> Option<NormalizedUsage> {
        self.usage.clone()
    }
    fn finish(
        self: Box<Self>,
        _end: UsageStreamEnd,
    ) -> Result<Option<NormalizedUsage>, ChannelError> {
        Ok(self.usage)
    }
}

impl UsageStream for Codex {
    fn start(
        &self,
        context: UsageStreamContext<'_>,
    ) -> Result<Box<dyn UsageObserver>, ChannelError> {
        let sse = match context.transport {
            UsageTransport::Http {
                framing: Some(StreamFraming::Sse),
            } => Some(SseDecoder::new(SSE_LIMITS)),
            UsageTransport::Http { .. } => {
                return Err(ChannelError::InvalidResponse(
                    "Responses streams are SSE".into(),
                ));
            }
            UsageTransport::WebSocket => None,
        };
        Ok(Box::new(ResponsesUsageObserver { sse, usage: None }))
    }
}

#[allow(dead_code)]
fn _views(_: ResponseView<'_>) {}
