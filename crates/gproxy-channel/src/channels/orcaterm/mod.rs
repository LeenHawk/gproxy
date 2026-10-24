//! OrcaTerm 1.6.6's LightAI account transport, recovered from the official
//! Windows x64 installer (SHA-256 f42940c099bc32fbe668adfea25fd5353f2b711b1e705536638aa1f78b28c173).
//! Wire sources: samples/orcaterm-1.6.6/modules/{2481,26196,82025,92054}.js;
//! the full audit and source locations are in docs/src/content/docs/guides/orcaterm.md.
//!
//! The desktop SDK sends /assistant/chat with historyMessages and parses an
//! Agent JSON object inside each SSE message's content: completion answers
//! are taskCompletion, questions are question, and thinking is separate.
//! It is not an OpenAI-compatible upstream. This channel exposes text Chat
//! Completions, including streamed reasoning and multi-turn history, to the
//! host's normal converters. Unsupported client tools and media are rejected,
//! never silently dropped or executed as OrcaTerm's built-in terminal tools.
//! Token counts are not present in the audited wire and remain unreported.
//!
//! Browser OAuth follows the desktop authorize/session_id exchange and uses
//! the host's device-flow polling UI; no cookie or callback needs copying.
//! Cookie import remains optional. GetUserInfo supplies the account identity
//! and OAuthRefreshToken renews the session. Built-in models come from the same
//! public Rainbow configuration as the desktop, not a hard-coded model list.
//! QuotaQuery reads the account's hunyuan_ai question allowance independently
//! of per-request token usage; its model coverage is not guessed.

mod auth;
mod config;
mod models;
mod oauth;
mod quota;
mod request;
mod response;

pub use config::{DEFAULT_BASE_URL, ID, OrcaTermConfig};
pub use request::CLIENT_HEADERS;

use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ConfigKey, ConfigKeyKind,
    CredentialRefresh, HOST_CONFIG_KEYS, LoginMode, OAuthDeviceCode, OperationContext,
    OperationFuture, PrepareContext, ProviderView, QuotaQuery,
};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireResponse};

#[derive(Debug, Default, Clone, Copy)]
pub struct OrcaTerm;

impl BaseChannel for OrcaTerm {
    fn id(&self) -> &'static str {
        ID
    }

    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "OrcaTerm",
            credential_kinds: vec!["oauth"],
            login_modes: vec![LoginMode::DeviceCode],
            capabilities: ChannelCapabilities { refresh: true, quota_query: true, ..Default::default() },
            config_keys: [
                ConfigKey::optional("base_url", ConfigKeyKind::String,
                    "LightAI origin, without /assistant. Defaults to the selected OrcaTerm region.")
                    .with_placeholder(DEFAULT_BASE_URL),
                ConfigKey::optional("international", ConfigKeyKind::Bool,
                    "Use OrcaTerm's international account and LightAI endpoints. Defaults to false."),
                ConfigKey::optional("site_url", ConfigKeyKind::String,
                    "Override the OrcaTerm account origin used for OAuth, cookie initialization and token refresh."),
                ConfigKey::optional("oauth_provider", ConfigKeyKind::String,
                    "OAuth login provider: wechat, txcloud, google or github. Defaults to wechat in China and google internationally."),
                ConfigKey::optional("headers", ConfigKeyKind::HeaderList,
                    "Static upstream headers."),
            ].into_iter().chain(HOST_CONFIG_KEYS).collect(),
        }
    }

    fn native_dialects(&self, _: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::ListModels => vec![Dialect::OpenAi],
            Operation::GenerateContent | Operation::StreamGenerateContent => {
                vec![Dialect::OpenAiChat]
            }
            _ => vec![],
        }
    }

    fn default_conversion_target(
        &self,
        _: ProviderView<'_>,
        source: OperationKey,
    ) -> Option<OperationKey> {
        (matches!(
            source.operation,
            Operation::GenerateContent | Operation::StreamGenerateContent
        ) && source.dialect != Dialect::OpenAiChat)
            .then_some(OperationKey {
                operation: source.operation,
                dialect: Dialect::OpenAiChat,
            })
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        request::prepare(ctx)
    }

    fn generate_content<'a>(
        &'a self,
        ctx: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(response::invoke(ctx, false))
    }

    fn stream_generate_content<'a>(
        &'a self,
        ctx: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(response::invoke(ctx, true))
    }

    fn list_models<'a>(
        &'a self,
        ctx: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(models::list(ctx))
    }
    fn oauth_device_code(&self) -> Option<&dyn OAuthDeviceCode> {
        Some(self)
    }
    fn validate_credential_secret(&self, secret: &serde_json::Value) -> Result<(), ChannelError> {
        auth::headers(secret).map(|_| ())
    }

    fn initial_credential_expiry(&self, _secret: &serde_json::Value) -> Option<i64> {
        Some(0)
    }

    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        Some(self)
    }
    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        Some(self)
    }
}
