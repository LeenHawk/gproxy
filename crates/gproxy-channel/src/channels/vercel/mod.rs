//! Vercel AI Gateway, ported from v3: OpenAI models, Chat, Responses and
//! embeddings, Claude Messages/count_tokens, dual API-key authentication and
//! the team's /v1/credits balance. Claude refusal fallback is gateway-owned.
mod quota;

use crate::channel::{
    BaseChannel, CLAUDE_FALLBACK_KEYS, ChannelCapabilities, ChannelDescriptor, ChannelError,
    ClaudeFallback, ConfigKey, ConfigKeyKind, HOST_CONFIG_KEYS, HeaderAllowlist, LoginMode,
    NormalizedUsage, PrepareContext, ProviderView, QuotaModel, QuotaQuery, UsageContext,
    UsageExtractor, forwardable,
};
use crate::channels::shared::{
    cache,
    claude_fallback::FallbackMode,
    claude_hygiene,
    compatible::http::{insert_configured, strip_query_auth},
    vendor_usage,
};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, connection::Bytes};
use http::{HeaderName, HeaderValue, header};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

pub const ID: &str = "vercel";
pub const DEFAULT_BASE_URL: &str = "https://ai-gateway.vercel.sh";
pub use quota::BALANCE_DIMENSION;

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct VercelConfig {
    pub headers: BTreeMap<String, String>,
    pub enable_claude_magic_cache: bool,
    pub enable_openai_magic_cache: bool,
}
impl VercelConfig {
    pub fn from_view(provider: ProviderView<'_>) -> Result<Self, ChannelError> {
        serde_json::from_value(provider.config.clone())
            .map_err(|e| ChannelError::InvalidConfig(e.to_string()))
    }
}
pub(super) fn base_url(provider: ProviderView<'_>) -> &str {
    provider
        .base_url
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_BASE_URL)
        .trim_end_matches('/')
}
pub(super) fn api_key(secret: &Value) -> Result<&str, ChannelError> {
    secret
        .get("api_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or(ChannelError::InvalidCredential)
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Vercel;
impl BaseChannel for Vercel {
    fn id(&self) -> &'static str {
        ID
    }
    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "Vercel AI Gateway",
            login_modes: vec![LoginMode::ApiKey],
            capabilities: ChannelCapabilities {
                quota_query: true,
                ..Default::default()
            },
            config_keys: [
                ConfigKey::optional(
                    "base_url",
                    ConfigKeyKind::String,
                    "Gateway origin; defaults to https://ai-gateway.vercel.sh.",
                )
                .with_placeholder(DEFAULT_BASE_URL),
                ConfigKey::optional(
                    "headers",
                    ConfigKeyKind::HeaderList,
                    "Static upstream headers.",
                ),
                ConfigKey::optional(
                    "enable_claude_magic_cache",
                    ConfigKeyKind::Bool,
                    "Place Claude cache breakpoints at magic cache strings.",
                ),
                ConfigKey::optional(
                    "enable_openai_magic_cache",
                    ConfigKeyKind::Bool,
                    "Place OpenAI cache breakpoints at magic cache strings.",
                ),
            ]
            .into_iter()
            .chain(CLAUDE_FALLBACK_KEYS)
            .chain(HOST_CONFIG_KEYS)
            .collect(),
        }
    }
    fn native_dialects(&self, _: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::ListModels | Operation::GetModel | Operation::CreateEmbedding => {
                vec![Dialect::OpenAi]
            }
            Operation::CountTokens => vec![Dialect::Claude],
            Operation::GenerateContent | Operation::StreamGenerateContent => {
                vec![Dialect::OpenAi, Dialect::OpenAiChat, Dialect::Claude]
            }
            _ => vec![],
        }
    }
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
            dialect: Dialect::OpenAi,
        })
    }
    fn claude_fallback(&self) -> Option<ClaudeFallback> {
        Some(ClaudeFallback {
            credit: false,
            recommended_model: "claude-opus-4-8",
        })
    }
    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        if !self
            .native_dialects(ctx.provider, ctx.operation.operation)
            .contains(&ctx.operation.dialect)
        {
            return Err(ChannelError::UnsupportedOperation(ctx.operation));
        }
        let config = VercelConfig::from_view(ctx.provider)?;
        let key = api_key(ctx.credential.secret)?;
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
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_str(key).map_err(|_| ChannelError::InvalidCredential)?,
        );
        headers
            .entry(header::CONTENT_TYPE)
            .or_insert(HeaderValue::from_static("application/json"));
        if ctx.operation.dialect == Dialect::Claude {
            headers
                .entry(HeaderName::from_static("anthropic-version"))
                .or_insert(HeaderValue::from_static("2023-06-01"));
        }
        // The native path is supplied by ingress or conversion. Only explicit
        // method URLs may contain a model placeholder.
        let model = match &ctx.request.body {
            HttpBody::Bytes(bytes) => serde_json::from_slice::<Value>(bytes)
                .ok()
                .and_then(|v| v.get("model").and_then(Value::as_str).map(str::to_owned)),
            _ => None,
        }
        .or_else(|| {
            ctx.request
                .path
                .strip_prefix("/v1/models/")
                .map(decode_segment)
        });
        let url = match ctx.endpoint_override {
            Some(url) => url.replace(
                "{model}",
                &encode_segment(model.as_deref().unwrap_or_default()),
            ),
            None => format!(
                "{}{}{}",
                base_url(ctx.provider),
                if ctx.request.path.starts_with('/') {
                    ""
                } else {
                    "/"
                },
                ctx.request.path
            ),
        };
        let query = ctx
            .request
            .query
            .as_deref()
            .map(strip_query_auth)
            .filter(|q| !q.is_empty());
        let uri = match query {
            Some(query) => format!("{url}{}{query}", if url.contains('?') { "&" } else { "?" }),
            None => url,
        };
        let body = match ctx.request.body {
            HttpBody::Bytes(bytes) => {
                let rules = cache::rules_for(
                    ctx.operation.dialect,
                    config.enable_claude_magic_cache,
                    config.enable_openai_magic_cache,
                );
                let bytes = cache::shape(bytes, rules);
                if let Some(mut value) = claude_hygiene::json_object(&bytes) {
                    if ctx.operation.dialect == Dialect::Claude {
                        // Never leak GProxy's retry policy to Vercel.
                        value.as_object_mut().unwrap().remove("fallbacks");
                        if ctx.operation.operation == Operation::CountTokens {
                            claude_hygiene::count_tokens(&value, &mut headers);
                        } else {
                            claude_hygiene::messages(
                                &mut value,
                                &mut headers,
                                config.enable_claude_magic_cache,
                                &FallbackMode::Off,
                                &[],
                            );
                        }
                    }
                    HttpBody::Bytes(Bytes::from(value.to_string()))
                } else {
                    HttpBody::Bytes(bytes)
                }
            }
            body => body,
        };
        let mut builder = http::Request::builder().method(ctx.request.method).uri(uri);
        if let Some(target) = builder.headers_mut() {
            *target = headers;
        }
        builder
            .body(body)
            .map_err(|e| ChannelError::InvalidConfig(e.to_string()))
    }
    fn usage_extractor(&self) -> Option<&dyn UsageExtractor> {
        Some(self)
    }
    fn quota_model(&self) -> Option<&dyn QuotaModel> {
        Some(self)
    }
    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        Some(self)
    }
}
impl UsageExtractor for Vercel {
    fn extract(&self, ctx: UsageContext<'_>) -> Result<Option<NormalizedUsage>, ChannelError> {
        if !ctx.response.status.is_success()
            || !matches!(
                ctx.operation.operation,
                Operation::GenerateContent
                    | Operation::StreamGenerateContent
                    | Operation::CreateEmbedding
            )
        {
            return Ok(None);
        }
        vendor_usage::from_body(ctx.operation.dialect, ctx.response.body)
    }
}
fn encode_segment(value: &str) -> String {
    const SEGMENT: &percent_encoding::AsciiSet = &percent_encoding::NON_ALPHANUMERIC
        .remove(b'-')
        .remove(b'_')
        .remove(b'.')
        .remove(b'~');
    percent_encoding::utf8_percent_encode(value, SEGMENT).to_string()
}
fn decode_segment(value: &str) -> String {
    percent_encoding::percent_decode_str(value)
        .decode_utf8_lossy()
        .into_owned()
}
