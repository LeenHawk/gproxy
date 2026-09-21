//! OpenCode: the `opencode` CLI's own subscription, on either of its tiers.
//!
//! Wire facts follow the v3 channel (`crates/gproxy-channels/src/opencode`
//! on `main`) and the quota code it shared
//! (`shared/quota_management/opencode.rs`). One account fronts many vendors
//! behind three compatible surfaces on one origin — Chat Completions,
//! Responses and Claude Messages — and the credential is presented as a
//! bearer everywhere except the Claude surface, which wants `x-api-key` and
//! Anthropic's version header (`request.rs`).
//!
//! The tier is the provider's, not the credential's: `zen`
//! (`opencode.ai/zen/v1`) and `go` (`opencode.ai/zen/go/v1`) are separate
//! origins with separate quota, and only `go` reports usage windows
//! (`quota.rs`). v3 carried two channel ids, `opencodezen` and `opencodego`,
//! canonicalized into one; v4 has one channel and a `tier` key.
//!
//! A credential is either a pasted `api_key` or the account token the
//! Console device login produced (`oauth.rs`); both are the same bearer to
//! the upstream, so `prepare` accepts either name.
//!
//! **v3 infrastructure that has no v4 counterpart.** v3's `routes.rs`
//! (`SurfaceTable` written with the `route!` macro) is replaced by
//! `native_dialects` plus the host's protocol conversion. v3's `ChannelLogin`
//! becomes `OAuthDeviceCode`. v3's `StreamDecoder` is gone: every surface
//! answers its own dialect verbatim, so nothing is decoded on the way out and
//! only `UsageStream` watches the bytes. v3's `ChannelTrafficPolicy`
//! (`x-opencode-session` in, OpenAI's response headers out) becomes the
//! declared `SESSION_HEADERS`; v4 forwards every client header but the fixed
//! drops, and an operator who wants v3's narrower set writes
//! `allowed_headers`.
//!
//! **No `default_connection()`**: v3 had no `opencode/profile.rs`, so there
//! is no captured client identity to reproduce.

mod config;
mod oauth;
mod quota;
mod request;
mod usage;

pub use config::{
    DEFAULT_CLIENT_ID, DEFAULT_CONSOLE_BASE_URL, GO_BASE_URL, ID, OpenCodeConfig, Tier,
    ZEN_BASE_URL,
};
pub use quota::GO_SOURCE;
pub use request::SESSION_HEADERS;

use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ConfigKey, ConfigKeyKind,
    CredentialRefresh, HOST_CONFIG_KEYS, LoginMode, OAuthDeviceCode, PrepareContext, ProviderView,
    QuotaQuery, UsageExtractor, UsageStream,
};
use gproxy_protocol::{Dialect, HttpBody, Operation};

#[derive(Debug, Default, Clone, Copy)]
pub struct OpenCode;

impl BaseChannel for OpenCode {
    fn id(&self) -> &'static str {
        ID
    }

    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "OpenCode (Zen and Go)",
            login_modes: vec![LoginMode::ApiKey, LoginMode::DeviceCode],
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
                    "Upstream origin, `/v1` included; defaults to the tier's own. Provider column, not config JSON.",
                ),
                ConfigKey::optional(
                    "tier",
                    ConfigKeyKind::String,
                    "`zen` or `go`. Decides the default origin and whether the account reports usage windows.",
                ),
                ConfigKey::optional(
                    "console_base_url",
                    ConfigKeyKind::String,
                    "Where the device login and the refresh talk; defaults to https://console.opencode.ai.",
                ),
                ConfigKey::optional(
                    "client_id",
                    ConfigKeyKind::String,
                    "OAuth client the device login presents; defaults to the opencode CLI's.",
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
        request::build(ctx)
    }

    fn oauth_device_code(&self) -> Option<&dyn OAuthDeviceCode> {
        Some(self)
    }

    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
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
