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
use crate::channels::shared::code_assist::google::{self, GoogleTool, Onboarding};
use gproxy_protocol::capability::CapabilityFuture;
use serde_json::Value;
use std::time::Duration;

/// `ClientMetadata` for `loadCodeAssist`/`onboardUser`: the CLI reports no
/// IDE and no platform, and identifies its plugin as `GEMINI`; a project
/// hint is repeated as `duetProject` (v3 `geminicli/login.rs::metadata`).
/// Key order is `setup.ts`'s: `{...coreClientMetadata, duetProject}`.
fn metadata(project: Option<&str>) -> String {
    let mut metadata = String::from(
        r#"{"ideType":"IDE_UNSPECIFIED","platform":"PLATFORM_UNSPECIFIED","pluginType":"GEMINI""#,
    );
    if let Some(project) = project {
        metadata.push_str(&format!(
            r#","duetProject":{}"#,
            Value::String(project.into())
        ));
    }
    metadata.push('}');
    metadata
}

/// gemini-cli `setup.ts::_doSetupUser`: an account that already has a tier
/// (`currentTier`) is never onboarded again; without a project it needs the
/// hint, or it is refused with the server's ineligibility reasons. Only an
/// account with no tier is onboarded, on the default allowed tier, else
/// `legacy-tier`.
fn onboarding(loaded: &Value, hint: Option<&str>) -> Result<Onboarding, ChannelError> {
    if loaded
        .get("currentTier")
        .is_some_and(|tier| !tier.is_null())
    {
        return match hint {
            Some(project) => Ok(Onboarding::Project(project.to_owned())),
            None => Err(google::project_required(loaded)),
        };
    }
    Ok(Onboarding::Tier(
        google::default_tier(loaded)
            .unwrap_or(FALLBACK_TIER)
            .to_owned(),
    ))
}

/// The google-auth-library the CLI pins (`packages/core/package.json`
/// `"google-auth-library": "10.9.0"`), as its token calls name it.
const AUTH_LIBRARY: &str = "google-api-nodejs-client/10.9.0";

/// `fetchAndCacheUserInfo` is a bare global `fetch` with only
/// `Authorization`; these are what Node's undici adds to it. Its
/// `accept-encoding: gzip, deflate` is left to the connection, which decodes
/// nothing by default.
const USERINFO_HEADERS: &[(&str, &str)] = &[
    ("accept", "*/*"),
    ("accept-language", "*"),
    ("sec-fetch-mode", "cors"),
    ("user-agent", "node"),
];

fn tool(config: &GeminiCliConfig) -> GoogleTool<'_> {
    GoogleTool {
        client_id: DEFAULT_CLIENT_ID,
        client_secret: DEFAULT_CLIENT_SECRET,
        token_url: non_empty(&config.token_url, super::DEFAULT_TOKEN_URL),
        redirect_uri: DEFAULT_REDIRECT_URI,
        scope: OAUTH_SCOPE,
        // `authWithUserCode` (`oauth2.ts`): `generateAuthUrl({redirect_uri,
        // access_type, scope, code_challenge_method, code_challenge, state})`,
        // to which google-auth-library appends `response_type`, `client_id`.
        authorize_params: &[
            "redirect_uri",
            "access_type",
            "scope",
            "code_challenge_method",
            "code_challenge",
            "state",
            "response_type",
            "client_id",
        ],
        auth_library: AUTH_LIBRARY,
        node_client: super::GOOG_API_CLIENT,
        // `setupUser` goes through `OAuth2Client.request`, whose interceptor
        // adds the runtime banner (`authclient.js`).
        code_assist_api_client: Some(super::GOOG_API_CLIENT),
        // `CodeAssistServer.requestPost` sets `responseType: 'json'`.
        code_assist_accept: "application/json",
        userinfo_headers: USERINFO_HEADERS,
        onboarding,
        // gemini-cli `setup.ts`: `setTimeout(f, 5000)` between polls.
        onboard_poll: Duration::from_secs(5),
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
                None,
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
                None,
            )
            .await
        })
    }
}
