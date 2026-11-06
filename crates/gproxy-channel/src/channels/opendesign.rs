//! OpenDesign (`open-design.ai`): the OpenDesign Go / Cloud model
//! subscription, reached with the API key the account dashboard issues.
//!
//! Wire facts come from two sources under `samples/`:
//!
//! - `samples/open-design` (`nexu-io/open-design`): the daemon names the
//!   model gateway `VELA_LINK_URL`, `https://amr-link.open-design.ai/v1`, and
//!   authenticates it with `authorization: Bearer <runtime key>`
//!   (`apps/daemon/src/integrations/vela-billing.ts`).
//! - `samples/vela-cli-0.1.9` (`@powerformer/vela-cli-linux-x64@0.1.9`, the
//!   Go CLI the app ships): its default link origin is
//!   `https://amr-link.open-design.ai`, the catalogue is `/v1/models`, and it
//!   drives its bundled `opencode` through `@ai-sdk/openai-compatible`.
//!
//! Probed without a valid key on 2026-11-07: `/v1/chat/completions`,
//! `/v1/responses` and `/v1/models` answer OpenAI-shaped
//! `invalid_api_key` errors, `/v1/messages` answers an Anthropic-shaped
//! `authentication_error` and takes the key as either `x-api-key` or a
//! bearer, and an unknown `/v1/*` path is a plain 404. So there are three
//! native surfaces on one origin, the same layout as `opencodego`. The
//! pricing copy ("API access for Codex, Claude Code, DSH, OpenCode") agrees.
//!
//! **No quota.** The wallet and plan live on `amr-api.open-design.ai`
//! (`/api/v1/billing/summary`, `/api/v1/me`) behind the control key that
//! `vela login`'s device authorization stores; whether a dashboard API key
//! is accepted there is unverified, so nothing is read.

use crate::channel::{
    BaseChannel, ChannelDescriptor, ChannelError, ConfigKey, ConfigKeyKind, HOST_CONFIG_KEYS,
    HeaderAllowlist, LoginMode, PrepareContext, ProviderView, forwardable, insert_configured,
    strip_query_auth,
};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey};
use http::{HeaderName, HeaderValue, header};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

pub const ID: &str = "opendesign";
pub const DEFAULT_BASE_URL: &str = "https://amr-link.open-design.ai";

/// Provider `config` JSON understood by this channel. Unknown keys ignored.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct OpenDesignConfig {
    /// Static headers added to every upstream request.
    pub headers: BTreeMap<String, String>,
}

impl OpenDesignConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }
}

fn base_url(provider: ProviderView<'_>) -> &str {
    provider
        .base_url
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(DEFAULT_BASE_URL)
        .trim_end_matches('/')
}

fn api_key(secret: &Value) -> Result<&str, ChannelError> {
    secret
        .get("api_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or(ChannelError::InvalidCredential)
}

/// Each dialect's own endpoint. The client's path is not trusted: the
/// gateway mounts everything at a fixed place under `/v1`.
fn path(operation: OperationKey) -> Result<&'static str, ChannelError> {
    match (operation.operation, operation.dialect) {
        (Operation::ListModels, Dialect::OpenAiChat | Dialect::OpenAi) => Ok("/v1/models"),
        (Operation::GenerateContent | Operation::StreamGenerateContent, Dialect::OpenAiChat) => {
            Ok("/v1/chat/completions")
        }
        (Operation::GenerateContent | Operation::StreamGenerateContent, Dialect::OpenAi) => {
            Ok("/v1/responses")
        }
        (Operation::GenerateContent | Operation::StreamGenerateContent, Dialect::Claude) => {
            Ok("/v1/messages")
        }
        _ => Err(ChannelError::UnsupportedOperation(operation)),
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct OpenDesign;

impl BaseChannel for OpenDesign {
    fn id(&self) -> &'static str {
        ID
    }

    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "OpenDesign",
            credential_kinds: vec!["api_key"],
            login_modes: vec![LoginMode::ApiKey],
            capabilities: Default::default(),
            config_keys: [
                ConfigKey::optional(
                    "base_url",
                    ConfigKeyKind::String,
                    "Upstream origin, without `/v1`. Provider column, not config JSON.",
                )
                .with_placeholder(DEFAULT_BASE_URL),
                ConfigKey::optional(
                    "headers",
                    ConfigKeyKind::HeaderList,
                    "Static headers added to every upstream request.",
                ),
            ]
            .into_iter()
            .chain(HOST_CONFIG_KEYS)
            .collect(),
        }
    }

    /// Three surfaces on one origin, each answering its own wire verbatim.
    fn native_dialects(&self, _: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::GenerateContent | Operation::StreamGenerateContent => {
                vec![Dialect::OpenAiChat, Dialect::OpenAi, Dialect::Claude]
            }
            Operation::ListModels => vec![Dialect::OpenAiChat, Dialect::OpenAi],
            _ => Vec::new(),
        }
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        let config = OpenDesignConfig::from_view(ctx.provider)?;
        let key = api_key(ctx.credential.secret)?;
        let url = match ctx.endpoint_override {
            Some(url) => url.to_owned(),
            None => format!("{}{}", base_url(ctx.provider), path(ctx.operation)?),
        };
        let uri = match ctx
            .request
            .query
            .as_deref()
            .map(strip_query_auth)
            .filter(|query| !query.is_empty())
        {
            Some(query) => format!("{url}{}{query}", if url.contains('?') { "&" } else { "?" }),
            None => url,
        };
        let allowlist = HeaderAllowlist::from_view_for(
            ctx.provider,
            crate::channel::ChannelHeaders::native(ctx.operation.dialect),
        )?;
        let mut headers = forwardable(&ctx.request.headers, allowlist.as_ref(), &[]);
        if ctx.operation.dialect == Dialect::Claude {
            // The Claude surface takes the key the way Anthropic does.
            headers.insert(
                HeaderName::from_static("x-api-key"),
                HeaderValue::from_str(key).map_err(|_| ChannelError::InvalidCredential)?,
            );
            headers
                .entry(HeaderName::from_static("anthropic-version"))
                .or_insert_with(|| HeaderValue::from_static(gproxy_protocol::claude::API_VERSION));
        } else {
            headers.insert(
                header::AUTHORIZATION,
                HeaderValue::from_str(&format!("Bearer {key}"))
                    .map_err(|_| ChannelError::InvalidCredential)?,
            );
        }
        for (name, value) in &config.headers {
            insert_configured(&mut headers, name, value)?;
        }
        let body = match ctx.request.body {
            HttpBody::Bytes(bytes) if !bytes.is_empty() => {
                headers
                    .entry(header::CONTENT_TYPE)
                    .or_insert_with(|| HeaderValue::from_static("application/json"));
                HttpBody::Bytes(bytes)
            }
            body => body,
        };
        let mut builder = http::Request::builder().method(ctx.request.method).uri(uri);
        if let Some(target) = builder.headers_mut() {
            *target = headers;
        }
        builder
            .body(body)
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }
}
