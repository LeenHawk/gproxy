use gproxy_channel_api::{
    AuthCodeExchangeCtx, AuthCodeStart, AuthCodeStartCtx, BoxFuture, ChannelError, ChannelLogin,
    CredentialAcquisition, SimpleHttp,
};
use serde_json::{Value, json};

use super::AntigravityChannel;

const CONFIG: crate::shared::google_login::GoogleLogin = crate::shared::google_login::GoogleLogin {
    client_id: "1071006060591-tmhssin2h21lcre235vtolojh4g403ep.apps.googleusercontent.com",
    client_secret: "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",
    authorize_url: "https://accounts.google.com/o/oauth2/v2/auth",
    token_url: "https://oauth2.googleapis.com/token",
    redirect_uri: "http://localhost:51121/oauth-callback",
    scope: "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile https://www.googleapis.com/auth/cclog https://www.googleapis.com/auth/experimentsandconfigs https://www.googleapis.com/auth/aicode",
    code_assist_base: "https://daily-cloudcode-pa.googleapis.com",
    fallback_tier: "LEGACY",
    user_agent: "antigravity/cli/1.0.6 linux/amd64",
    metadata,
    profile: &super::profile::PROFILE,
};

impl ChannelLogin for AntigravityChannel {
    fn authcode_start<'a>(
        &'a self,
        _http: &'a dyn SimpleHttp,
        ctx: AuthCodeStartCtx<'a>,
    ) -> BoxFuture<'a, Result<Option<AuthCodeStart>, ChannelError>> {
        let started = crate::shared::google_login::start(
            &CONFIG,
            ctx.redirect_uri,
            ctx.state,
            ctx.pkce_challenge,
            ctx.params,
        );
        Box::pin(async move { Ok(Some(started)) })
    }

    fn authcode_exchange<'a>(
        &'a self,
        http: &'a dyn SimpleHttp,
        ctx: AuthCodeExchangeCtx<'a>,
    ) -> BoxFuture<'a, Result<CredentialAcquisition, ChannelError>> {
        Box::pin(async move {
            crate::shared::google_login::exchange(
                http,
                &CONFIG,
                ctx.code,
                ctx.verifier,
                ctx.redirect_uri,
                ctx.extra,
            )
            .await
            .map(CredentialAcquisition::oauth)
        })
    }
}

fn metadata(_: Option<&str>) -> Value {
    json!({
        "ideType":"ANTIGRAVITY",
        "platform":"PLATFORM_UNSPECIFIED",
        "pluginType":"GEMINI",
    })
}
