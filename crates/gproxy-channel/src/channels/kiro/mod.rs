//! Kiro: an AWS CodeWhisperer subscription used the way the Kiro desktop app
//! and its CLI use it.
//!
//! Wire facts follow the v3 channel (`crates/gproxy-channels/src/kiro` on
//! `main`), which was calibrated against the app's own traffic. Nothing here
//! is an AWS public API:
//!
//! * **Transport.** Both planes are Smithy JSON 1.0 over `POST /`:
//!   `runtime.{region}.kiro.dev` serves
//!   `AmazonCodeWhispererStreamingService.GenerateAssistantResponse` and
//!   `management.{region}.kiro.dev` serves
//!   `AmazonCodeWhispererService.ListAvailableModels` and `GetUsageLimits`.
//!   The operation is named in `x-amz-target`, never in the path, and every
//!   request carries the AWS SDK's own `user-agent`, `x-amz-user-agent`,
//!   `amz-sdk-request` and `amz-sdk-invocation-id`.
//! * **Request.** The generation body is not OpenAI's. `conversationState`
//!   holds a `history` of alternating user and assistant turns plus one
//!   `currentMessage` that must be a user turn, with tools and tool results
//!   hanging off `userInputMessageContext` (`request/`). The channel declares
//!   `Dialect::OpenAi`, so the host converts anything else into Responses
//!   first and this is the only shape the translation has to read.
//! * **Response.** The reply is AWS `vnd.amazon.eventstream` framing carrying
//!   CodeWhisperer events, not SSE. `stream_generate_content` translates the
//!   frames into Responses SSE and `generate_content` returns the same
//!   answer buffered, so the client gets the dialect the channel declared
//!   (`stream/`). `ListModels` is rewritten into an OpenAI list (`models`).
//! * **Credential.** A Bearer token from one of two issuers, with the profile
//!   ARN the management plane needs beside it (`oauth`). Both flows are
//!   refreshable; which endpoint renews a credential is decided by what the
//!   credential itself says, never discovered in `prepare`.
//! * **Quota.** `GetUsageLimits` reports fractional credit windows per
//!   resource type (`quota`).
//!
//! Not ported from v3: `CountTokens`, which v3 answered locally and v4's host
//! owns; the `endpoints` map, which v4 expresses as the host's
//! per-operation `endpoint_override`; and the `ChannelTrafficPolicy`, which
//! has no v4 counterpart — v4 forwards every client header that is not
//! dropped, and a provider that wants v3's "forward nothing" writes
//! `allowed_headers: []`.
//!
//! Session identity: Kiro has no session field in `design/session-identity.md`
//! and the app sends none. The envelope's `conversationId` is minted per
//! request, as v3 minted it, because the whole history is replayed on every
//! turn.

mod config;
mod models;
mod oauth;
mod quota;
mod request;
mod stream;
mod usage;

pub use config::{
    DEFAULT_AUTH_BASE_URL, DEFAULT_REDIRECT_URI, DEFAULT_REGION, DEFAULT_START_URL, ID, KiroConfig,
    OAUTH_SCOPE,
};
pub use quota::{AGENTIC_REQUEST_DIMENSION, TARGET_USAGE_LIMITS};

use crate::OutboundClient;
use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ChannelHeaders, ConfigKey,
    ConfigKeyKind, CredentialRefresh, CredentialView, HOST_CONFIG_KEYS, HeaderAllowlist, LoginMode,
    OAuthAuthorizationCode, OAuthDeviceCode, OperationContext, OperationFuture, PrepareContext,
    ProviderView, QuotaQuery, UsageExtractor, UsageStream, forwardable,
};
use futures_util::StreamExt as _;
use gproxy_client::{Alpn, Backend, ConnectionConfig, EmulationConfig, Fingerprint, TlsVersion};
use gproxy_protocol::connection::{ByteStream, Bytes, TransportError};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse};
use http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// The Smithy content type both planes speak.
const AMZ_JSON: &str = "application/x-amz-json-1.0";
/// The runtime plane's SDK banner (v3 `prepare.rs`).
const UA_RUNTIME: &str = "aws-sdk-rust/1.3.15 ua/2.1 api/codewhispererstreaming/0.1.16551 os/linux lang/rust/1.92.0 md/appVersion-2.6.1 app/AmazonQ-For-CLI";
/// The management plane's, which names the other service.
const UA_MANAGEMENT: &str = "aws-sdk-rust/1.3.15 ua/2.1 api/codewhispererruntime/0.1.16551 os/linux lang/rust/1.92.0 md/appVersion-2.6.1 app/AmazonQ-For-CLI";
const TARGET_GENERATE: &str = "AmazonCodeWhispererStreamingService.GenerateAssistantResponse";
const TARGET_LIST_MODELS: &str = "AmazonCodeWhispererService.ListAvailableModels";
/// Login, refresh, quota and the catalogue all answer small documents.
const MAX_ABILITY_BODY: usize = 1024 * 1024;

#[derive(Debug, Default, Clone, Copy)]
pub struct Kiro;

/// Which host a call goes to. The two planes have different origins, SDK
/// banners and Smithy services.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Plane {
    Runtime,
    Management,
}

/// One Smithy operation, with the plane and banner that go with it.
#[derive(Clone, Copy)]
pub(super) enum Target {
    Generate,
    ListModels,
    UsageLimits,
}

impl Target {
    fn plane(self) -> Plane {
        match self {
            Self::Generate => Plane::Runtime,
            Self::ListModels | Self::UsageLimits => Plane::Management,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Generate => TARGET_GENERATE,
            Self::ListModels => TARGET_LIST_MODELS,
            Self::UsageLimits => TARGET_USAGE_LIMITS,
        }
    }

    fn user_agent(self) -> &'static str {
        match self.plane() {
            Plane::Runtime => UA_RUNTIME,
            Plane::Management => UA_MANAGEMENT,
        }
    }
}

// ---------------------------------------------------------------- headers

/// What the Kiro CLI itself sends. The channel sets every one of them from
/// the credential and the provider configuration, so a provider allow-list
/// can never hide the identity being impersonated.
pub const CLI_HEADERS: ChannelHeaders = ChannelHeaders {
    names: &[
        "user-agent",
        "x-amz-user-agent",
        "x-amz-target",
        "x-amzn-codewhisperer-optout",
        "amz-sdk-request",
        "amz-sdk-invocation-id",
    ],
    prefixes: &[],
};

/// Headers the channel owns; a client cannot supply them. AWS console
/// cookies are browser identity and never belong on a bearer-token call.
const CHANNEL_HEADERS: &[&str] = &[
    "user-agent",
    "x-amz-user-agent",
    "x-amz-target",
    "x-amzn-codewhisperer-optout",
    "amz-sdk-request",
    "amz-sdk-invocation-id",
    "accept",
    "cookie",
];

/// The Kiro app's outbound stack, the default for a provider that names no
/// connection profile: the Rust AWS SDK over rustls, TLS 1.2 floor, TLS 1.3
/// suites first, no GREASE and no ALPN offer of its own (v3
/// `kiro/profile.rs`).
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
                    "ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-ECDSA-AES128-GCM-SHA256:",
                    "ECDHE-ECDSA-CHACHA20-POLY1305:ECDHE-RSA-AES256-GCM-SHA384:",
                    "ECDHE-RSA-AES128-GCM-SHA256:ECDHE-RSA-CHACHA20-POLY1305"
                )
                .into(),
            ),
            curves_list: Some("X25519:P-256:P-384".into()),
            sigalgs_list: Some(
                concat!(
                    "ecdsa_secp384r1_sha384:ecdsa_secp256r1_sha256:ed25519:",
                    "rsa_pss_rsae_sha512:rsa_pss_rsae_sha384:rsa_pss_rsae_sha256:",
                    "rsa_pkcs1_sha512:rsa_pkcs1_sha384:rsa_pkcs1_sha256"
                )
                .into(),
            ),
            preserve_tls13_cipher_list: Some(false),
            grease: Some(false),
            ocsp_stapling: None,
            signed_cert_timestamps: None,
            http2: None,
            headers: None,
        })),
        ..ConnectionConfig::default()
    }
}

// ---------------------------------------------------------------- helpers

pub(super) fn unix_now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn unix_now_nanos() -> u128 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
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

/// A UUID-shaped id from the request being sent, its purpose and the clock —
/// the AWS SDK mints a fresh one per call and so does this (v3
/// `prepare.rs::request_id`).
fn request_id(seed: &[u8], label: &str) -> String {
    let digest = Sha256::digest([seed, label.as_bytes(), &unix_now_nanos().to_be_bytes()].concat());
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

/// Percent-encode a query or form component; a space becomes `%20`, so a
/// space-separated scope survives being either.
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

pub(super) fn json_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
    // The desktop app's auth calls do not carry the SDK banner.
    headers.insert(header::USER_AGENT, HeaderValue::from_static("Kiro-CLI"));
    headers
}

pub(super) async fn read_body(body: HttpBody) -> Result<Bytes, ChannelError> {
    match body {
        HttpBody::Bytes(bytes) => Ok(bytes),
        HttpBody::Stream(mut stream) => {
            let mut out = Vec::new();
            while let Some(chunk) = stream.next().await {
                let chunk =
                    chunk.map_err(|error| ChannelError::InvalidResponse(error.to_string()))?;
                out.extend_from_slice(&chunk);
                if out.len() > MAX_ABILITY_BODY {
                    return Err(ChannelError::InvalidResponse(
                        "the reply exceeds the read limit".into(),
                    ));
                }
            }
            Ok(Bytes::from(out))
        }
    }
}

/// One bounded exchange outside the operation path: login, refresh, quota and
/// the catalogue. Its failures are errors rather than responses.
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
        .map_err(|error| ChannelError::InvalidConfig(error.to_string()))?;
    let WireResponse {
        status,
        headers,
        body,
    } = client.send(request).await?;
    Ok((status, headers, read_body(body).await?))
}

fn non_empty(value: Option<&Value>) -> Option<&str> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// A public account fact: host metadata first, then the secret's
/// `provider_fields`, then the flat layout v3 wrote.
pub(super) fn fact<'a>(credential: &CredentialView<'a>, name: &str) -> Option<&'a str> {
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
    non_empty(credential.secret.get("access_token")).ok_or(ChannelError::InvalidCredential)
}

/// The profile the management plane bills to: the credential's first, then
/// the provider's.
pub(super) fn profile_arn(config: &KiroConfig, credential: &CredentialView<'_>) -> Option<String> {
    fact(credential, "profile_arn")
        .map(str::to_owned)
        .or_else(|| {
            config
                .profile_arn
                .as_deref()
                .map(str::trim)
                .filter(|arn| !arn.is_empty())
                .map(str::to_owned)
        })
}

/// The origin of one plane, without a trailing slash.
pub(super) fn plane_base(
    provider: ProviderView<'_>,
    config: &KiroConfig,
    plane: Plane,
) -> Result<String, ChannelError> {
    let configured = match plane {
        Plane::Management => config
            .management_base_url
            .as_deref()
            .map(str::trim)
            .filter(|base| !base.is_empty())
            .or_else(|| provider.base_url.map(str::trim).filter(|b| !b.is_empty())),
        Plane::Runtime => provider.base_url.map(str::trim).filter(|b| !b.is_empty()),
    };
    if let Some(base) = configured {
        return Ok(base.trim_end_matches('/').to_owned());
    }
    let region = config.region()?;
    Ok(match plane {
        Plane::Runtime => format!("https://runtime.{region}.kiro.dev"),
        Plane::Management => format!("https://management.{region}.kiro.dev"),
    })
}

/// The AWS SDK header set one Smithy call carries.
pub(super) fn smithy_headers(
    mut headers: HeaderMap,
    config: &KiroConfig,
    token: &str,
    target: Target,
    body: &[u8],
) -> Result<HeaderMap, ChannelError> {
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|_| ChannelError::InvalidCredential)?,
    );
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(AMZ_JSON));
    headers.insert(header::ACCEPT, HeaderValue::from_static("*/*"));
    let user_agent = config
        .user_agent
        .as_deref()
        .map(str::trim)
        .filter(|agent| !agent.is_empty());
    let banner = user_agent.unwrap_or(target.user_agent());
    headers.insert(
        header::USER_AGENT,
        HeaderValue::from_str(banner).map_err(|_| invalid_config("user_agent"))?,
    );
    headers.insert(
        HeaderName::from_static("x-amz-user-agent"),
        HeaderValue::from_str(banner).map_err(|_| invalid_config("user_agent"))?,
    );
    headers.insert(
        HeaderName::from_static("x-amz-target"),
        HeaderValue::from_static(target.name()),
    );
    headers.insert(
        HeaderName::from_static("x-amzn-codewhisperer-optout"),
        HeaderValue::from_static("false"),
    );
    headers.insert(
        HeaderName::from_static("amz-sdk-request"),
        HeaderValue::from_static("attempt=1; max=3"),
    );
    headers.insert(
        HeaderName::from_static("amz-sdk-invocation-id"),
        HeaderValue::from_str(&request_id(body, "invocation"))
            .map_err(|_| invalid_config("invocation id"))?,
    );
    for (name, value) in &config.headers {
        headers.insert(
            HeaderName::from_bytes(name.as_bytes()).map_err(|_| invalid_config(name))?,
            HeaderValue::from_str(value).map_err(|_| invalid_config(name))?,
        );
    }
    Ok(headers)
}

fn invalid_config(what: &str) -> ChannelError {
    ChannelError::InvalidConfig(format!("Kiro: `{what}` is not a header"))
}

fn unsupported(operation: OperationKey) -> ChannelError {
    ChannelError::UnsupportedOperation(operation)
}

// ---------------------------------------------------------------- prepare

/// A prepared generation call, with what the translator needs to name the
/// Responses items it will synthesize.
struct Generation {
    request: http::Request<HttpBody>,
    response_id: String,
    model: String,
}

impl Kiro {
    fn generation(&self, ctx: PrepareContext<'_>) -> Result<Generation, ChannelError> {
        let config = KiroConfig::from_view(ctx.provider)?;
        let token = access_token(&ctx.credential)?;
        let allowlist = HeaderAllowlist::from_view_for(ctx.provider, CLI_HEADERS)?;
        let WireRequest {
            headers: source,
            body,
            ..
        } = ctx.request;
        let HttpBody::Bytes(buffered) = body else {
            return Err(ChannelError::InvalidConfig(
                "the Kiro conversation envelope needs a buffered request body".into(),
            ));
        };
        let response_id = request_id(&buffered, "conversation");
        let envelope = request::build(&buffered, &response_id)?;
        let mut value: Value = serde_json::from_slice(&envelope)
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))?;
        if value.get("profileArn").is_none()
            && let Some(profile) = profile_arn(&config, &ctx.credential)
        {
            value["profileArn"] = Value::String(profile);
        }
        let model = value
            .pointer("/conversationState/currentMessage/userInputMessage/modelId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        let body = value.to_string().into_bytes();
        let url = match ctx.endpoint_override {
            Some(url) => url.to_owned(),
            None => format!("{}/", plane_base(ctx.provider, &config, Plane::Runtime)?),
        };
        let headers = smithy_headers(
            forwardable(&source, allowlist.as_ref(), CHANNEL_HEADERS),
            &config,
            token,
            Target::Generate,
            &body,
        )?;
        let mut builder = http::Request::builder().method(Method::POST).uri(url);
        if let Some(map) = builder.headers_mut() {
            *map = headers;
        }
        let request = builder
            .body(HttpBody::Bytes(Bytes::from(body)))
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))?;
        Ok(Generation {
            request,
            response_id,
            model,
        })
    }

    fn catalogue(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        let config = KiroConfig::from_view(ctx.provider)?;
        let token = access_token(&ctx.credential)?;
        let allowlist = HeaderAllowlist::from_view_for(ctx.provider, CLI_HEADERS)?;
        // The catalogue is per profile; without one there is nothing to ask.
        let profile =
            profile_arn(&config, &ctx.credential).ok_or(ChannelError::InvalidCredential)?;
        let query = format!("origin=KIRO_CLI&profileArn={}", encode_component(&profile));
        let url = match ctx.endpoint_override {
            Some(url) if url.contains('?') => format!("{url}&{query}"),
            Some(url) => format!("{url}?{query}"),
            None => format!(
                "{}/?{query}",
                plane_base(ctx.provider, &config, Plane::Management)?
            ),
        };
        let body = serde_json::json!({"origin": "KIRO_CLI", "profileArn": profile})
            .to_string()
            .into_bytes();
        let headers = smithy_headers(
            forwardable(&ctx.request.headers, allowlist.as_ref(), CHANNEL_HEADERS),
            &config,
            token,
            Target::ListModels,
            &body,
        )?;
        let mut builder = http::Request::builder().method(Method::POST).uri(url);
        if let Some(map) = builder.headers_mut() {
            *map = headers;
        }
        builder
            .body(HttpBody::Bytes(Bytes::from(body)))
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }

    /// Send a generation call and hand back the translator primed with the
    /// ids the prepared envelope carries.
    async fn generate(
        &self,
        operation: Operation,
        ctx: OperationContext<'_>,
    ) -> Result<(WireResponse<HttpBody>, stream::Translator), ChannelError> {
        let OperationContext {
            provider,
            credential,
            dialect,
            request,
            client,
            endpoint_override,
            ..
        } = ctx;
        let prepared = self.generation(PrepareContext {
            provider,
            credential,
            operation: OperationKey { operation, dialect },
            request,
            endpoint_override,
        })?;
        let translator = stream::Translator::new(&prepared.response_id, &prepared.model);
        let response = client.send(prepared.request).await?;
        Ok((response, translator))
    }
}

impl BaseChannel for Kiro {
    fn id(&self) -> &'static str {
        ID
    }

    /// A CodeWhisperer subscription through the Kiro app: two ways in, a
    /// refreshable token, a per-profile catalogue and credit windows.
    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "Kiro (AWS CodeWhisperer)",
            login_modes: vec![LoginMode::DeviceCode, LoginMode::AuthorizationCode],
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
                    "Runtime origin, replacing https://runtime.{region}.kiro.dev; also serves the management plane unless management_base_url names one. Provider column, not config JSON.",
                ),
                ConfigKey::optional(
                    "region",
                    ConfigKeyKind::String,
                    "Region for both Kiro planes and for the AWS OIDC host an Identity Center credential refreshes against; defaults to us-east-1.",
                ),
                ConfigKey::optional(
                    "management_base_url",
                    ConfigKeyKind::String,
                    "Origin replacing https://management.{region}.kiro.dev, which serves the model catalogue and the usage limits.",
                ),
                ConfigKey::optional(
                    "auth_base_url",
                    ConfigKeyKind::String,
                    "The Kiro desktop auth host the device login and its refresh talk to.",
                ),
                ConfigKey::optional(
                    "profile_arn",
                    ConfigKeyKind::String,
                    "CodeWhisperer profile ARN for credentials whose login did not report one; a credential's own wins.",
                ),
                ConfigKey::optional(
                    "login_provider",
                    ConfigKeyKind::String,
                    "Identity provider the device login opens: github or google.",
                ),
                ConfigKey::optional(
                    "sso_start_url",
                    ConfigKeyKind::String,
                    "The Identity Center portal the authorization code is issued by; defaults to the Builder ID portal.",
                ),
                ConfigKey::optional(
                    "user_agent",
                    ConfigKeyKind::String,
                    "Replaces the AWS SDK banner on every request.",
                ),
                ConfigKey::optional(
                    "headers",
                    ConfigKeyKind::HeaderList,
                    "Static headers added to every request.",
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

    /// The envelope is built from a Responses body and the reply is handed
    /// back as Responses, so that is the only shape the channel reads.
    fn native_dialects(&self, _provider: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::GenerateContent
            | Operation::StreamGenerateContent
            | Operation::ListModels => {
                vec![Dialect::OpenAi]
            }
            _ => Vec::new(),
        }
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        match (ctx.operation.operation, ctx.operation.dialect) {
            (Operation::GenerateContent | Operation::StreamGenerateContent, Dialect::OpenAi) => {
                Ok(self.generation(ctx)?.request)
            }
            (Operation::ListModels, Dialect::OpenAi) => self.catalogue(ctx),
            _ => Err(unsupported(ctx.operation)),
        }
    }

    /// The catalogue is not an OpenAI list until the channel makes it one.
    fn list_models<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(async move {
            let request = self.catalogue(PrepareContext {
                provider: context.provider,
                credential: context.credential,
                operation: OperationKey {
                    operation: Operation::ListModels,
                    dialect: context.dialect,
                },
                request: context.request,
                endpoint_override: context.endpoint_override,
            })?;
            let response = context.client.send(request).await?;
            if !response.status.is_success() {
                return Ok(response);
            }
            let body = read_body(response.body).await?;
            let mut headers = response.headers;
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            headers.remove(header::CONTENT_LENGTH);
            Ok(WireResponse {
                status: StatusCode::OK,
                headers,
                body: HttpBody::Bytes(models::rewrite(&body)?),
            })
        })
    }

    /// The upstream only streams, so a buffered caller gets the translated
    /// answer collected into the one Responses object it asked for.
    fn generate_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(async move {
            let (response, mut translator) =
                self.generate(Operation::GenerateContent, context).await?;
            if !response.status.is_success() {
                return Ok(response);
            }
            let body = collect(response.body).await?;
            translator.push(&body)?;
            translator.finish()?;
            let completed = translator
                .completed()
                .cloned()
                .ok_or_else(|| ChannelError::InvalidResponse("Kiro produced no answer".into()))?;
            let mut headers = response.headers;
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            headers.remove(header::CONTENT_LENGTH);
            Ok(WireResponse {
                status: response.status,
                headers,
                body: HttpBody::Bytes(Bytes::from(completed.to_string())),
            })
        })
    }

    fn stream_generate_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(async move {
            let (response, translator) = self
                .generate(Operation::StreamGenerateContent, context)
                .await?;
            if !response.status.is_success() {
                return Ok(response);
            }
            let mut headers = response.headers;
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/event-stream"),
            );
            headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
            headers.remove(header::CONTENT_LENGTH);
            Ok(WireResponse {
                status: response.status,
                headers,
                body: HttpBody::Stream(translate(response.body, translator)),
            })
        })
    }

    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        Some(self)
    }
    fn oauth_device_code(&self) -> Option<&dyn OAuthDeviceCode> {
        Some(self)
    }
    fn oauth_authorization_code(&self) -> Option<&dyn OAuthAuthorizationCode> {
        Some(self)
    }
    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        Some(self)
    }
    fn usage_extractor(&self) -> Option<&dyn UsageExtractor> {
        Some(self)
    }
    fn usage_stream(&self) -> Option<&dyn UsageStream> {
        Some(self)
    }
}

/// A generation reply is not an ability call: it has no small-document bound
/// of its own, only whatever the host already enforces.
async fn collect(body: HttpBody) -> Result<Bytes, ChannelError> {
    match body {
        HttpBody::Bytes(bytes) => Ok(bytes),
        HttpBody::Stream(mut stream) => {
            let mut out = Vec::new();
            while let Some(chunk) = stream.next().await {
                out.extend_from_slice(
                    &chunk.map_err(|error| ChannelError::InvalidResponse(error.to_string()))?,
                );
            }
            Ok(Bytes::from(out))
        }
    }
}

enum Translating {
    Reading(ByteStream, Box<stream::Translator>),
    Done,
}

/// Wrap an event-stream body in the Responses SSE translation. A decoding
/// failure ends the stream as a transport error, so the host records an
/// interrupted response rather than a complete one.
fn translate(body: HttpBody, translator: stream::Translator) -> ByteStream {
    let upstream: ByteStream = match body {
        HttpBody::Stream(stream) => stream,
        HttpBody::Bytes(bytes) => Box::pin(futures_util::stream::once(async move { Ok(bytes) })),
    };
    Box::pin(futures_util::stream::unfold(
        Translating::Reading(upstream, Box::new(translator)),
        |state| async move {
            let Translating::Reading(mut upstream, mut translator) = state else {
                return None;
            };
            loop {
                match upstream.next().await {
                    Some(Ok(chunk)) => {
                        let out = match translator.push(&chunk) {
                            Ok(out) => out,
                            Err(error) => {
                                return Some((Err(transport_error(error)), Translating::Done));
                            }
                        };
                        if translator.failed() {
                            return Some((Ok(out), Translating::Done));
                        }
                        if !out.is_empty() {
                            return Some((Ok(out), Translating::Reading(upstream, translator)));
                        }
                    }
                    Some(Err(error)) => return Some((Err(error), Translating::Done)),
                    None => {
                        return match translator.finish() {
                            Ok(tail) => Some((Ok(tail), Translating::Done)),
                            Err(error) => Some((Err(transport_error(error)), Translating::Done)),
                        };
                    }
                }
            }
        },
    ))
}

fn transport_error(error: ChannelError) -> TransportError {
    error.to_string().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_region_only_reaches_a_host_name_when_it_looks_like_one() {
        assert!(config::validate_region("eu-central-1").is_ok());
        assert!(config::validate_region("eu_central_1").is_err());
    }

    #[test]
    fn an_invocation_id_is_a_v4_uuid_and_changes_per_call() {
        let id = request_id(b"{}", "invocation");
        assert_eq!(id.len(), 36);
        assert_eq!(id.as_bytes()[14], b'4');
        assert!(matches!(id.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
    }
}
