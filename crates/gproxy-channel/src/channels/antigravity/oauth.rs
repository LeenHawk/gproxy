//! The Google login and token refresh, wired to Antigravity's own client.
//!
//! `authorize` builds the consent URL for the host's PKCE challenge and
//! state; `exchange` turns the code into tokens and then learns the Cloud
//! project and the tier, which travel back in `provider_fields` for the host
//! to persist. `refresh` renews the access token, carries the discovered
//! facts forward, and completes a discovery that the login could not finish
//! (v3 `antigravity/{login,auth}.rs`, `shared/{google_login,google_oauth}.rs`).

use super::{
    Antigravity, AntigravityConfig, CLI_USER_AGENT, DEFAULT_CLIENT_ID, DEFAULT_CLIENT_SECRET,
    DEFAULT_REDIRECT_URI, DEFAULT_TOKEN_URL, OAUTH_SCOPE, base_url,
};
use crate::channel::{
    AuthorizationCode, AuthorizationRequest, AuthorizationStart, ChannelError, CredentialRefresh,
    CredentialUpdate, LoginContext, OAuthAuthorizationCode, OAuthCredential, OperationFuture,
    RefreshContext,
};
use crate::channels::shared::code_assist::google::{self, GoogleTool, Onboarding};
use gproxy_protocol::capability::CapabilityFuture;
use serde_json::{Value, json};
use std::time::Duration;

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

/// The tier the Antigravity IDE onboards every consumer account on.
const FREE_TIER: &str = "free-tier";

/// The Antigravity IDE (`antigravityAuthMainService.confirmUserForService`):
/// a consumer account is onboarded on the literal `free-tier` when that tier
/// is allowed or carries no ineligibility reason
/// (`const s=!!n||!i; … onboardUser("free-tier",t)`); otherwise it is
/// ineligible, with the free tier's `reasonMessage`.
fn onboarding(loaded: &Value, _hint: Option<&str>) -> Result<Onboarding, ChannelError> {
    let reason = google::ineligible_reason(loaded, FREE_TIER);
    match reason {
        Some(reason) if !google::tier_allowed(loaded, FREE_TIER) => {
            Err(ChannelError::InvalidResponse(format!(
                "Antigravity: account is not eligible: {reason}"
            )))
        }
        _ => Ok(Onboarding::Tier(FREE_TIER.to_owned())),
    }
}

/// The google-auth-library the Antigravity IDE bundles
/// (`resources/app/node_modules/google-auth-library` 10.3.0), whose token
/// calls the login and refresh reproduce.
const AUTH_LIBRARY: &str = "google-api-nodejs-client/10.3.0";
/// The Node inside the IDE's Electron 39.2.3 (the binary carries `v22.21.1`).
const NODE_CLIENT: &str = "gl-node/22.21.1";

fn tool(config: &AntigravityConfig) -> GoogleTool<'_> {
    GoogleTool {
        client_id: DEFAULT_CLIENT_ID,
        client_secret: DEFAULT_CLIENT_SECRET,
        token_url: non_empty(&config.token_url, DEFAULT_TOKEN_URL),
        redirect_uri: DEFAULT_REDIRECT_URI,
        scope: OAUTH_SCOPE,
        // The IDE's `getLoginUrl`: `generateAuthUrl({…, prompt:"consent"})`.
        prompt_consent: true,
        auth_library: AUTH_LIBRARY,
        node_client: NODE_CLIENT,
        code_assist_api_client: None,
        userinfo_headers: &[
            ("accept", "application/json"),
            ("user-agent", CLI_USER_AGENT),
        ],
        onboarding,
        // The IDE's `_ss=500` between polls of the onboarding operation.
        onboard_poll: Duration::from_millis(500),
        user_agent: CLI_USER_AGENT,
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
                None,
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
                None,
            )
            .await
        })
    }
}
