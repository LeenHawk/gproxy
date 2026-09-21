//! Moonshot Kimi: one channel over two products that share a vendor.
//!
//! An API key buys the Moonshot platform at `https://api.moonshot.cn`, an
//! ordinary OpenAI-compatible origin. A device login buys the Kimi Code
//! subscription at `https://api.kimi.com/coding/v1`, whose origin already
//! carries the `/v1`, which answers Claude Messages on its own wire as well as
//! Chat Completions, and which will only serve a request that presents the
//! CLI's `x-msh-*` identity — platform, version, device name, model, OS and
//! above all the device id minted at login (`auth.rs`, `oauth.rs`).
//!
//! The credential shape decides which product a request reaches, because it is
//! the credential that can authenticate against one and not the other. The
//! quota surface follows it too: `/usages` reports the subscription's rolling
//! windows, a platform key reports a cash balance (`quota.rs`).
//!
//! `native_dialects` is the one question the host asks without a credential in
//! hand, so the provider row answers it through the `product` key; unstated, it
//! is read off `base_url`, and failing that assumed to be the platform, whose
//! narrower answer converts safely either way.
//!
//! Not ported from v3: reading the device name and OS release out of the host
//! environment. Preparation here is pure and a proxy is not the machine it
//! claims to be, so an operator states them in configuration and `unknown`
//! stands where they said nothing.

mod auth;
mod config;
mod oauth;
mod quota;
mod request;
mod usage;

pub use auth::Mode;
pub use config::{
    CLI_PLATFORM, DEFAULT_API_BASE_URL, DEFAULT_CLI_VERSION, DEFAULT_CLIENT_ID,
    DEFAULT_CODE_BASE_URL, DEFAULT_OAUTH_HOST, ID, KimiConfig, Product,
};
pub use quota::{BALANCE_DIMENSION, WEEKLY_DIMENSION};

use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ConfigKey, ConfigKeyKind,
    CredentialRefresh, HOST_CONFIG_KEYS, LoginMode, OAuthDeviceCode, PrepareContext, ProviderView,
    QuotaModel, QuotaQuery, UsageExtractor, UsageStream,
};
use gproxy_protocol::{Dialect, HttpBody, Operation};

#[derive(Debug, Default, Clone, Copy)]
pub struct Kimi;

impl BaseChannel for Kimi {
    fn id(&self) -> &'static str {
        ID
    }

    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "Moonshot (Kimi)",
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
                    "Upstream origin; defaults to https://api.moonshot.cn for a key and https://api.kimi.com/coding/v1 for a subscription. Provider column, not config JSON.",
                ),
                ConfigKey::optional(
                    "product",
                    ConfigKeyKind::String,
                    "`platform` or `code`. Says whether this provider's upstream answers Claude Messages natively; read off base_url when unstated.",
                ),
                ConfigKey::optional(
                    "oauth_host",
                    ConfigKeyKind::String,
                    "Where the device login and refresh talk; defaults to https://auth.kimi.com.",
                ),
                ConfigKey::optional(
                    "client_id",
                    ConfigKeyKind::String,
                    "OAuth client the device login presents; defaults to the Kimi Code CLI's.",
                ),
                ConfigKey::optional(
                    "cli_version",
                    ConfigKeyKind::String,
                    "Version announced in user-agent and x-msh-version.",
                ),
                ConfigKey::optional(
                    "device_name",
                    ConfigKeyKind::String,
                    "Machine name announced in x-msh-device-name; `unknown` when unset.",
                ),
                ConfigKey::optional(
                    "device_model",
                    ConfigKeyKind::String,
                    "Hardware announced in x-msh-device-model; the build's os and arch when unset.",
                ),
                ConfigKey::optional(
                    "os_version",
                    ConfigKeyKind::String,
                    "Release announced in x-msh-os-version; the build's target OS when unset.",
                ),
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

    /// Both products serve OpenAI's two shapes; only the subscription has a
    /// Claude Messages endpoint of its own.
    fn native_dialects(&self, provider: ProviderView<'_>, _: Operation) -> Vec<Dialect> {
        let product = KimiConfig::from_view(provider)
            .map(|config| config.product(provider))
            .unwrap_or_default();
        match product {
            Product::Platform => vec![Dialect::OpenAiChat, Dialect::OpenAi],
            Product::Code => vec![Dialect::OpenAiChat, Dialect::OpenAi, Dialect::Claude],
        }
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        let (builder, request) = request::build(ctx)?;
        builder
            .body(request.body)
            .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
    }

    fn oauth_device_code(&self) -> Option<&dyn OAuthDeviceCode> {
        Some(self)
    }

    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        Some(self)
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
