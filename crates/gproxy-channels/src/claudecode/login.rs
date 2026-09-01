use bytes::Bytes;
use gproxy_channel_api::{
    AuthCodeExchangeCtx, AuthCodeStart, AuthCodeStartCtx, BoxFuture, ChannelError, ChannelLogin,
    SimpleHttp,
};
use serde_json::{Value, json};

use super::{ClaudeCodeChannel, account, auth, profile};

const AUTHORIZE_URL: &str = "https://claude.com/cai/oauth/authorize";
const DEFAULT_REDIRECT_URI: &str = "https://platform.claude.com/oauth/code/callback";

impl ChannelLogin for ClaudeCodeChannel {
    fn authcode_start<'a>(
        &'a self,
        _http: &'a dyn SimpleHttp,
        ctx: AuthCodeStartCtx<'a>,
    ) -> BoxFuture<'a, Result<Option<AuthCodeStart>, ChannelError>> {
        let redirect_uri = if ctx.redirect_uri.trim().is_empty() {
            DEFAULT_REDIRECT_URI
        } else {
            ctx.redirect_uri
        };
        let query = crate::shared::http::form(&[
            ("code", "true"),
            ("client_id", auth::CLIENT_ID),
            ("response_type", "code"),
            ("redirect_uri", redirect_uri),
            ("scope", auth::OAUTH_SCOPE),
            ("code_challenge", ctx.pkce_challenge),
            ("code_challenge_method", "S256"),
            ("state", ctx.state),
        ]);
        let started = AuthCodeStart {
            authorize_url: format!("{AUTHORIZE_URL}?{query}"),
            redirect_uri: redirect_uri.into(),
            extra: Some(json!({ "state": ctx.state })),
        };
        Box::pin(async move { Ok(Some(started)) })
    }

    fn authcode_exchange<'a>(
        &'a self,
        http: &'a dyn SimpleHttp,
        ctx: AuthCodeExchangeCtx<'a>,
    ) -> BoxFuture<'a, Result<Value, ChannelError>> {
        let state = ctx
            .extra
            .and_then(|extra| extra.get("state"))
            .and_then(Value::as_str)
            .unwrap_or_default();
        let body = crate::shared::http::form(&[
            ("grant_type", "authorization_code"),
            ("client_id", auth::CLIENT_ID),
            ("code", ctx.code),
            ("redirect_uri", ctx.redirect_uri),
            ("code_verifier", ctx.verifier),
            ("state", state),
        ]);
        let request = http::Request::post(auth::TOKEN_URL)
            .header(
                http::header::CONTENT_TYPE,
                "application/x-www-form-urlencoded",
            )
            .header("anthropic-version", "2023-06-01")
            .header("anthropic-beta", auth::OAUTH_BETA)
            .header(http::header::USER_AGENT, auth::CLI_USER_AGENT)
            .body(Bytes::from(body));
        let mut request = match request {
            Ok(request) => request,
            Err(error) => {
                return Box::pin(async move { Err(ChannelError::Login(error.to_string())) });
            }
        };
        request
            .extensions_mut()
            .insert(profile::CLIENT_PROFILE.clone());
        Box::pin(async move {
            let response = http.send(request).await?;
            if !response.status().is_success() {
                return Err(ChannelError::Login(format!(
                    "token endpoint returned {}",
                    response.status()
                )));
            }
            let token: Value = serde_json::from_slice(response.body())
                .map_err(|_| ChannelError::Login("invalid token response".into()))?;
            let mut secret = login_secret(&token)?;
            account::enrich(http, &mut secret).await;
            Ok(secret)
        })
    }
}

fn login_secret(token: &Value) -> Result<Value, ChannelError> {
    let access = required(token, "access_token")?;
    let refresh = required(token, "refresh_token")?;
    let expires_in = token
        .get("expires_in")
        .and_then(Value::as_i64)
        .unwrap_or(3_600)
        .max(0);
    let mut secret = json!({
        "access_token": access,
        "refresh_token": refresh,
        "expires_at_ms": unix_now_ms().saturating_add(expires_in.saturating_mul(1_000)),
        "device_id": auth::device_id(token),
    });
    if let Some(scope) = token.get("scope").and_then(Value::as_str) {
        secret["scopes"] = Value::Array(
            scope
                .split_whitespace()
                .map(|value| Value::String(value.into()))
                .collect(),
        );
    }
    Ok(secret)
}

fn required<'a>(token: &'a Value, name: &str) -> Result<&'a str, ChannelError> {
    token
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ChannelError::Login(format!("token response missing {name}")))
}

fn unix_now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system clock is before the Unix epoch")
        .as_millis() as i64
}
