//! Trae desktop: China 3.4.3 / international 3.6.2, official Linux distributions
//! under samples/trae/{china,international}. See this module's README.md for sources.
//! product.json supplies region origins, app/client IDs and versions;
//! out/main.js supplies PKCE, device-bound OAuth and refresh signatures;
//! ai-modules-chat's generated SDK supplies GetConfigDetail and LLMRawChatV2;
//! libai_agent.so/libharness.so retain the raw SSE/usage fields; the workbench
//! ICubeUsageService supplies quota endpoints and entitlement field meanings.
//!
//! This channel translates the native raw chat envelope into OpenAI Chat.
//! It never executes tools, injects tool prompts, aliases model names, reads
//! local Trae credentials, or chooses an outbound client. Account credentials
//! carry their own device key; the host seals it as a provider secret.
mod common;
mod config;
mod models;
mod oauth;
mod quota;
mod request;
mod response;

use crate::channel::{
    BaseChannel, ChannelCapabilities, ChannelDescriptor, ConfigKey, ConfigKeyKind,
    CredentialRefresh, HOST_CONFIG_KEYS, LoginMode, OAuthAuthorizationCode, OperationContext,
    OperationFuture, ProviderView, QuotaQuery,
};
pub use config::{ID, TraeConfig};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey, WireResponse};
pub use request::CLI_HEADERS;

#[derive(Debug, Default, Clone, Copy)]
pub struct Trae;
impl BaseChannel for Trae {
    fn id(&self) -> &'static str {
        ID
    }
    fn descriptor(&self) -> ChannelDescriptor {
        ChannelDescriptor { id:ID,display_name:"Trae",credential_kinds:vec!["oauth"],login_modes:vec![LoginMode::AuthorizationCode],
            capabilities:ChannelCapabilities { refresh:true,quota_query:true,..Default::default() },
            config_keys:[
                ConfigKey::optional("international",ConfigKeyKind::Bool,"Use international Trae; defaults to mainland China. Account region selects the international service origin."),
                ConfigKey::optional("base_url",ConfigKeyKind::String,"Chat and model API origin override. Defaults to the selected edition and account region."),
                ConfigKey::optional("auth_base_url",ConfigKeyKind::String,"Authentication and account API origin override. Normally selected from the edition and account region."),
                ConfigKey::optional("ide_version",ConfigKeyKind::String,"Desktop version override. Defaults to the audited edition's version."),
                ConfigKey::optional("headers",ConfigKeyKind::HeaderList,"Static upstream headers; account authentication and device identity remain credential-owned."),
            ].into_iter().chain(HOST_CONFIG_KEYS).collect(),
        }
    }
    fn native_dialects(&self, _: ProviderView<'_>, op: Operation) -> Vec<Dialect> {
        match op {
            Operation::GenerateContent | Operation::StreamGenerateContent => {
                vec![Dialect::OpenAiChat]
            }
            Operation::ListModels => vec![Dialect::OpenAi],
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
    fn oauth_authorization_code(&self) -> Option<&dyn OAuthAuthorizationCode> {
        Some(&oauth::OAuth)
    }
    fn credential_refresh(&self) -> Option<&dyn CredentialRefresh> {
        Some(&oauth::OAuth)
    }
    fn quota_query(&self) -> Option<&dyn QuotaQuery> {
        Some(&quota::Quota)
    }
}
