use gproxy_channel_api::{
    AuthCodeExchangeCtx, AuthCodeStart, AuthCodeStartCtx, BoxFuture, ChannelError, ChannelLogin,
    CredentialAcquisition, SimpleHttp,
};
use serde_json::{Value, json};

use super::GeminiCliChannel;

const CODE_REDIRECT: &str = "https://codeassist.google.com/authcode";
const LOOPBACK_REDIRECT: &str = "http://127.0.0.1:1455/oauth2callback";
const CONFIG: crate::shared::google_login::GoogleLogin = crate::shared::google_login::GoogleLogin {
    client_id: "681255809395-oo8ft2oprdrnp9e3aqf6av3hmdib135j.apps.googleusercontent.com",
    client_secret: "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",
    authorize_url: "https://accounts.google.com/o/oauth2/v2/auth",
    token_url: "https://oauth2.googleapis.com/token",
    redirect_uri: CODE_REDIRECT,
    scope: "https://www.googleapis.com/auth/cloud-platform https://www.googleapis.com/auth/userinfo.email https://www.googleapis.com/auth/userinfo.profile",
    code_assist_base: "https://cloudcode-pa.googleapis.com",
    fallback_tier: "legacy-tier",
    user_agent: "GeminiCLI-tui/0.55.1 (linux; x64; terminal) google-api-nodejs-client/10.9.0",
    metadata,
    profile: &super::profile::PROFILE,
};

impl ChannelLogin for GeminiCliChannel {
    fn authcode_start<'a>(
        &'a self,
        _http: &'a dyn SimpleHttp,
        ctx: AuthCodeStartCtx<'a>,
    ) -> BoxFuture<'a, Result<Option<AuthCodeStart>, ChannelError>> {
        let redirect = if ctx.redirect_uri.trim().is_empty()
            && ctx.params.get("code_only").and_then(Value::as_str) == Some("false")
        {
            LOOPBACK_REDIRECT
        } else {
            ctx.redirect_uri
        };
        let started = crate::shared::google_login::start(
            &CONFIG,
            redirect,
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

fn metadata(project: Option<&str>) -> Value {
    let mut metadata = json!({
        "ideType":"IDE_UNSPECIFIED",
        "platform":"PLATFORM_UNSPECIFIED",
        "pluginType":"GEMINI",
    });
    if let Some(project) = project {
        metadata["duetProject"] = Value::String(project.into());
    }
    metadata
}
