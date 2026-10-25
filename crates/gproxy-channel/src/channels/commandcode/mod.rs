//! Command Code CLI 1.84.0. Wire evidence: the npm release's `createModelClient`,
//! `toWireMessages`, `consumeStream`, `buildCommandAuthHeaders` and
//! `fetchUsageData`, recovered in samples/command-code-1.84.0/decompiled/cli.mjs.
//! See design/commandcode-1.84.0.md for the audit and reproduction commands.
//!
//! The CLI uses /alpha/generate with AI SDK messages and NDJSON events. This
//! channel presents Claude Messages to the host; other dialects use the host's
//! normal conversion. The separate /provider/v1 compatible API is not used
//! for generation. Browser login produces an API key, not a refreshable OAuth
//! token; acquire it through browser authorization or paste an existing API key.

mod config;
mod oauth;
mod quota;
mod request;
mod response;

pub use config::{CommandCodeConfig, DEFAULT_BASE_URL, ID};
pub use request::{CLI_HEADERS, CLI_VERSION};

use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ChannelError, ConfigKey, ConfigKeyKind,
    HOST_CONFIG_KEYS, LoginMode, OAuthAuthorizationCode, OperationContext, OperationFuture,
    PrepareContext, ProviderView, QuotaQuery,
};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireResponse};

#[derive(Debug, Default, Clone, Copy)]
pub struct CommandCode;

impl BaseChannel for CommandCode {
    fn id(&self) -> &'static str {
        ID
    }

    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor {
            id: ID,
            display_name: "Command Code",
            credential_kinds: vec!["api_key", "oauth"],
            login_modes: vec![LoginMode::ApiKey, LoginMode::AuthorizationCode],
            capabilities: ChannelCapabilities {
                quota_query: true,
                ..Default::default()
            },
            config_keys: [
                ConfigKey::optional(
                    "base_url",
                    ConfigKeyKind::String,
                    "Command Code API origin, without /alpha or /provider.",
                )
                .with_placeholder(DEFAULT_BASE_URL),
                ConfigKey::optional(
                    "headers",
                    ConfigKeyKind::HeaderList,
                    "Static upstream headers.",
                ),
                ConfigKey::optional(
                    "project_slug",
                    ConfigKeyKind::String,
                    "Project identity sent to the CLI API; defaults to gproxy.",
                ),
                ConfigKey::optional(
                    "zero_data_retention",
                    ConfigKeyKind::Bool,
                    "Send x-cmd-zdr: 1, matching CMD_ZDR=1 in the CLI.",
                ),
            ]
            .into_iter()
            .chain(HOST_CONFIG_KEYS)
            .collect(),
        }
    }

    fn native_dialects(&self, _: ProviderView<'_>, operation: Operation) -> Vec<Dialect> {
        match operation {
            Operation::ListModels => vec![Dialect::OpenAi],
            Operation::GenerateContent | Operation::StreamGenerateContent => vec![Dialect::Claude],
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
        ) && source.dialect != Dialect::Claude)
            .then_some(OperationKey {
                operation: source.operation,
                dialect: Dialect::Claude,
            })
    }

    fn prepare(&self, ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
        request::build(ctx)
    }

    fn generate_content<'a>(
        &'a self,
        ctx: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(response::invoke(self, ctx, false))
    }

    fn stream_generate_content<'a>(
        &'a self,
        ctx: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(response::invoke(self, ctx, true))
    }

    fn list_models<'a>(
        &'a self,
        ctx: OperationContext<'a>,
    ) -> OperationFuture<'a, WireResponse<HttpBody>> {
        Box::pin(response::models(self, ctx))
    }

    fn oauth_authorization_code(&self) -> Option<&dyn OAuthAuthorizationCode> {
        Some(self)
    }

    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        Some(self)
    }
}
