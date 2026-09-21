//! The Google login and token refresh, wired to Antigravity's own client.
//!
//! `authorize` builds the consent URL for the host's PKCE challenge and
//! state; `exchange` turns the code into tokens and then learns the Cloud
//! project and the tier, which travel back in `provider_fields` for the host
//! to persist. `refresh` renews the access token, carries the discovered
//! facts forward, and completes a discovery that the login could not finish
//! (v3 `antigravity/{login,auth}.rs`, `shared/{google_login,google_oauth}.rs`).

use super::{
    Antigravity, AntigravityConfig, DEFAULT_CLIENT_ID, DEFAULT_CLIENT_SECRET, DEFAULT_REDIRECT_URI,
    DEFAULT_TOKEN_URL, FALLBACK_TIER, OAUTH_SCOPE, base_url,
};
use crate::channel::{
    AuthorizationCode, AuthorizationRequest, AuthorizationStart, ChannelError, CredentialRefresh,
    CredentialUpdate, LoginContext, OAuthAuthorizationCode, OAuthCredential, OperationFuture,
    RefreshContext,
};
use crate::channels::shared::code_assist::google::{self, GoogleTool};
use gproxy_protocol::capability::CapabilityFuture;
use serde_json::{Value, json};

/// `ClientMetadata` for `loadCodeAssist`/`onboardUser`. Antigravity names
/// itself as the IDE, which is how the Code Assist host distinguishes it
/// from the Gemini CLI, and it never sends a `duetProject`
/// (v3 `antigravity/login.rs::metadata`).
fn metadata(_project: Option<&str>) -> Value {
    json!({
        "ideType": "ANTIGRAVITY",
        "platform": "PLATFORM_UNSPECIFIED",
        "pluginType": "GEMINI",
    })
}

fn tool(config: &AntigravityConfig) -> GoogleTool<'_> {
    GoogleTool {
        client_id: non_empty(&config.client_id, DEFAULT_CLIENT_ID),
        client_secret: non_empty(&config.client_secret, DEFAULT_CLIENT_SECRET),
        token_url: non_empty(&config.token_url, DEFAULT_TOKEN_URL),
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

impl OAuthAuthorizationCode for Antigravity {
    fn authorize<'a>(
        &'a self,
        context: LoginContext<'a>,
        request: AuthorizationRequest<'a>,
    ) -> OperationFuture<'a, AuthorizationStart> {
        Box::pin(async move {
            let config = AntigravityConfig::from_view(context.provider)?;
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
            let config = AntigravityConfig::from_view(context.provider)?;
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

impl CredentialRefresh for Antigravity {
    fn refresh<'a>(
        &'a self,
        context: RefreshContext<'a>,
    ) -> CapabilityFuture<'a, Result<CredentialUpdate, ChannelError>> {
        Box::pin(async move {
            let config = AntigravityConfig::from_view(context.provider)?;
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
