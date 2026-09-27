//! DeepSeek: OpenAI Chat, OpenAI Responses and Claude Messages on one origin,
//! each at a path of its own.
//!
//! Wire facts follow DeepSeek's API reference and the v3 `deepseek` channel.
//! Chat Completions sits under `/v1`, Responses sits at the origin root
//! (`/responses`, no `/v1`), and the Anthropic-compatible surface hangs off
//! `/anthropic`. The key is a bearer token everywhere except that surface,
//! which wants `x-api-key`; unlike Anthropic itself, DeepSeek asks for no
//! `anthropic-version`, so the channel sends none. Prompt caching is
//! automatic and reported under DeepSeek's own
//! `usage.prompt_cache_hit_tokens`, which is what `cached_input_tokens`
//! means here. `/user/balance` reports the account's remaining credit.

mod config;
mod quota;
mod request;
mod usage;

pub use config::{DEFAULT_BASE_URL, DeepSeekConfig, ID};
pub use quota::BALANCE_DIMENSION;

use crate::channel::{UsageExtras, 
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ConfigKey, ConfigKeyKind,
    HOST_CONFIG_KEYS, LoginMode, PrepareContext, ProviderView, QuotaModel, QuotaQuery,
    };
use gproxy_protocol::{Dialect, HttpBody, Operation};

#[derive(Debug, Default, Clone, Copy)]
pub struct DeepSeek;

impl BaseChannel for DeepSeek {
    fn id(&self) -> &'static str {
        ID
    }

    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "DeepSeek",
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
                    "Upstream origin; defaults to https://api.deepseek.com. Provider column, not config JSON.",
                ).with_placeholder(config::DEFAULT_BASE_URL),
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

    fn native_dialects(&self, _: ProviderView<'_>, _: Operation) -> Vec<Dialect> {
        vec![Dialect::OpenAiChat, Dialect::OpenAi, Dialect::Claude]
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        let (builder, request) = request::build(ctx)?;
        builder
            .body(request.body)
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
