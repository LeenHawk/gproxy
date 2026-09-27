//! OpenRouter: one API key in front of many physical providers.
//!
//! Wire facts follow OpenRouter's API reference and the v3 `openrouter`
//! channel: the origin is `https://openrouter.ai/api`, so a native path joins
//! onto `/api/v1/...`; the key travels as `Authorization: Bearer`; image
//! creation and editing share `/v1/images`; `HTTP-Referer` and `X-Title`
//! attribute traffic to an application. Three request shapes are native —
//! Chat Completions, Responses and Claude Messages.
//!
//! Two things make this an upstream rather than a `custom` endpoint with a
//! different base URL. It routes: the body's `provider` object chooses among
//! the physical providers behind a model name, and this channel fills it from
//! per-model and per-provider configuration when the client expressed no
//! preference (`routing.rs`). And it prices: with usage accounting on, the
//! reply says what it charged, which the channel records as
//! `upstream_cost_usd` (`usage.rs`) instead of leaving the exchange to be
//! valued from local rates. `/v1/auth/key` reports the key's own budget
//! (`quota.rs`).

mod config;
mod quota;
mod request;
mod routing;
mod usage;

pub use config::{DEFAULT_BASE_URL, ID, OpenRouterConfig};
pub use quota::BUDGET_DIMENSION;
pub use request::ATTRIBUTION_HEADERS;
pub use usage::{UPSTREAM_COST_METRIC, UPSTREAM_PRICED_DIMENSION};

use crate::channel::{UsageExtras, 
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ConfigKey, ConfigKeyKind,
    HOST_CONFIG_KEYS, LoginMode, PrepareContext, ProviderView, QuotaModel, QuotaQuery,
    };
use gproxy_protocol::{Dialect, HttpBody, Operation};

#[derive(Debug, Default, Clone, Copy)]
pub struct OpenRouter;

impl BaseChannel for OpenRouter {
    fn id(&self) -> &'static str {
        ID
    }

    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "OpenRouter",
            login_modes: vec![LoginMode::ApiKey],
            capabilities: ChannelCapabilities {
                refresh: false,
                quota_query: true,
                quota_reset: false,
                services: false,
                websocket: false,
            },
            config_keys: [
                ConfigKey::optional(
                    "base_url",
                    ConfigKeyKind::String,
                    "Upstream origin; defaults to https://openrouter.ai/api. Provider column, not config JSON.",
                ).with_placeholder(config::DEFAULT_BASE_URL),
                ConfigKey::optional(
                    "referer",
                    ConfigKeyKind::String,
                    "HTTP-Referer sent when the client sends none; OpenRouter attributes traffic to it.",
                ),
                ConfigKey::optional(
                    "title",
                    ConfigKeyKind::String,
                    "X-Title sent when the client sends none.",
                ),
                ConfigKey::optional(
                    "provider",
                    ConfigKeyKind::Json,
                    "Default provider routing preferences (order, only, ignore, allow_fallbacks, sort, require_parameters, data_collection, quantizations, max_price), forwarded as the body's `provider`.",
                ),
                ConfigKey::optional(
                    "model_providers",
                    ConfigKeyKind::Json,
                    "Routing preferences per upstream model name, each replacing `provider` whole for that model.",
                ),
                ConfigKey::optional(
                    "usage_accounting",
                    ConfigKeyKind::Bool,
                    "Ask for usage.include so the reply reports the price charged; on by default.",
                ),
                ConfigKey::optional(
                    "normalize_service_tier",
                    ConfigKeyKind::Bool,
                    "Rewrite OpenAI's service_tier `fast` into OpenRouter's `priority`; on by default.",
                ),
                ConfigKey::optional(
                    "headers",
                    ConfigKeyKind::HeaderList,
                    "Static headers added to every upstream request.",
                ),
                ConfigKey::optional(
                    "enable_claude_magic_cache",
                    ConfigKeyKind::Bool,
                    "Turn a client's magic cache string in a Claude-dialect body into cache_control. The strings are stripped either way.",
                ),
                ConfigKey::optional(
                    "enable_openai_magic_cache",
                    ConfigKeyKind::Bool,
                    "Turn a client's magic cache string in an OpenAI Chat or Responses body into prompt_cache_breakpoint.",
                ),
            ]
            .into_iter()
            .chain(crate::channel::CLAUDE_FALLBACK_KEYS)
            .chain(HOST_CONFIG_KEYS)
            .collect(),
        }
    }

    /// Three request shapes are served verbatim; Gemini reaches OpenRouter
    /// through the host's conversion, as it has no endpoint of its own here.
    fn native_dialects(&self, _: ProviderView<'_>, _: Operation) -> Vec<Dialect> {
        vec![Dialect::OpenAi, Dialect::OpenAiChat, Dialect::Claude]
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        let config = OpenRouterConfig::from_view(ctx.provider)?;
        let uri = request::target(&ctx);
        let headers = request::headers(&ctx, &config)?;
        let dialect = ctx.operation.dialect;
        let body = request::body(ctx.request.body, &config, dialect, ctx.operation.operation)?;
        let mut builder = http::Request::builder()
            .method(ctx.request.method.clone())
            .uri(uri);
        if let Some(map) = builder.headers_mut() {
            *map = headers;
        }
        builder
            .body(body)
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }

    fn quota_model(&self) -> Option<&dyn QuotaModel> {
        Some(self)
    }

    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        Some(self)
    }

    fn usage_extras(&self) -> Option<&dyn UsageExtras> {
        Some(self)
    }
}
