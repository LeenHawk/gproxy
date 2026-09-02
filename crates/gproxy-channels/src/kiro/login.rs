use gproxy_channel_api::{
    AuthCodeExchangeCtx, AuthCodeStart, AuthCodeStartCtx, BoxFuture, ChannelError, ChannelLogin,
    CredentialAcquisition, DeviceInit, DevicePoll, DevicePollCtx, DeviceStartCtx, SimpleHttp,
};
use serde::Deserialize;
use serde_json::{Value, json};

use super::KiroChannel;

const AUTH_BASE: &str = "https://prod.us-east-1.auth.desktop.kiro.dev";
const BUILDER_START_URL: &str = "https://view.awsapps.com/start";
const BUILDER_REGION: &str = "us-east-1";
const REDIRECT_URI: &str = "http://127.0.0.1:1455/oauth/callback";
const SCOPES: &str = "codewhisperer:completions codewhisperer:analysis codewhisperer:conversations";

impl ChannelLogin for KiroChannel {
    fn device_start<'a>(
        &'a self,
        http: &'a dyn SimpleHttp,
        ctx: DeviceStartCtx<'a>,
    ) -> BoxFuture<'a, Result<DeviceInit, ChannelError>> {
        Box::pin(async move {
            let base = auth_base(ctx.provider_settings);
            let provider = match crate::shared::login::field(ctx.params, "login_provider") {
                Some("google") => "Google",
                _ => "Github",
            };
            let mut request = crate::shared::login::json_request(
                http::Method::POST,
                &format!("{base}/oauth/device/authorization"),
                &json!({ "clientId":"Kiro-CLI", "loginProvider":provider }),
            )?;
            request
                .extensions_mut()
                .insert(super::profile::CLIENT_PROFILE.clone());
            let started: DeviceCode =
                crate::shared::login::send_json(http, request, "Kiro device code").await?;
            Ok(DeviceInit {
                device_code: started.device_code,
                user_code: started.user_code,
                verification_uri: started
                    .verification_uri_complete
                    .or(started.verification_uri)
                    .ok_or_else(|| {
                        ChannelError::Login("Kiro device code missing verificationUri".into())
                    })?,
                interval_secs: started
                    .interval_in_milliseconds
                    .map(|milliseconds| (milliseconds / 1_000).max(1))
                    .unwrap_or(5),
            })
        })
    }

    fn device_poll<'a>(
        &'a self,
        http: &'a dyn SimpleHttp,
        ctx: DevicePollCtx<'a>,
    ) -> BoxFuture<'a, Result<DevicePoll, ChannelError>> {
        Box::pin(async move {
            let mut request = crate::shared::login::json_request(
                http::Method::POST,
                &format!("{}/oauth/device/poll", auth_base(ctx.provider_settings)),
                &json!({ "deviceCode":ctx.device_code, "clientId":"Kiro-CLI" }),
            )?;
            request
                .extensions_mut()
                .insert(super::profile::CLIENT_PROFILE.clone());
            let reply: DeviceReply =
                crate::shared::login::send_json(http, request, "Kiro device poll").await?;
            match reply.status.as_deref() {
                Some("authorization_pending") | Some("slow_down") => Ok(DevicePoll::Pending),
                Some("authorized") => {
                    let access = required(reply.access_token, "accessToken")?;
                    let refresh = required(reply.refresh_token, "refreshToken")?;
                    let mut secret = json!({
                        "access_token":access,
                        "refresh_token":refresh,
                        "provider":reply.identity_provider.unwrap_or_else(|| "Github".into()),
                        "expires_at_ms":crate::shared::login::now_ms().saturating_add(3_600_000),
                    });
                    if let Some(profile) = reply.profile_arn.filter(|value| !value.is_empty()) {
                        secret["profile_arn"] = Value::String(profile);
                    }
                    Ok(DevicePoll::Ready(CredentialAcquisition::oauth(secret)))
                }
                _ => Ok(DevicePoll::Denied),
            }
        })
    }

    fn authcode_start<'a>(
        &'a self,
        http: &'a dyn SimpleHttp,
        ctx: AuthCodeStartCtx<'a>,
    ) -> BoxFuture<'a, Result<Option<AuthCodeStart>, ChannelError>> {
        Box::pin(async move {
            let (start_url, region) = target(ctx.params)?;
            let redirect_uri = if ctx.redirect_uri.trim().is_empty() {
                REDIRECT_URI
            } else {
                ctx.redirect_uri
            };
            let base = oidc_base(&region)?;
            let mut request = crate::shared::login::json_request(
                http::Method::POST,
                &format!("{base}/client/register"),
                &json!({
                    "clientName":"Kiro-CLI",
                    "clientType":"public",
                    "scopes":SCOPES.split(' ').collect::<Vec<_>>(),
                    "grantTypes":["authorization_code", "refresh_token"],
                    "redirectUris":[redirect_uri],
                    "issuerUrl":start_url,
                }),
            )?;
            request
                .extensions_mut()
                .insert(super::profile::CLIENT_PROFILE.clone());
            let registered: Registered =
                crate::shared::login::send_json(http, request, "Kiro client registration").await?;
            let client_id = required(registered.client_id, "clientId")?;
            let client_secret = required(registered.client_secret, "clientSecret")?;
            let query = crate::shared::http::form(&[
                ("response_type", "code"),
                ("client_id", &client_id),
                ("redirect_uri", redirect_uri),
                ("scopes", SCOPES),
                ("state", ctx.state),
                ("code_challenge", ctx.pkce_challenge),
                ("code_challenge_method", "S256"),
            ]);
            Ok(Some(AuthCodeStart {
                authorize_url: format!("{base}/authorize?{query}"),
                redirect_uri: redirect_uri.into(),
                extra: Some(json!({
                    "client_id":client_id,
                    "client_secret":client_secret,
                    "region":region,
                    "start_url":start_url,
                })),
            }))
        })
    }

    fn authcode_exchange<'a>(
        &'a self,
        http: &'a dyn SimpleHttp,
        ctx: AuthCodeExchangeCtx<'a>,
    ) -> BoxFuture<'a, Result<CredentialAcquisition, ChannelError>> {
        Box::pin(async move {
            let extra = ctx
                .extra
                .ok_or_else(|| ChannelError::Login("Kiro login session missing state".into()))?;
            let client_id = required_field(extra, "client_id")?;
            let client_secret = required_field(extra, "client_secret")?;
            let region = required_field(extra, "region")?;
            let mut request = crate::shared::login::json_request(
                http::Method::POST,
                &format!("{}/token", oidc_base(region)?),
                &json!({
                    "grantType":"authorization_code",
                    "clientId":client_id,
                    "clientSecret":client_secret,
                    "code":ctx.code,
                    "redirectUri":ctx.redirect_uri,
                    "codeVerifier":ctx.verifier,
                }),
            )?;
            request
                .extensions_mut()
                .insert(super::profile::CLIENT_PROFILE.clone());
            let token: TokenReply =
                crate::shared::login::send_json(http, request, "Kiro token").await?;
            Ok(CredentialAcquisition::oauth(json!({
                "access_token":required(token.access_token, "accessToken")?,
                "refresh_token":required(token.refresh_token, "refreshToken")?,
                "expires_at_ms":crate::shared::login::now_ms().saturating_add(
                    i64::try_from(token.expires_in.unwrap_or(3_600)).unwrap_or(i64::MAX)
                        .saturating_mul(1_000),
                ),
                "client_id":client_id,
                "client_secret":client_secret,
                "region":region,
                "start_url":required_field(extra, "start_url")?,
            })))
        })
    }
}

fn auth_base(settings: &Value) -> String {
    crate::shared::login::field(settings, "auth_base_url")
        .unwrap_or(AUTH_BASE)
        .trim_end_matches('/')
        .into()
}

fn target(params: &Value) -> Result<(String, String), ChannelError> {
    if crate::shared::login::field(params, "auth_method") == Some("idc") {
        return Ok((
            required_field(params, "start_url")?.into(),
            required_field(params, "region")?.into(),
        ));
    }
    Ok((BUILDER_START_URL.into(), BUILDER_REGION.into()))
}

fn oidc_base(region: &str) -> Result<String, ChannelError> {
    super::endpoint::validate_region(region)?;
    Ok(format!("https://oidc.{region}.amazonaws.com"))
}

fn required(value: Option<String>, name: &str) -> Result<String, ChannelError> {
    value
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| ChannelError::Login(format!("Kiro response missing {name}")))
}

fn required_field<'a>(value: &'a Value, name: &str) -> Result<&'a str, ChannelError> {
    crate::shared::login::field(value, name)
        .ok_or_else(|| ChannelError::Login(format!("Kiro login missing {name}")))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceCode {
    device_code: String,
    user_code: String,
    verification_uri_complete: Option<String>,
    verification_uri: Option<String>,
    interval_in_milliseconds: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeviceReply {
    status: Option<String>,
    access_token: Option<String>,
    refresh_token: Option<String>,
    profile_arn: Option<String>,
    identity_provider: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Registered {
    client_id: Option<String>,
    client_secret: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TokenReply {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}
