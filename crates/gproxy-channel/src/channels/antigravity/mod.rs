//! Antigravity: a Google account used against the Code Assist host the
//! Antigravity editor's agent talks to, rather than a Gemini API key.
//!
//! Wire facts follow the v3 channel
//! (`crates/gproxy-channels/src/antigravity` on `main`) and the reverse
//! engineered client in
//! `samples/CLIProxyAPI/internal/runtime/executor/antigravity_executor_request.go`:
//! the same Google installed-app authorization code login as the Gemini CLI
//! but with Antigravity's own client id, secret, loopback redirect and the
//! three extra scopes (`cclog`, `experimentsandconfigs`, `aicode`), and the
//! same `loadCodeAssist`/`onboardUser` project discovery reporting
//! `ideType: ANTIGRAVITY`. Requests go to `daily-cloudcode-pa.googleapis.com`
//! as `POST /v1internal:generateContent`,
//! `:streamGenerateContent?alt=sse` and `:countTokens` carrying the Code
//! Assist envelope; the catalogue and the per-model quota both come from
//! `POST /v1internal:fetchAvailableModels` with an empty JSON body.
//!
//! The credential secret is an `OAuthCredential`. The project and the tier
//! are facts a login discovers: they travel in `provider_fields`, the host
//! persists them on the credential row, and `prepare` reads them back from
//! the metadata. Preparation never discovers anything.
//!
//! Session identity: an Antigravity client names its session inside the
//! envelope as `request.sessionId` (camel case, unlike the Gemini CLI's
//! `request.session_id`; `antigravity_executor_request.go` reads and writes
//! exactly that path). The envelope this channel builds nests the client's
//! body under `request` unchanged, so a `sessionId` the client sent arrives
//! at `request.sessionId`, and a client that already sent a whole envelope
//! keeps its own. The field is never renamed, moved or dropped.

mod claude;
mod models;
mod oauth;
mod quota;
mod usage;

use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ChannelHeaders, ConfigKey,
    ConfigKeyKind, CredentialRefresh, CredentialView, HOST_CONFIG_KEYS, HeaderAllowlist, LoginMode,
    OAuthAuthorizationCode, OperationContext, OperationFuture, PrepareContext, ProviderView,
    QuotaModel, QuotaQuery, UsageExtractor, UsageStream, forwardable,
};
use crate::channels::shared::code_assist;
use gproxy_client::{Alpn, Backend, ConnectionConfig, EmulationConfig, Fingerprint, TlsVersion};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse, connection::Bytes,
};
use http::{HeaderMap, HeaderName, HeaderValue, Method, header};
use serde::Deserialize;
use serde_json::Value;

pub const ID: &str = "antigravity";
/// The Code Assist host Antigravity talks to (v3 `prepare.rs`); note the
/// `daily-` prefix, which is a different deployment from the Gemini CLI's.
pub const DEFAULT_BASE_URL: &str = "https://daily-cloudcode-pa.googleapis.com";
pub const DEFAULT_AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const DEFAULT_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
/// Antigravity's own installed-app client (v3 `login.rs`, `auth.rs`).
pub const DEFAULT_CLIENT_ID: &str =
    "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com";
/// Shipped inside the tool, so an identifier rather than a secret.
pub const DEFAULT_CLIENT_SECRET: &str = "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";
/// The loopback callback the editor listens on (v3 `login.rs`).
pub const DEFAULT_REDIRECT_URI: &str = "http://localhost:51121/oauth-callback";
/// Antigravity asks for three scopes beyond the Gemini CLI's (v3 `login.rs`).
pub const OAUTH_SCOPE: &str = concat!(
    "https://www.googleapis.com/auth/cloud-platform ",
    "https://www.googleapis.com/auth/userinfo.email ",
    "https://www.googleapis.com/auth/userinfo.profile ",
    "https://www.googleapis.com/auth/cclog ",
    "https://www.googleapis.com/auth/experimentsandconfigs ",
    "https://www.googleapis.com/auth/aicode"
);
/// Tier used for onboarding when `loadCodeAssist` names no default; unlike
/// the Gemini CLI's `legacy-tier`, Antigravity spells it in upper case
/// (v3 `login.rs`).
const FALLBACK_TIER: &str = "LEGACY";
/// The editor's Go client banner (v3 `prepare.rs::USER_AGENT_VALUE`, and
/// `misc.AntigravityRequestUserAgent` in the CLIProxyAPI sample).
pub const CLI_USER_AGENT: &str = "antigravity/cli/1.0.6 linux/amd64";
/// The model Antigravity exposes as its own high reasoning tier; its
/// catalogue supplies this thinking budget as the default (v3
/// `prepare.rs::apply_model_defaults`).
const HIGH_REASONING_MODEL: &str = "gemini-3.1-pro-high";
const HIGH_REASONING_BUDGET: i64 = 10_001;

/// Provider `config` JSON understood by this channel. Unknown keys ignored.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct AntigravityConfig {
    pub authorize_url: String,
    pub token_url: String,
    /// Static headers added to every Code Assist request.
    pub headers: std::collections::BTreeMap<String, String>,
}

impl Default for AntigravityConfig {
    fn default() -> Self {
        Self {
            authorize_url: DEFAULT_AUTHORIZE_URL.into(),
            token_url: DEFAULT_TOKEN_URL.into(),
            headers: std::collections::BTreeMap::new(),
        }
    }
}

impl AntigravityConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Antigravity;

/// What the Antigravity client itself sends: only its user agent, which is
/// how the Code Assist host tells it apart from the Gemini CLI (v3
/// `policy::ANTIGRAVITY` forwarded exactly `user-agent` and nothing else).
/// The channel sets it from the credential and the provider configuration,
/// so a provider allow-list can never hide the identity being impersonated;
/// a client's own copy is dropped by `CHANNEL_HEADERS`.
pub const CLI_HEADERS: ChannelHeaders = ChannelHeaders {
    names: &["user-agent"],
    prefixes: &[],
};

/// Headers the channel owns; a client cannot supply them. Google account
/// cookies are browser identity and never belong on a bearer-token call.
const CHANNEL_HEADERS: &[&str] = &[
    "user-agent",
    "accept",
    "accept-encoding",
    "cookie",
    "x-goog-user-project",
];

/// The editor's outbound stack, the default for a provider that names no
/// connection profile: a Go client, TLS 1.2 floor, Go's cipher order with the
/// TLS 1.3 suites kept last and preserved, X25519MLKEM768 first, Go's
/// signature algorithm order, no GREASE (v3 `antigravity/profile.rs`).
pub fn default_connection() -> ConnectionConfig {
    ConnectionConfig {
        backend: Backend::Wreq,
        // Go's HTTP transport negotiates and decodes gzip. The downstream
        // client's encoding preferences cannot describe this channel's parser.
        gzip: true,
        emulation: Some(EmulationConfig::Custom(Fingerprint {
            alpn: Vec::<Alpn>::new(),
            min_tls: Some(TlsVersion::Tls12),
            max_tls: Some(TlsVersion::Tls13),
            cipher_list: Some(
                concat!(
                    "ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:",
                    "ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:",
                    "ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-CHACHA20-POLY1305:",
                    "ECDHE-ECDSA-AES128-SHA:ECDHE-RSA-AES128-SHA:",
                    "ECDHE-ECDSA-AES256-SHA:ECDHE-RSA-AES256-SHA:",
                    "TLS_AES_128_GCM_SHA256:TLS_AES_256_GCM_SHA384:TLS_CHACHA20_POLY1305_SHA256"
                )
                .into(),
            ),
            curves_list: Some("X25519MLKEM768:X25519:P-256:P-384:P-521".into()),
            sigalgs_list: Some(
                concat!(
                    "rsa_pss_rsae_sha256:ecdsa_secp256r1_sha256:ed25519:",
                    "rsa_pss_rsae_sha384:rsa_pss_rsae_sha512:rsa_pkcs1_sha256:",
                    "rsa_pkcs1_sha384:rsa_pkcs1_sha512:ecdsa_secp384r1_sha384:",
                    "ecdsa_secp521r1_sha512"
                )
                .into(),
            ),
            preserve_tls13_cipher_list: Some(true),
            grease: Some(false),
            extension_permutation: None,
            ocsp_stapling: None,
            signed_cert_timestamps: None,
            http2: None,
            headers: None,
        })),
        ..ConnectionConfig::default()
    }
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

/// A public account fact: host metadata first, then the secret's
/// `provider_fields`, then the flat layout v3 wrote.
pub(super) fn fact<'a>(credential: &CredentialView<'a>, name: &str) -> Option<&'a str> {
    fn non_empty(value: Option<&Value>) -> Option<&str> {
        value
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
    }
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

pub(super) fn access_token<'a>(credential: &CredentialView<'a>) -> Result<&'a str, ChannelError> {
    credential
        .secret
        .get("access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .ok_or(ChannelError::InvalidCredential)
}

/// The Cloud project discovered for this credential at login.
pub(super) fn project(credential: &CredentialView<'_>) -> Result<String, ChannelError> {
    fact(credential, "project_id")
        .map(str::to_owned)
        .ok_or(ChannelError::InvalidCredential)
}

fn header_value(value: &str) -> Result<HeaderValue, ChannelError> {
    HeaderValue::from_str(value).map_err(|_| ChannelError::InvalidCredential)
}

/// Headers every Code Assist call carries (v3 `prepare.rs`, `quota.rs`):
/// the bearer token, JSON content type and the editor's user agent. Unlike
/// the Gemini CLI, Antigravity sends no `x-goog-api-client`, and it asks for
/// `application/json` on the catalogue and quota call.
pub(super) fn apply_headers(
    headers: &mut HeaderMap,
    config: &AntigravityConfig,
    access_token: &str,
    json_only: bool,
) -> Result<(), ChannelError> {
    headers.insert(
        header::AUTHORIZATION,
        header_value(&format!("Bearer {access_token}"))?,
    );
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    if json_only {
        headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    }
    headers.insert(header::USER_AGENT, HeaderValue::from_static(CLI_USER_AGENT));
    for (name, value) in &config.headers {
        headers.insert(
            HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
                ChannelError::InvalidConfig(format!("header `{name}` is not a header name"))
            })?,
            HeaderValue::from_str(value).map_err(|_| {
                ChannelError::InvalidConfig(format!("header `{name}` has an invalid value"))
            })?,
        );
    }
    Ok(())
}

/// The Code Assist method for an operation (v3 `prepare.rs`). The client's
/// own `/v1beta/models/...` path is not trusted: Code Assist mounts every
/// operation on `/v1internal`.
fn operation_path(operation: Operation) -> Result<&'static str, ChannelError> {
    Ok(match operation {
        Operation::ListModels | Operation::GetModel => "/v1internal:fetchAvailableModels",
        Operation::CountTokens => "/v1internal:countTokens",
        Operation::GenerateContent => "/v1internal:generateContent",
        Operation::StreamGenerateContent => "/v1internal:streamGenerateContent",
        other => {
            return Err(ChannelError::UnsupportedOperation(OperationKey {
                operation: other,
                dialect: Dialect::Gemini,
            }));
        }
    })
}

/// Antigravity exposes its high reasoning tier as a distinct model whose
/// catalogue entry carries a default thinking budget; an explicit budget
/// from the caller is left alone (v3 `apply_model_defaults`).
fn apply_model_defaults(envelope: &mut Value, model: &str) {
    if code_assist::model_id(model) != HIGH_REASONING_MODEL {
        return;
    }
    let Some(request) = envelope.get_mut("request").and_then(Value::as_object_mut) else {
        return;
    };
    let Some(config) = request
        .entry("generationConfig")
        .or_insert_with(|| Value::Object(serde_json::Map::new()))
        .as_object_mut()
    else {
        return;
    };
    let Some(thinking) = config
        .entry("thinkingConfig")
        .or_insert_with(|| Value::Object(serde_json::Map::new()))
        .as_object_mut()
    else {
        return;
    };
    thinking
        .entry("thinkingBudget")
        .or_insert_with(|| Value::from(HIGH_REASONING_BUDGET));
}

impl Antigravity {
    fn build(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        let config = AntigravityConfig::from_view(ctx.provider)?;
        let token = access_token(&ctx.credential)?;
        let operation = ctx.operation.operation;
        let allowlist = HeaderAllowlist::from_view_for(ctx.provider, CLI_HEADERS)?;
        let WireRequest {
            path,
            headers: source,
            body,
            ..
        } = ctx.request;

        let buffered = match body {
            HttpBody::Bytes(bytes) => bytes,
            HttpBody::Stream(_) => {
                return Err(ChannelError::InvalidConfig(
                    "the Code Assist envelope needs a buffered request body".into(),
                ));
            }
        };
        let parsed = serde_json::from_slice::<Value>(&buffered).ok();
        let model = code_assist::request_model(&path, parsed.as_ref()).unwrap_or_default();

        let (body, stream) = match operation {
            // The catalogue call takes an empty body and no project.
            Operation::ListModels | Operation::GetModel => (Bytes::from_static(b"{}"), false),
            Operation::CountTokens => (code_assist::count_envelope(&buffered, &model)?, false),
            Operation::GenerateContent | Operation::StreamGenerateContent => {
                let project = project(&ctx.credential)?;
                let bytes = code_assist::envelope(&buffered, &model, &project)?;
                let mut envelope: Value = serde_json::from_slice(&bytes)
                    .map_err(|error| ChannelError::InvalidConfig(error.to_string()))?;
                apply_model_defaults(&mut envelope, &model);
                if claude::is_claude(&model)
                    && let Some(request) = envelope.get_mut("request")
                {
                    claude::declare_tools(request);
                    claude::tidy_history(request);
                    claude::apply_limits(request, parsed.as_ref().and_then(claude::output_limit));
                }
                (
                    code_assist::encode(&envelope, ChannelError::InvalidConfig)?,
                    operation == Operation::StreamGenerateContent,
                )
            }
            other => {
                return Err(ChannelError::UnsupportedOperation(OperationKey {
                    operation: other,
                    dialect: ctx.operation.dialect,
                }));
            }
        };

        let url = match ctx.endpoint_override {
            Some(url) => url.to_owned(),
            None => format!("{}{}", base_url(ctx.provider), operation_path(operation)?),
        };
        // Code Assist takes no client query beyond the streaming marker; the
        // client's own `?key=` and paging parameters are dropped.
        let uri = match (stream, url.contains('?')) {
            (true, false) => format!("{url}?alt=sse"),
            (true, true) => format!("{url}&alt=sse"),
            (false, _) => url,
        };

        let mut headers = forwardable(&source, allowlist.as_ref(), CHANNEL_HEADERS);
        apply_headers(
            &mut headers,
            &config,
            token,
            matches!(operation, Operation::ListModels | Operation::GetModel),
        )?;
        let mut builder = http::Request::builder().method(Method::POST).uri(uri);
        if let Some(map) = builder.headers_mut() {
            *map = headers;
        }
        builder
            .body(HttpBody::Bytes(body))
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }

    /// Buffered generation: one JSON object with the Gemini reply under
    /// `response`, which is unwrapped before the host sees it.
    async fn generate(
        &self,
        operation: Operation,
        ctx: OperationContext<'_>,
    ) -> Result<WireResponse<HttpBody>, ChannelError> {
        let OperationContext {
            provider,
            credential,
            dialect,
            request,
            client,
            endpoint_override,
            ..
        } = ctx;
        let prepared = self.build(PrepareContext {
            provider,
            credential,
            operation: OperationKey { operation, dialect },
            request,
            endpoint_override,
        })?;
        let mut response = client.send(prepared).await?;
        if !response.status.is_success() {
            return Ok(response);
        }
        if operation == Operation::StreamGenerateContent {
            response.headers.remove(header::CONTENT_LENGTH);
            response.body = code_assist::stream::unwrap_sse(response.body);
            return Ok(response);
        }
        let bytes = code_assist::read_body(response.body).await?;
        let unwrapped = code_assist::unwrap_body(&bytes)?;
        response.headers.remove(header::CONTENT_LENGTH);
        response.body = HttpBody::Bytes(unwrapped);
        Ok(response)
    }
}

impl BaseChannel for Antigravity {
    fn id(&self) -> &'static str {
        ID
    }

    /// A Google account through the Antigravity editor: a PKCE login that
    /// also discovers the Cloud project, a refreshable token, and the
    /// per-model quota the catalogue call reports.
    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "Antigravity (Code Assist)",
            login_modes: vec![LoginMode::AuthorizationCode],
            capabilities: ChannelCapabilities {
                refresh: true,
                quota_query: true,
                quota_reset: false,
                services: false,
                websocket: false,
            },
            config_keys: [
                ConfigKey::optional(
                    "base_url",
                    ConfigKeyKind::String,
                    "Code Assist origin; defaults to https://daily-cloudcode-pa.googleapis.com. Provider column, not config JSON.",
                ).with_placeholder(DEFAULT_BASE_URL),
                ConfigKey::optional(
                    "authorize_url",
                    ConfigKeyKind::String,
                    "Google OAuth authorization endpoint for the browser step.",
                ).with_placeholder(DEFAULT_AUTHORIZE_URL),
                ConfigKey::optional(
                    "token_url",
                    ConfigKeyKind::String,
                    "Google OAuth token endpoint used by the code exchange and by refresh.",
                ).with_placeholder(DEFAULT_TOKEN_URL),
                ConfigKey::optional(
                    "headers",
                    ConfigKeyKind::HeaderList,
                    "Static headers added to every Code Assist request.",
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

    fn native_dialects(&self, _provider: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::GenerateContent
            | Operation::StreamGenerateContent
            | Operation::CountTokens
            | Operation::ListModels
            | Operation::GetModel => vec![Dialect::Gemini],
            _ => Vec::new(),
        }
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        self.build(ctx)
    }

    fn list_models<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(models::invoke(self, Operation::ListModels, context))
    }

    fn get_model<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(models::invoke(self, Operation::GetModel, context))
    }

    fn generate_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(self.generate(Operation::GenerateContent, context))
    }

    fn stream_generate_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(self.generate(Operation::StreamGenerateContent, context))
    }

    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        Some(self)
    }
    fn oauth_authorization_code(&self) -> Option<&dyn OAuthAuthorizationCode> {
        Some(self)
    }
    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        Some(self)
    }
    fn quota_model(&self) -> Option<&dyn QuotaModel> {
        Some(self)
    }
    fn usage_extractor(&self) -> Option<&dyn UsageExtractor> {
        Some(self)
    }
    fn usage_stream(&self) -> Option<&dyn UsageStream> {
        Some(self)
    }
}

/// The catalogue and quota calls send the same empty body to the same
/// endpoint; unlike the Gemini CLI's, this one needs no project.
pub(super) fn catalog_request(
    config: &AntigravityConfig,
    provider: ProviderView<'_>,
    credential: &CredentialView<'_>,
) -> Result<(String, HeaderMap, Bytes), ChannelError> {
    let token = access_token(credential)?;
    let mut headers = HeaderMap::new();
    apply_headers(&mut headers, config, token, true)?;
    Ok((
        format!(
            "{}{}",
            base_url(provider),
            "/v1internal:fetchAvailableModels"
        ),
        headers,
        Bytes::from_static(b"{}"),
    ))
}
