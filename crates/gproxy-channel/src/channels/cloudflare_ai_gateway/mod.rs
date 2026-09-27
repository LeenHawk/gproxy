//! Cloudflare AI Gateway through the Cloudflare REST API, ported from v3's
//! `cloudflare-ai-gateway`.
//!
//! Every request goes to `{base}/client/v4/accounts/{account_id}/ai{path}`,
//! answers in the dialect it was sent in (Chat Completions, Responses or
//! Claude Messages), and names its gateway in `cf-aig-gateway-id`. The token
//! is a Cloudflare API token, sent as a bearer on every dialect — the Claude
//! surface included, which is what `custom` could not say: it would have sent
//! Anthropic's `x-api-key` there.
//!
//! The account and the gateway belong to the **credential**, as in v3:
//! `{"api_key", "account_id", "gateway_id"?}`. One provider can rotate over
//! several Cloudflare accounts or gateways, which a `custom` row with the
//! account in its `base_url` and the gateway in a static header could not.
//!
//! The account's AI Gateway credit balance is the quota (`quota.rs`).

mod quota;

pub use quota::BALANCE_DIMENSION;

use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ConfigKey, ConfigKeyKind,
    HOST_CONFIG_KEYS, HeaderAllowlist, LoginMode, PrepareContext, ProviderView,
    QuotaModel, QuotaQuery, forwardable,
};
use crate::channels::shared::compatible::http::{insert_configured, strip_query_auth};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey};
use http::{HeaderName, HeaderValue, header};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

pub const ID: &str = "cloudflare_ai_gateway";
/// The Cloudflare REST API origin (v3 `cloudflare_ai_gateway/prepare.rs`).
pub const DEFAULT_BASE_URL: &str = "https://api.cloudflare.com";
/// The gateway every Cloudflare account has, used when a credential names none.
pub const DEFAULT_GATEWAY_ID: &str = "default";
const GATEWAY_HEADER: &str = "cf-aig-gateway-id";

/// Provider `config` JSON understood by this channel. Unknown keys ignored.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct CloudflareConfig {
    /// Static headers added to every upstream request, e.g. `cf-aig-*`
    /// caching or logging switches.
    pub headers: BTreeMap<String, String>,
}

impl CloudflareConfig {
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

fn field<'a>(secret: &'a Value, name: &str) -> Option<&'a str> {
    secret
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// An account id goes into a URL path. Cloudflare's are hex; anything outside
/// v3's identifier alphabet is refused rather than escaped into a different
/// path (v3 `quota_cloud/prepare.rs::segment`).
fn account_id(secret: &Value, name: &str) -> Result<String, ChannelError> {
    let account = field(secret, name).ok_or(ChannelError::InvalidCredential)?;
    if matches!(account, "." | "..")
        || !account
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(ChannelError::InvalidCredential);
    }
    Ok(account.to_owned())
}

#[derive(Debug, Default, Clone, Copy)]
pub struct CloudflareAiGateway;

impl BaseChannel for CloudflareAiGateway {
    fn id(&self) -> &'static str {
        ID
    }

    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "Cloudflare AI Gateway",
            login_modes: vec![LoginMode::ApiKey],
            capabilities: ChannelCapabilities {
                quota_query: true,
                ..Default::default()
            },
            config_keys: [
                ConfigKey::optional(
                    "base_url",
                    ConfigKeyKind::String,
                    "Cloudflare REST API origin; the account path is appended. Provider column, not config JSON.",
                )
                .with_placeholder(DEFAULT_BASE_URL),
                ConfigKey::optional(
                    "headers",
                    ConfigKeyKind::HeaderList,
                    "Static headers added to every upstream request, such as cf-aig-* switches.",
                ),
            ]
            .into_iter()
            .chain(HOST_CONFIG_KEYS)
            .collect(),
        }
    }

    /// Three surfaces, each answering its own wire (v3 `routes.rs`).
    fn native_dialects(&self, _: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::GenerateContent | Operation::StreamGenerateContent => {
                vec![Dialect::OpenAiChat, Dialect::OpenAi, Dialect::Claude]
            }
            _ => Vec::new(),
        }
    }

    /// Gemini conversation goes through Chat, as v3 routed it.
    fn default_conversion_target(
        &self,
        _: ProviderView<'_>,
        source: OperationKey,
    ) -> Option<OperationKey> {
        (source.dialect == Dialect::Gemini
            && matches!(
                source.operation,
                Operation::GenerateContent | Operation::StreamGenerateContent
            ))
        .then_some(OperationKey {
            operation: source.operation,
            dialect: Dialect::OpenAiChat,
        })
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        if !self
            .native_dialects(ctx.provider, ctx.operation.operation)
            .contains(&ctx.operation.dialect)
        {
            return Err(ChannelError::UnsupportedOperation(ctx.operation));
        }
        let config = CloudflareConfig::from_view(ctx.provider)?;
        let secret = ctx.credential.secret;
        let key = field(secret, "api_key").ok_or(ChannelError::InvalidCredential)?;
        let gateway = field(secret, "gateway_id").unwrap_or(DEFAULT_GATEWAY_ID);
        let url = match ctx.endpoint_override {
            Some(url) => url.to_owned(),
            None => format!(
                "{}/client/v4/accounts/{}/ai/{}",
                base_url(ctx.provider),
                account_id(secret, "account_id")?,
                ctx.request.path.trim_start_matches('/')
            ),
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
        let allowlist = HeaderAllowlist::from_view(ctx.provider)?;
        let mut headers = forwardable(&ctx.request.headers, allowlist.as_ref(), &[]);
        for (name, value) in &config.headers {
            insert_configured(&mut headers, name, value)?;
        }
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {key}"))
                .map_err(|_| ChannelError::InvalidCredential)?,
        );
        headers.insert(
            HeaderName::from_static(GATEWAY_HEADER),
            HeaderValue::from_str(gateway).map_err(|_| ChannelError::InvalidCredential)?,
        );
        if matches!(&ctx.request.body, HttpBody::Bytes(bytes) if !bytes.is_empty()) {
            headers
                .entry(header::CONTENT_TYPE)
                .or_insert_with(|| HeaderValue::from_static("application/json"));
        }
        let mut builder = http::Request::builder().method(ctx.request.method).uri(uri);
        if let Some(target) = builder.headers_mut() {
            *target = headers;
        }
        builder
            .body(ctx.request.body)
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }

    fn quota_model(&self) -> Option<&dyn QuotaModel> {
        Some(self)
    }

    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        Some(self)
    }
}

