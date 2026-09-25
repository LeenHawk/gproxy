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
//! Two channels, one per product: `opencodezen` (`opencode.ai/zen/v1`,
//! pay-as-you-go from the Console wallet, full model list) and `opencodego`
//! (`opencode.ai/zen/go/v1`, the subscription, open models only, the only one
//! that reports usage windows — `quota.rs`). They were two channels in v3 as
//! well; v4 merged them behind a `tier` key for a while, which let a Go row
//! offer the Console account login that only Zen has in the v2 client
//! (opencode `packages/core/src/plugin/provider/opencode.ts` registers the
//! device method on `opencode`; `opencode-go` is a pasted key only). The wire
//! is otherwise the same, so both are one type carrying its [`Tier`].
//!
//! A Zen credential is either a pasted `api_key` or the account token the
//! Console device login produced (`oauth.rs`); both are the same bearer to
//! the upstream, so `prepare` accepts either name. A Go credential is a key.
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
    DEFAULT_CLIENT_ID, DEFAULT_CONSOLE_BASE_URL, GO_BASE_URL, GO_ID, OpenCodeConfig, Tier,
    ZEN_BASE_URL, ZEN_ID,
};
pub use quota::GO_SOURCE;
pub use request::SESSION_HEADERS;

use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ConfigKey, ConfigKeyKind,
    CredentialRefresh, HOST_CONFIG_KEYS, LoginMode, OAuthDeviceCode, PrepareContext, ProviderView,
    QuotaQuery, UsageExtractor, UsageStream,
};
use gproxy_protocol::{Dialect, HttpBody, Operation};

/// One OpenCode product; register [`OpenCode::ZEN`] and [`OpenCode::GO`].
#[derive(Debug, Clone, Copy)]
pub struct OpenCode {
    tier: Tier,
}

impl OpenCode {
    pub const ZEN: Self = Self { tier: Tier::Zen };
    pub const GO: Self = Self { tier: Tier::Go };

    pub const fn tier(&self) -> Tier {
        self.tier
    }

    const fn is_zen(&self) -> bool {
        matches!(self.tier, Tier::Zen)
    }
}

impl BaseChannel for OpenCode {
    fn id(&self) -> &'static str {
        self.tier.id()
    }

    fn descriptor(&self) -> ChannelDescriptor {
        let zen = self.is_zen();
        let base_url = ConfigKey::optional(
            "base_url",
            ConfigKeyKind::String,
            "Upstream origin, `/v1` included. Provider column, not config JSON.",
        )
        .with_placeholder(self.tier.default_base_url());
        // Only Zen has an account login, so only Zen has a console to reach.
        let console = zen.then(|| {
            ConfigKey::optional(
                "console_base_url",
                ConfigKeyKind::String,
                "Where the device login and the refresh talk.",
            )
            .with_placeholder(config::DEFAULT_CONSOLE_BASE_URL)
        });
        ChannelDescriptor {
            id: self.tier.id(),
            display_name: if zen { "OpenCode Zen" } else { "OpenCode Go" },
            login_modes: if zen {
                vec![LoginMode::ApiKey, LoginMode::DeviceCode]
            } else {
                vec![LoginMode::ApiKey]
            },
            capabilities: ChannelCapabilities {
                refresh: zen,
                quota_query: !zen,
                quota_reset: false,
                services: false,
                websocket: false,
            },
            config_keys: [Some(base_url), console]
                .into_iter()
                .flatten()
                .chain([
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
            ])
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
        request::build(ctx, self.tier)
    }

    fn oauth_device_code(&self) -> Option<&dyn OAuthDeviceCode> {
        self.is_zen().then_some(self as &dyn OAuthDeviceCode)
    }

    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        self.is_zen().then_some(self as &dyn CredentialRefresh)
    }

    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        (!self.is_zen()).then_some(self as &dyn QuotaQuery)
    }

    fn usage_extractor(&self) -> Option<&dyn UsageExtractor> {
        Some(self)
    }

    fn usage_stream(&self) -> Option<&dyn UsageStream> {
        Some(self)
    }
}
