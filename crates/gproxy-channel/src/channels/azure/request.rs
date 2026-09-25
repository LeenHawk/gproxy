//! The `BaseChannel` implementation: which dialects Azure serves natively,
//! and how one request is built. Every operation uses the default HTTP flow;
//! nothing here needs more than one exchange.

use super::config::AzureConfig;
use super::usage::AzureUsage;
use super::{Azure, ID, endpoint};
use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ConfigKey, ConfigKeyKind,
    HOST_CONFIG_KEYS, HeaderAllowlist, LoginMode, PrepareContext, ProviderView, UsageExtractor,
    forwardable,
};
use gproxy_protocol::{Dialect, HttpBody, Operation, WireFamily};
use http::{HeaderName, HeaderValue};

/// Azure's Anthropic surface expects the same version header as Anthropic's.
const DEFAULT_ANTHROPIC_VERSION: &str = "2023-06-01";

const USAGE: AzureUsage = AzureUsage;

impl BaseChannel for Azure {
    fn claude_fallback(&self) -> Option<crate::channel::ClaudeFallback> {
        Some(crate::channel::ClaudeFallback {
            credit: true,
            recommended_model: "claude-opus-4-8",
        })
    }

    fn id(&self) -> &'static str {
        ID
    }

    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "Microsoft Azure",
            login_modes: vec![LoginMode::ApiKey],
            capabilities: ChannelCapabilities::default(),
            config_keys: [
                ConfigKey::optional(
                    "base_url",
                    ConfigKeyKind::String,
                    "Complete origin of the Azure endpoint, for a Foundry project, a private endpoint or an API Management front door. Provider column, not config JSON. Replaces `resource`.",
                ).with_placeholder("https://{resource}.openai.azure.com"),
                ConfigKey::optional(
                    "resource",
                    ConfigKeyKind::String,
                    "Azure OpenAI resource name; the origin becomes https://{resource}.openai.azure.com. Required unless the provider has a base_url.",
                ),
                ConfigKey::optional(
                    "api_version",
                    ConfigKeyKind::String,
                    "api-version query value. Optional on the v1 surface; required when `deployment` is set.",
                ),
                ConfigKey::optional(
                    "deployment",
                    ConfigKeyKind::String,
                    "Deployment name. Set it to address the deployment-scoped layout /openai/deployments/{deployment}/... instead of the version-less v1 surface.",
                ),
            ]
            .into_iter()
            .chain(crate::channel::CLAUDE_FALLBACK_KEYS)
            .chain(HOST_CONFIG_KEYS)
            .collect(),
        }
    }

    /// Azure resells OpenAI and Anthropic verbatim and serves no Gemini
    /// surface at all. A resource without Anthropic deployments simply fails
    /// upstream; which models a provider actually has is routing configuration,
    /// not something this channel can know.
    fn default_conversion_target(&self, _provider: ProviderView<'_>, source: gproxy_protocol::OperationKey) -> Option<gproxy_protocol::OperationKey> {
        matches!(source.operation, Operation::GenerateContent | Operation::StreamGenerateContent)
            .then_some(gproxy_protocol::OperationKey { operation: source.operation, dialect: Dialect::OpenAi })
    }

    fn native_dialects(&self, _provider: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::GenerateContent | Operation::StreamGenerateContent => {
                vec![Dialect::OpenAi, Dialect::OpenAiChat, Dialect::Claude]
            }
            // Only Anthropic's surface counts tokens; Azure OpenAI has no
            // equivalent method.
            Operation::CountTokens => vec![Dialect::Claude],
            Operation::ListModels
            | Operation::GetModel
            | Operation::CompactContent
            | Operation::CreateEmbedding
            | Operation::CreateImage
            | Operation::EditImage
            | Operation::CreateVideo
            | Operation::RetrieveVideo
            | Operation::ListVideos
            | Operation::DeleteVideo
            | Operation::DownloadVideoContent => vec![Dialect::OpenAi],
            _ => Vec::new(),
        }
    }

    /// The body is the vendor's own and passes through untouched, streamed or
    /// buffered: Azure accepts the same request shapes as the vendors it hosts.
    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        let config = AzureConfig::from_view(ctx.provider)?;
        let key = api_key(&ctx)?;
        let uri = endpoint::method_url(
            &config,
            ctx.provider.base_url,
            ctx.endpoint_override,
            ctx.operation,
            &ctx.request.path,
            ctx.request.query.as_deref(),
        )?;
        let allowlist = HeaderAllowlist::from_view(ctx.provider)?;
        let mut headers = forwardable(&ctx.request.headers, allowlist.as_ref(), &[]);
        match ctx.operation.dialect.family() {
            WireFamily::Claude => {
                headers.insert(
                    HeaderName::from_static("x-api-key"),
                    HeaderValue::from_str(key).map_err(|_| ChannelError::InvalidCredential)?,
                );
                headers
                    .entry(HeaderName::from_static("anthropic-version"))
                    .or_insert_with(|| HeaderValue::from_static(DEFAULT_ANTHROPIC_VERSION));
            }
            // Gemini never reaches here: `method_url` rejects it first.
            _ => {
                headers.insert(
                    HeaderName::from_static("api-key"),
                    HeaderValue::from_str(key).map_err(|_| ChannelError::InvalidCredential)?,
                );
            }
        }
        let mut builder = http::Request::builder()
            .method(ctx.request.method.clone())
            .uri(uri);
        if let Some(map) = builder.headers_mut() {
            *map = headers;
        }
        builder
            .body(ctx.request.body)
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }

    fn usage_extractor(&self) -> Option<&dyn UsageExtractor> {
        Some(&USAGE)
    }
}

fn api_key<'a>(ctx: &PrepareContext<'a>) -> Result<&'a str, ChannelError> {
    ctx.credential
        .secret
        .get("api_key")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or(ChannelError::InvalidCredential)
}
