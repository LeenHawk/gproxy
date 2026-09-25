//! xAI: Grok through OpenAI Chat and Responses, with xAI's own media paths.
//!
//! Wire facts follow xAI's API reference and the v3 `xai` channel. Chat
//! Completions and Responses sit where OpenAI puts them under
//! `https://api.x.ai`, but speech is `/v1/tts`, transcription `/v1/stt` and
//! video creation `/v1/videos/generations`; the speech body is renamed into
//! xAI's own fields (`request.rs`). Usage carries `cost_in_usd_ticks`, xAI's
//! metering unit, and a finished video job states its price in dollars
//! (`usage.rs`). Billing is a different host behind a different key
//! (`quota.rs`).
//!
//! Not ported from v3: the transcription multipart rebuild, which stripped
//! OpenAI-only form fields, moved the file part last and synthesized a
//! filename. The path is correct here but a multipart body is forwarded as
//! the caller wrote it, so an OpenAI-shaped transcription may carry fields
//! xAI rejects. Nor is the `grok-4.6` metadata patch on `ListModels`: that
//! belongs to the model catalogue, not to the wire. Grok's OAuth build
//! surface is the separate `grokbuild` upstream, not this one.

mod config;
mod quota;
mod request;
mod usage;

pub use config::{DEFAULT_BASE_URL, DEFAULT_MANAGEMENT_URL, ID, XaiConfig};
pub use quota::{POSTPAID_DIMENSION, PREPAID_DIMENSION};
pub use request::CLIENT_HEADERS;
pub use usage::{COST_TICKS_METRIC, UPSTREAM_COST_METRIC, UPSTREAM_PRICED_DIMENSION};

use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ConfigKey, ConfigKeyKind,
    HOST_CONFIG_KEYS, LoginMode, PrepareContext, ProviderView, QuotaModel, QuotaQuery,
    UsageExtractor, UsageStream,
};
use gproxy_protocol::{Dialect, HttpBody, Operation};

#[derive(Debug, Default, Clone, Copy)]
pub struct Xai;

impl BaseChannel for Xai {
    fn id(&self) -> &'static str {
        ID
    }

    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "xAI (Grok)",
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
                    "Upstream origin; defaults to https://api.x.ai. Provider column, not config JSON.",
                ).with_placeholder(config::DEFAULT_BASE_URL),
                ConfigKey::optional(
                    "headers",
                    ConfigKeyKind::HeaderList,
                    "Static headers added to every upstream request.",
                ),
                ConfigKey::optional(
                    "quota_api_key",
                    ConfigKeyKind::String,
                    "An xAI management key, which is not the inference key. Without it there is no billing surface to read.",
                ),
                ConfigKey::optional(
                    "quota_team_id",
                    ConfigKeyKind::String,
                    "The team whose prepaid balance and postpaid limit the probe reads.",
                ),
                ConfigKey::optional(
                    "quota_base_url",
                    ConfigKeyKind::String,
                    "Management origin; defaults to https://management-api.x.ai.",
                ).with_placeholder(config::DEFAULT_MANAGEMENT_URL),
            ]
            .into_iter()
            .chain(HOST_CONFIG_KEYS)
            .collect(),
        }
    }

    /// Grok serves OpenAI's two shapes; Claude and Gemini callers reach it
    /// through the host's conversion.
    fn native_dialects(&self, _: ProviderView<'_>, _: Operation) -> Vec<Dialect> {
        vec![Dialect::OpenAiChat, Dialect::OpenAi]
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

    fn usage_extractor(&self) -> Option<&dyn UsageExtractor> {
        Some(self)
    }

    fn usage_stream(&self) -> Option<&dyn UsageStream> {
        Some(self)
    }
}
