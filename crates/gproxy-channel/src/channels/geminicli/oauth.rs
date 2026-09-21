//! The Google login and token refresh, wired to the Gemini CLI's own client.
//!
//! `authorize` builds the consent URL for the host's PKCE challenge and
//! state; `exchange` turns the code into tokens and then learns the Cloud
//! project and the tier, which travel back in `provider_fields` for the host
//! to persist. `refresh` renews the access token, carries the discovered
//! facts forward, and completes a discovery that the login could not finish
//! (v3 `geminicli/{login,auth}.rs`, `shared/{google_login,google_oauth}.rs`).

use super::{
    DEFAULT_CLIENT_ID, DEFAULT_CLIENT_SECRET, DEFAULT_REDIRECT_URI, FALLBACK_TIER, GeminiCli,
    GeminiCliConfig, OAUTH_SCOPE, base_url,
};
use crate::channel::{
    AuthorizationCode, AuthorizationRequest, AuthorizationStart, ChannelError, CredentialRefresh,
    CredentialUpdate, LoginContext, OAuthAuthorizationCode, OAuthCredential, OperationFuture,
    RefreshContext,
};
use crate::channels::shared::code_assist::google::{self, GoogleTool};
use gproxy_protocol::capability::CapabilityFuture;
use serde_json::{Value, json};

/// `ClientMetadata` for `loadCodeAssist`/`onboardUser`: the CLI reports no
/// IDE and no platform, and identifies its plugin as `GEMINI`; a project
/// hint is repeated as `duetProject` (v3 `geminicli/login.rs::metadata`).
fn metadata(project: Option<&str>) -> Value {
    let mut metadata = json!({
        "ideType": "IDE_UNSPECIFIED",
        "platform": "PLATFORM_UNSPECIFIED",
        "pluginType": "GEMINI",
    });
    if let Some(project) = project {
        metadata["duetProject"] = Value::String(project.into());
    }
    metadata
}

fn tool(config: &GeminiCliConfig) -> GoogleTool<'_> {
    GoogleTool {
        client_id: non_empty(&config.client_id, DEFAULT_CLIENT_ID),
        client_secret: non_empty(&config.client_secret, DEFAULT_CLIENT_SECRET),
        token_url: non_empty(&config.token_url, super::DEFAULT_TOKEN_URL),
        redirect_uri: DEFAULT_REDIRECT_URI,
        scope: OAUTH_SCOPE,
        fallback_tier: FALLBACK_TIER,
        user_agent: super::CLI_USER_AGENT,
        metadata,
    }
}

fn non_empty<'a>(configured: &'a str, default: &'a str) -> &'a str {
    match configured.trim() {
        "" => default,
        value => value,
    }
}

impl OAuthAuthorizationCode for GeminiCli {
    fn authorize<'a>(
        &'a self,
        context: LoginContext<'a>,
        request: AuthorizationRequest<'a>,
    ) -> OperationFuture<'a, AuthorizationStart> {
        Box::pin(async move {
            let config = GeminiCliConfig::from_view(context.provider)?;
            Ok(google::authorize(
                &tool(&config),
                &config.authorize_url,
                request,
            ))
        })
    }

    fn exchange<'a>(
        &'a self,
        context: LoginContext<'a>,
        grant: AuthorizationCode<'a>,
    ) -> OperationFuture<'a, OAuthCredential> {
        Box::pin(async move {
            let config = GeminiCliConfig::from_view(context.provider)?;
            google::exchange(
                context.client,
                &tool(&config),
                &base_url(context.provider),
                grant.code,
                grant.redirect_uri,
                grant.code_verifier,
                config.project_id.as_deref(),
            )
            .await
        })
    }
}

impl CredentialRefresh for GeminiCli {
    fn refresh<'a>(
        &'a self,
        context: RefreshContext<'a>,
    ) -> CapabilityFuture<'a, Result<CredentialUpdate, ChannelError>> {
        Box::pin(async move {
            let config = GeminiCliConfig::from_view(context.provider)?;
            google::refresh(
                context.client,
                &tool(&config),
                &base_url(context.provider),
                context.credential.secret,
                config.project_id.as_deref(),
            )
            .await
        })
    }
}
