//! WorkBuddy: Tencent Copilot used the way its editor plugin and CLI use it.
//!
//! Wire facts follow the v3 channel (`crates/gproxy-channels/src/workbuddy` on
//! `main`), which was written against a static investigation of plugin
//! 4.22.16. Everything sits under one origin, `copilot.tencent.com`:
//!
//! * **Generation** is OpenAI Chat Completions at `POST /v2/chat/completions`,
//!   so the channel declares `Dialect::OpenAiChat` and the host converts
//!   anything else. Nothing about the body is rewritten beyond asking a
//!   stream to end with its usage chunk.
//! * **The catalogue** is the plugin's whole configuration document at
//!   `GET /v3/config`; only its `models` are the catalogue, so `ListModels` is
//!   overridden and answers an OpenAI list (`models`).
//! * **Images** are `POST /v2/images/{generations,edits}` with Hunyuan's own
//!   field set (`images`).
//! * **Every reply** arrives inside the gateway envelope `{code, msg, data}`,
//!   which the channel unwraps for the surfaces it declares (`envelope`).
//! * **Identity.** The plugin announces itself with `x-product`, `x-ide-*`,
//!   `x-agent-intent` and a `WorkBuddy/<version>` user agent, and announces
//!   the account with `x-user-id` plus, for an enterprise seat,
//!   `x-enterprise-id`/`x-tenant-id`, `x-department-info` and `x-domain`
//!   (`auth`). Those account facts are what the login learned, read back out
//!   of the credential's metadata.
//! * **Login** is a device flow whose device code is the auth `state`
//!   (`oauth`), and **quota** is one of two billing meters (`quota`).
//!
//! Session identity: `design/session-identity.md` reads WorkBuddy's session
//! from `x-conversation-id`, and names `x-request-id`,
//! `x-conversation-request-id` and `x-conversation-message-id` as explicitly
//! *not* session ids. So the three request ids are minted fresh per call and a
//! client's `x-conversation-id` passes through under its own name; only a
//! client that sent none gets a minted one.
//!
//! Not ported from v3: `GetModel` and `CountTokens`, which v3 answered locally
//! and v4's host owns; the `endpoints` map, which v4 expresses as the host's
//! per-operation `endpoint_override`; the multipart image rebuild, for the
//! reason `images` gives; and the `ChannelTrafficPolicy`, which has no v4
//! counterpart — v4 forwards every client header that is not dropped, and a
//! provider that wants v3's "forward nothing" writes `allowed_headers: []`.

mod auth;
mod config;
mod envelope;
mod images;
mod models;
mod oauth;
mod quota;
mod usage;

pub use config::{CLI_VERSION, DEFAULT_BASE_URL, ID, WorkBuddyConfig};
pub use quota::ENTERPRISE_DIMENSION;

use crate::OutboundClient;
use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ChannelHeaders, ConfigKey,
    ConfigKeyKind, CredentialRefresh, HOST_CONFIG_KEYS, HeaderAllowlist, LoginMode,
    OAuthDeviceCode, OperationContext, OperationFuture, PrepareContext, ProviderView, QuotaQuery,
    UsageExtractor, UsageStream, forwardable,
};
use crate::channels::shared::openai_wire;
use config::base_url;
use futures_util::StreamExt as _;
use gproxy_protocol::connection::Bytes;
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireRequest, WireResponse};
use http::{HeaderMap, HeaderValue, Method, StatusCode, header};

/// Login, refresh, quota and the catalogue all answer small documents.
const MAX_ABILITY_BODY: usize = 1024 * 1024;

#[derive(Debug, Default, Clone, Copy)]
pub struct WorkBuddy;

/// What the WorkBuddy plugin itself sends. The channel sets all of them, so a
/// provider allow-list can never hide the identity being impersonated; the
/// conversation id is additionally *read* here, because it is the session id
/// the gateway's own ladder uses.
pub const CLI_HEADERS: ChannelHeaders = ChannelHeaders {
    names: &[
        "x-conversation-id",
        "x-request-id",
        "x-conversation-message-id",
        "x-conversation-request-id",
        "x-agent-intent",
        "x-product",
        "x-ide-type",
        "x-ide-name",
        "x-ide-version",
        "user-agent",
    ],
    prefixes: &[],
};

/// Headers the channel owns; a client cannot supply them. The account block
/// is the credential's, the identity block is the plugin's, and the refresh
/// token never belongs on a data-plane call.
const CHANNEL_HEADERS: &[&str] = &[
    "x-user-id",
    "x-enterprise-id",
    "x-tenant-id",
    "x-department-info",
    "x-domain",
    "x-request-id",
    "x-conversation-message-id",
    "x-conversation-request-id",
    "x-conversation-id",
    "x-agent-intent",
    "x-product",
    "x-ide-type",
    "x-ide-name",
    "x-ide-version",
    "x-refresh-token",
    "x-auth-refresh-source",
    "user-agent",
    "accept",
    "cookie",
];

// ---------------------------------------------------------------- helpers

pub(super) fn unix_now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Percent-encode a query component.
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

pub(super) fn json_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    headers.insert(header::ACCEPT, HeaderValue::from_static("application/json"));
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

/// One bounded exchange outside the operation path: login, refresh and quota.
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

/// Where an operation lives under the origin (v3 `model.rs::path`).
fn operation_path(operation: Operation) -> Result<&'static str, ChannelError> {
    Ok(match operation {
        Operation::ListModels => "/v3/config",
        Operation::CreateImage => "/v2/images/generations",
        Operation::EditImage => "/v2/images/edits",
        Operation::GenerateContent | Operation::StreamGenerateContent => "/v2/chat/completions",
        other => {
            return Err(ChannelError::UnsupportedOperation(OperationKey {
                operation: other,
                dialect: Dialect::OpenAiChat,
            }));
        }
    })
}

// ---------------------------------------------------------------- prepare

impl WorkBuddy {
    fn build(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        let config = WorkBuddyConfig::from_view(ctx.provider)?;
        let operation = ctx.operation.operation;
        let allowlist = HeaderAllowlist::from_view_for(ctx.provider, CLI_HEADERS)?;
        let WireRequest {
            method,
            headers: source,
            body,
            ..
        } = ctx.request;
        let url = match ctx.endpoint_override {
            Some(url) => url.to_owned(),
            None => format!("{}{}", base_url(ctx.provider), operation_path(operation)?),
        };
        // The client's own session id, read before the forwarding filter
        // drops it, so the channel can put it back under its own name.
        let session = auth::client_session(&source, allowlist.as_ref()).map(str::to_owned);
        let mut headers = forwardable(&source, allowlist.as_ref(), CHANNEL_HEADERS);
        let body = match (operation, body) {
            (Operation::CreateImage | Operation::EditImage, HttpBody::Bytes(bytes)) => {
                HttpBody::Bytes(images::request(&bytes, operation == Operation::EditImage)?)
            }
            (Operation::StreamGenerateContent, HttpBody::Bytes(bytes))
                if ctx.operation.dialect == Dialect::OpenAiChat =>
            {
                HttpBody::Bytes(openai_wire::stream_usage_opt_in(bytes))
            }
            (_, other) => other,
        };
        if !matches!(&body, HttpBody::Bytes(bytes) if bytes.is_empty()) {
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
        }
        auth::authorize(&mut headers, &ctx.credential)?;
        auth::identity(&mut headers, &config, session.as_deref())?;
        let mut builder = http::Request::builder().method(method).uri(url);
        if let Some(map) = builder.headers_mut() {
            *map = headers;
        }
        builder
            .body(body)
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }

    /// Send, then lift the gateway envelope off a successful reply. A failure
    /// is returned exactly as it arrived.
    async fn unwrapped(
        &self,
        operation: Operation,
        context: OperationContext<'_>,
    ) -> Result<WireResponse<HttpBody>, ChannelError> {
        let OperationContext {
            provider,
            credential,
            dialect,
            request,
            client,
            endpoint_override,
            ..
        } = context;
        let prepared = self.build(PrepareContext {
            provider,
            credential,
            operation: OperationKey { operation, dialect },
            request,
            endpoint_override,
        })?;
        let response = client.send(prepared).await?;
        if !response.status.is_success() {
            return Ok(response);
        }
        let body = read_body(response.body).await?;
        let body = match operation {
            Operation::ListModels => models::rewrite(&body)?,
            _ => images::response(body),
        };
        let mut headers = response.headers;
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
        headers.remove(header::CONTENT_LENGTH);
        Ok(WireResponse {
            status: response.status,
            headers,
            body: HttpBody::Bytes(body),
        })
    }
}

impl BaseChannel for WorkBuddy {
    fn id(&self) -> &'static str {
        ID
    }

    /// Tencent Copilot through its plugin: a browser device login, a
    /// refreshable token, a configuration document that doubles as the
    /// catalogue, and two billing meters.
    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "WorkBuddy (Tencent Copilot)",
            login_modes: vec![LoginMode::DeviceCode],
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
                    "Upstream origin; defaults to https://copilot.tencent.com. Provider column, not config JSON.",
                ),
                ConfigKey::optional(
                    "ide_version",
                    ConfigKeyKind::String,
                    "Plugin version reported in x-ide-version and in the user agent.",
                ),
                ConfigKey::optional(
                    "ide_name",
                    ConfigKeyKind::String,
                    "What x-ide-name claims the plugin runs inside; the CLI says CLI.",
                ),
                ConfigKey::optional(
                    "ide_type",
                    ConfigKeyKind::String,
                    "What x-ide-type claims the plugin runs inside; the CLI says CLI.",
                ),
                ConfigKey::optional(
                    "agent_intent",
                    ConfigKeyKind::String,
                    "The intent x-agent-intent asks the agent surface for; the CLI says craft.",
                ),
                ConfigKey::optional(
                    "user_agent",
                    ConfigKeyKind::String,
                    "Replaces the WorkBuddy/<version> user agent on every request.",
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

    /// v3 sent no connection profile for WorkBuddy and there is no capture of
    /// the plugin's ClientHello to reproduce, so the operator's own profile is
    /// the only thing that decides the outbound stack.
    fn default_connection(&self) -> Option<gproxy_client::ConnectionConfig> {
        None
    }

    fn native_dialects(&self, _provider: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::GenerateContent | Operation::StreamGenerateContent => {
                vec![Dialect::OpenAiChat]
            }
            Operation::ListModels | Operation::CreateImage | Operation::EditImage => {
                vec![Dialect::OpenAi]
            }
            _ => Vec::new(),
        }
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        self.build(ctx)
    }

    /// The configuration document is not an OpenAI list until the channel
    /// makes it one.
    fn list_models<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(self.unwrapped(Operation::ListModels, context))
    }

    fn create_image<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(self.unwrapped(Operation::CreateImage, context))
    }

    fn edit_image<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(self.unwrapped(Operation::EditImage, context))
    }

    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        Some(self)
    }
    fn oauth_device_code(&self) -> Option<&dyn OAuthDeviceCode> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_declared_operation_knows_where_it_lives() {
        assert_eq!(
            operation_path(Operation::GenerateContent).unwrap(),
            "/v2/chat/completions"
        );
        assert_eq!(operation_path(Operation::ListModels).unwrap(), "/v3/config");
        assert!(operation_path(Operation::CreateSpeech).is_err());
    }

    #[test]
    fn a_state_survives_being_a_query_value() {
        assert_eq!(encode_component("a b/c"), "a%20b%2Fc");
    }
}
