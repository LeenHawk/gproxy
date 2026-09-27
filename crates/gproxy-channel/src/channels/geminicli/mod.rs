//! Gemini CLI: a Google account used against the Code Assist internal
//! endpoints the `gemini` CLI talks to, rather than a Gemini API key.
//!
//! Wire facts follow the v3 channel
//! (`crates/gproxy-channels/src/geminicli` on `main`) and the CLI itself
//! (`samples/geminicli/packages/core/src/code_assist/`): a Google installed
//! app authorization code login with PKCE, the tool's own client id and
//! secret at `oauth2.googleapis.com/token`, and, once a token exists,
//! `loadCodeAssist`/`onboardUser` to learn the Cloud project and the
//! subscription tier. Requests go to `cloudcode-pa.googleapis.com` as
//! `POST /v1internal:generateContent`, `:streamGenerateContent?alt=sse` and
//! `:countTokens`, each carrying the Code Assist envelope
//! (`{model, project, user_prompt_id, request}`); the model directory and
//! the per-model quota both come from `POST /v1internal:retrieveUserQuota`.
//!
//! The credential secret is an `OAuthCredential`. The project and the tier
//! are facts a login discovers: they travel in `provider_fields`, the host
//! persists them on the credential row, and `prepare` reads them back from
//! the metadata. Preparation never discovers anything.
//!
//! Session identity: a Gemini CLI client names its session inside the Code
//! Assist envelope as `request.session_id` (`converter.ts`,
//! `VertexGenerateContentRequest.session_id`). The envelope this channel
//! builds nests the client's body under `request` unchanged, so a
//! `session_id` the client sent arrives at `request.session_id`; a client
//! that already sent a whole envelope keeps its own. The field is never
//! renamed, moved or dropped.

mod models;
mod oauth;
mod quota;

use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ChannelHeaders, ConfigKey,
    ConfigKeyKind, CredentialRefresh, CredentialView, HOST_CONFIG_KEYS, HeaderAllowlist, LoginMode,
    OAuthAuthorizationCode, OperationContext, OperationFuture, PrepareContext, ProviderView,
    QuotaQuery, forwardable,
};
use crate::channels::shared::code_assist;
use gproxy_client::{Alpn, Backend, ConnectionConfig, EmulationConfig, Fingerprint, TlsVersion};
use gproxy_protocol::{
    Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse, connection::Bytes,
};
use http::{HeaderMap, HeaderName, HeaderValue, Method, header};
use serde::Deserialize;
use serde_json::{Value, json};

pub const ID: &str = "geminicli";
/// The Code Assist host the CLI talks to (v3 `prepare.rs`).
pub const DEFAULT_BASE_URL: &str = "https://cloudcode-pa.googleapis.com";
pub const DEFAULT_AUTHORIZE_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
pub const DEFAULT_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
/// The CLI's own installed-app client (v3 `login.rs`, `auth.rs`).
pub const DEFAULT_CLIENT_ID: &str =
    "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com";
/// Shipped inside the tool, so an identifier rather than a secret; Google's
/// token endpoint rejects the grant without it.
pub const DEFAULT_CLIENT_SECRET: &str = "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";
/// The paste-the-code redirect the CLI registers (v3 `login.rs`).
pub const DEFAULT_REDIRECT_URI: &str = "https://codeassist.google.com/authcode";
/// The CLI's local redirect, for a host that can listen (v3 `login.rs`).
pub const LOOPBACK_REDIRECT_URI: &str = "http://127.0.0.1:1455/oauth2callback";
pub const OAUTH_SCOPE: &str = concat!(
    "https://www.googleapis.com/auth/cloud-platform ",
    "https://www.googleapis.com/auth/userinfo.email ",
    "https://www.googleapis.com/auth/userinfo.profile"
);
/// Tier used for onboarding when `loadCodeAssist` names no default.
const FALLBACK_TIER: &str = "legacy-tier";
/// The CLI version and platform triple the channel impersonates (v3
/// `prepare.rs::user_agent`, `login.rs`).
pub const CLI_USER_AGENT: &str =
    "GeminiCLI-tui/0.55.1 (linux; x64; terminal) google-api-nodejs-client/10.9.0";
/// The Node runtime banner the CLI's googleapis client sends (v3 `prepare.rs`).
pub const GOOG_API_CLIENT: &str = "gl-node/22.20.0";

/// Provider `config` JSON understood by this channel. Unknown keys ignored.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct GeminiCliConfig {
    pub authorize_url: String,
    pub token_url: String,
    /// Static headers added to every Code Assist request.
    pub headers: std::collections::BTreeMap<String, String>,
}

impl Default for GeminiCliConfig {
    fn default() -> Self {
        Self {
            authorize_url: DEFAULT_AUTHORIZE_URL.into(),
            token_url: DEFAULT_TOKEN_URL.into(),
            headers: std::collections::BTreeMap::new(),
        }
    }
}

impl GeminiCliConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct GeminiCli;

/// What the Gemini CLI itself sends. The channel sets all of them from the
/// credential and the provider configuration, so a provider allow-list can
/// never hide the identity being impersonated; a client's own copies are
/// dropped by `CHANNEL_HEADERS` instead of being forwarded.
pub const CLI_HEADERS: ChannelHeaders = ChannelHeaders {
    names: &["user-agent", "x-goog-api-client"],
    prefixes: &[],
};

/// Headers the channel owns; a client cannot supply them. Google account
/// cookies are browser identity and never belong on a bearer-token call.
const CHANNEL_HEADERS: &[&str] = &[
    "user-agent",
    "x-goog-api-client",
    "accept",
    "cookie",
    "x-goog-user-project",
];

/// The CLI's outbound stack, the default for a provider that names no
/// connection profile: Node 22 over OpenSSL, TLS 1.2 floor, the CLI's cipher
/// and curve order (X25519MLKEM768 first), no GREASE and no ALPN list of its
/// own (v3 `geminicli/profile.rs`).
pub fn default_connection() -> ConnectionConfig {
    ConnectionConfig {
        backend: Backend::Wreq,
        emulation: Some(EmulationConfig::Custom(Fingerprint {
            alpn: Vec::<Alpn>::new(),
            min_tls: Some(TlsVersion::Tls12),
            max_tls: Some(TlsVersion::Tls13),
            cipher_list: Some(
                concat!(
                    "TLS_AES_256_GCM_SHA384:TLS_AES_128_GCM_SHA256:TLS_CHACHA20_POLY1305_SHA256:",
                    "ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:",
                    "ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:",
                    "ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-CHACHA20-POLY1305"
                )
                .into(),
            ),
            curves_list: Some("X25519MLKEM768:X25519:P-256:P-384:P-521".into()),
            sigalgs_list: None,
            preserve_tls13_cipher_list: Some(false),
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
    fact(credential, "project_id").map(str::to_owned).ok_or(ChannelError::InvalidCredential)
}

fn header_value(value: &str) -> Result<HeaderValue, ChannelError> {
    HeaderValue::from_str(value).map_err(|_| ChannelError::InvalidCredential)
}

/// `GeminiCLI-tui/0.55.1/<model> (...)`: the CLI appends the model it is
/// about to call to its version (v3 `prepare.rs::user_agent`).
fn user_agent(model: &str) -> String {
    match model.trim() {
        "" => CLI_USER_AGENT.to_owned(),
        model => CLI_USER_AGENT.replacen(
            "GeminiCLI-tui/0.55.1 ",
            &format!("GeminiCLI-tui/0.55.1/{model} "),
            1,
        ),
    }
}

/// Headers every Code Assist call carries. `json_only` marks the catalogue
/// and quota calls, which the CLI makes with plain JSON `Accept` and without
/// the Node client banner (v3 `prepare.rs::apply_headers`).
pub(super) fn apply_headers(
    headers: &mut HeaderMap,
    config: &GeminiCliConfig,
    access_token: &str,
    model: &str,
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
    headers.insert(
        header::ACCEPT,
        HeaderValue::from_static(if json_only { "application/json" } else { "*/*" }),
    );
    headers.insert(
        header::USER_AGENT,
        header_value(&user_agent(model))?,
    );
    if !json_only {
        headers.insert(
            HeaderName::from_static("x-goog-api-client"),
            HeaderValue::from_static(GOOG_API_CLIENT),
        );
    }
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
        // The catalogue is synthesized from the quota buckets; that endpoint
        // is the only per-account model list Code Assist offers.
        Operation::ListModels | Operation::GetModel => "/v1internal:retrieveUserQuota",
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

impl GeminiCli {
    fn build(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        let config = GeminiCliConfig::from_view(ctx.provider)?;
        let token = access_token(&ctx.credential)?;
        let project = project(&ctx.credential)?;
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
            Operation::ListModels | Operation::GetModel => (
                code_assist::encode(&json!({ "project": project }), |message| {
                    ChannelError::InvalidConfig(message)
                })?,
                false,
            ),
            Operation::CountTokens => (code_assist::count_envelope(&buffered, &model)?, false),
            Operation::GenerateContent => {
                (code_assist::envelope(&buffered, &model, &project)?, false)
            }
            Operation::StreamGenerateContent => {
                (code_assist::envelope(&buffered, &model, &project)?, true)
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
            &model,
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

impl BaseChannel for GeminiCli {
    fn id(&self) -> &'static str {
        ID
    }

    /// A Google account through the Gemini CLI: a PKCE login that also
    /// discovers the Cloud project, a refreshable token, and the per-model
    /// quota buckets the CLI reads.
    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "Gemini CLI (Code Assist)",
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
                    "Code Assist origin; defaults to https://cloudcode-pa.googleapis.com. Provider column, not config JSON.",
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
}

/// The catalogue and quota calls send the same body to the same endpoint.
pub(super) fn quota_request(
    config: &GeminiCliConfig,
    provider: ProviderView<'_>,
    credential: &CredentialView<'_>,
) -> Result<(String, HeaderMap, Bytes), ChannelError> {
    let token = access_token(credential)?;
    let project = project(credential)?;
    let mut headers = HeaderMap::new();
    apply_headers(&mut headers, config, token, "", true)?;
    let body = code_assist::encode(&json!({ "project": project }), |message| {
        ChannelError::InvalidConfig(message)
    })?;
    Ok((
        format!("{}{}", base_url(provider), "/v1internal:retrieveUserQuota"),
        headers,
        body,
    ))
}
