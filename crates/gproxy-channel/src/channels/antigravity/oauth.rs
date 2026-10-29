//! The Google login and token refresh, wired to Antigravity's own client.
//!
//! Everything here is what the Antigravity IDE's Electron main process does
//! (`resources/app/out/main.js`, IDE 2.6.0): google-auth-library 10.3.0's
//! `OAuth2Client` for the login and refresh, and its `request` for the Code
//! Assist setup calls and the profile.
//!
//! `authorize` builds the consent URL for the host's `state` (the IDE uses
//! no PKCE, so the host's challenge is not sent); `exchange` turns the code
//! into tokens and then learns the Cloud project and the tier, which travel
//! back in `provider_fields` for the host to persist. `refresh` renews the access token, carries the discovered
//! facts forward, and completes a discovery that the login could not finish
//! (v3 `antigravity/{login,auth}.rs`, `shared/{google_login,google_oauth}.rs`).

use super::{
    AUTH_LIBRARY, Antigravity, AntigravityConfig, DEFAULT_CLIENT_ID, DEFAULT_CLIENT_SECRET,
    DEFAULT_REDIRECT_URI, DEFAULT_TOKEN_URL, EDITOR_USER_AGENT, IDE_VERSION, NODE_CLIENT,
    OAUTH_SCOPE, base_url,
};
use crate::channel::{
    AuthorizationCode, AuthorizationRequest, AuthorizationStart, ChannelError, CredentialRefresh,
    CredentialUpdate, LoginContext, OAuthAuthorizationCode, OAuthCredential, OperationFuture,
    RefreshContext,
};
use crate::channels::shared::code_assist::google::{self, GoogleTool, Onboarding};
use gproxy_protocol::capability::CapabilityFuture;
use serde_json::Value;
use std::time::Duration;

/// `ClientMetadata` for `loadCodeAssist`/`onboardUser`: the IDE's
/// `get clientMetadata()` is `{ideName:"antigravity", ideType:ANTIGRAVITY,
/// ideVersion}` and nothing else (no platform, no plugin type, no
/// `duetProject`). protobuf-es writes the fields in field-number order:
/// `ideType` (1), `ideVersion` (2), `ideName` (8), per the
/// `google.internal.cloud.code.v1internal.ClientMetadata` descriptor
/// embedded in `main.js` (`FF=Uo("…onboarding.proto…")`).
fn metadata(_project: Option<&str>) -> String {
    format!(r#"{{"ideType":"ANTIGRAVITY","ideVersion":"{IDE_VERSION}","ideName":"antigravity"}}"#)
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

fn tool(config: &AntigravityConfig) -> GoogleTool<'_> {
    GoogleTool {
        client_id: DEFAULT_CLIENT_ID,
        client_secret: DEFAULT_CLIENT_SECRET,
        token_url: non_empty(&config.token_url, DEFAULT_TOKEN_URL),
        redirect_uri: DEFAULT_REDIRECT_URI,
        scope: OAUTH_SCOPE,
        // The IDE's `getLoginUrl`: `new OAuth2Client(id, secret, redirect)
        // .generateAuthUrl({access_type:"offline", scope, state,
        // prompt:"consent"})`; google-auth-library appends `response_type`,
        // `client_id`, then the constructor's `redirect_uri`. No PKCE: the
        // exchange is `getToken(code)` alone.
        authorize_params: &[
            "access_type",
            "scope",
            "state",
            "prompt",
            "response_type",
            "client_id",
            "redirect_uri",
        ],
        auth_library: AUTH_LIBRARY,
        node_client: NODE_CLIENT,
        // `makePostRequest` goes through `OAuth2Client.request`, whose
        // interceptor adds the runtime banner; it sets no `responseType`, so
        // node-fetch's `Accept: */*` stands.
        code_assist_api_client: Some(NODE_CLIENT),
        code_assist_accept: "*/*",
        // `getProfileData`: the same `request` with only `User-Agent` set.
        userinfo_headers: &[
            ("accept", "*/*"),
            ("user-agent", EDITOR_USER_AGENT),
            ("x-goog-api-client", NODE_CLIENT),
        ],
        onboarding,
        // The IDE's `_ss=500` between polls of the onboarding operation.
        onboard_poll: Duration::from_millis(500),
        user_agent: EDITOR_USER_AGENT,
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
