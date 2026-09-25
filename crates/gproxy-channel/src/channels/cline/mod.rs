//! Cline: the coding agent's own account, not a passthrough to a vendor.
//!
//! Wire facts follow the v3 channel (`crates/gproxy-channels/src/cline` on
//! `main`). Cline fronts many vendors behind one Chat Completions route on
//! its own API (`https://api.cline.bot/api/v1`), and everything else about it
//! is Cline's: identity is delegated to WorkOS but the token traffic presents
//! is one Cline minted (`oauth.rs`), replies come wrapped in a `{success,
//! data}` envelope (`response.rs`), the catalogue is a pair of recommendation
//! groups rather than a model list, and the account has both credits and plan
//! windows (`quota.rs`).
//!
//! A credential is either a pasted `api_key` or the account token a login
//! produced. They authenticate differently — the account token is announced
//! with the `workos:` scheme the upstream keys account traffic on — so
//! `auth.rs` decides from the secret's shape, not from `auth_kind`.
//!
//! **v3 infrastructure that has no v4 counterpart.** v3's `routes.rs`
//! (`SurfaceTable` written with the `route!` macro) is replaced by
//! `native_dialects` plus the host's protocol conversion; the channel no
//! longer lists "claude→openai" conversion pairs of its own. v3's
//! `ChannelLogin` becomes `OAuthDeviceCode`. v3's `shape_response` and
//! `StreamDecoder` become the overridden `list_models` and `generate_content`,
//! which unwrap inside the channel before the host sees a body. v3's
//! `ChannelTrafficPolicy` (which forwarded no client header at all) has no v4
//! counterpart: v4 forwards everything but the fixed drops, and an operator
//! who wants v3's behaviour writes `allowed_headers: []`.
//!
//! **No `default_connection()`**: v3 had no `cline/profile.rs`, so there is no
//! captured client identity to reproduce and the operator's connection
//! profile is the only thing that decides the outbound stack.

mod auth;
mod config;
mod oauth;
mod quota;
mod request;
mod response;
mod usage;

pub use auth::CLIENT_HEADERS;
pub use config::{
    ClineConfig, DEFAULT_BASE_URL, DEFAULT_CLIENT_ID, DEFAULT_DEVICE_AUTHORIZATION_URL,
    DEFAULT_TOKEN_URL, ID,
};
pub use quota::{BALANCE_DIMENSION, PLAN_SOURCE};

use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ConfigKey, ConfigKeyKind,
    CredentialRefresh, HOST_CONFIG_KEYS, LoginMode, OAuthDeviceCode, OperationContext,
    OperationFuture, PrepareContext, ProviderView, QuotaModel, QuotaQuery, UsageExtractor,
    UsageStream,
};
use gproxy_protocol::{Dialect, HttpBody, Operation, WireResponse};

#[derive(Debug, Default, Clone, Copy)]
pub struct Cline;

impl BaseChannel for Cline {
    fn id(&self) -> &'static str {
        ID
    }

    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "Cline",
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
                    "Cline API origin, `/api/v1` included; defaults to https://api.cline.bot/api/v1. Provider column, not config JSON.",
                ).with_placeholder(config::DEFAULT_BASE_URL),
                ConfigKey::optional(
                    "device_authorization_url",
                    ConfigKeyKind::String,
                    "Where the device login starts; defaults to WorkOS's device authorization endpoint.",
                ).with_placeholder(config::DEFAULT_DEVICE_AUTHORIZATION_URL),
                ConfigKey::optional(
                    "token_url",
                    ConfigKeyKind::String,
                    "Where the device login polls for the WorkOS token pair.",
                ).with_placeholder(config::DEFAULT_TOKEN_URL),
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

    /// One Chat Completions route and one catalogue; every other dialect is
    /// the host's conversion problem.
    fn native_dialects(&self, _: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::GenerateContent
            | Operation::StreamGenerateContent
            | Operation::ListModels => vec![Dialect::OpenAiChat],
            _ => Vec::new(),
        }
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        request::build(ctx)
    }

    /// The catalogue is two recommendation groups, not a model list.
    fn list_models<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(response::invoke(self, Operation::ListModels, context))
    }

    /// A buffered reply arrives inside the `{success, data}` envelope.
    fn generate_content<'a>(
        &'a self,
        context: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(response::invoke(self, Operation::GenerateContent, context))
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
