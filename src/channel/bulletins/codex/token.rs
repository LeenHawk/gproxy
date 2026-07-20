//! Codex credential parsing, OpenID token metadata, expiry, and refresh.

use std::sync::Arc;

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD as B64URL;
use serde_json::{Value, json};

use super::auth::OAUTH_CLIENT_ID;
use crate::channel::ChannelError;
use crate::channel::oauth::{self, TokenResponse};
use crate::http::client::UpstreamClient;

const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const EXPIRY_SKEW_MS: i64 = 60_000;

fn secret_str<'a>(secret: &'a Value, key: &str) -> Option<&'a str> {
    secret
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

pub(super) fn access_token(secret: &Value) -> Result<&str, ChannelError> {
    secret_str(secret, "access_token")
        .ok_or_else(|| ChannelError::InvalidCredential("missing access_token".into()))
}

pub(super) fn account_id(secret: &Value) -> Option<&str> {
    secret_str(secret, "account_id")
}

pub(super) fn secret_from_login(response: TokenResponse) -> Result<Value, ChannelError> {
    let access_token = response
        .access_token
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ChannelError::Build("token response missing access_token".into()))?;
    let expires_at_ms = expires_at_ms(response.expires_in);
    let mut secret = json!({
        "access_token": access_token,
        "expires_at_ms": expires_at_ms,
    });
    if let Some(refresh_token) = response.refresh_token.filter(|value| !value.is_empty()) {
        secret["refresh_token"] = Value::String(refresh_token);
    }
    if let Some(id_token) = response.id_token.filter(|value| !value.is_empty()) {
        if let Some(account_id) = account_id_from_id_token(&id_token) {
            secret["account_id"] = Value::String(account_id);
        }
        if let Some(email) = email_from_id_token(&id_token) {
            secret["user_email"] = Value::String(email);
        }
        secret["id_token"] = Value::String(id_token);
    }
    Ok(secret)
}

pub(super) fn needs_refresh(secret: &Value) -> bool {
    if secret_str(secret, "access_token").is_none() {
        return true;
    }
    let expires_at_ms = secret
        .get("expires_at_ms")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    if expires_at_ms == 0 {
        return false;
    }
    let now_ms = crate::util::time::unix_now().saturating_mul(1000);
    now_ms > expires_at_ms - EXPIRY_SKEW_MS
}

pub(super) async fn refresh(
    client: &Arc<dyn UpstreamClient>,
    secret: &Value,
) -> Result<Value, ChannelError> {
    let refresh_token = secret_str(secret, "refresh_token")
        .ok_or_else(|| ChannelError::InvalidCredential("missing refresh_token".into()))?;
    let form = [
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("client_id", OAUTH_CLIENT_ID),
    ];
    let response = oauth::token_post(client, TOKEN_URL, &form, &[]).await?;
    let new_access = response
        .access_token
        .filter(|value| !value.is_empty())
        .ok_or_else(|| ChannelError::Build("refresh response missing access_token".into()))?;
    let expires_at_ms = expires_at_ms(response.expires_in);

    let mut output = secret.clone();
    let object = output
        .as_object_mut()
        .ok_or_else(|| ChannelError::Build("secret is not an object".into()))?;
    object.insert("access_token".into(), Value::String(new_access));
    if let Some(refresh_token) = response.refresh_token.filter(|value| !value.is_empty()) {
        object.insert("refresh_token".into(), Value::String(refresh_token));
    }
    if let Some(id_token) = response.id_token.filter(|value| !value.is_empty()) {
        if let Some(account_id) = account_id_from_id_token(&id_token) {
            object.insert("account_id".into(), Value::String(account_id));
        }
        object.insert("id_token".into(), Value::String(id_token));
    }
    object.insert("expires_at_ms".into(), Value::Number(expires_at_ms.into()));
    Ok(output)
}

fn expires_at_ms(expires_in: Option<u64>) -> i64 {
    crate::util::time::unix_now().saturating_mul(1000) + expires_in.unwrap_or(3600) as i64 * 1000
}

pub(super) fn account_id_from_id_token(id_token: &str) -> Option<String> {
    let payload = id_token_payload(id_token)?;
    payload
        .get("https://api.openai.com/auth")
        .and_then(|auth| auth.get("chatgpt_account_id"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn email_from_id_token(id_token: &str) -> Option<String> {
    let payload = id_token_payload(id_token)?;
    payload
        .get("email")
        .or_else(|| {
            payload
                .get("https://api.openai.com/profile")
                .and_then(|profile| profile.get("email"))
        })
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn id_token_payload(id_token: &str) -> Option<Value> {
    let encoded = id_token.split('.').nth(1)?;
    let bytes = B64URL.decode(encoded).ok()?;
    serde_json::from_slice(&bytes).ok()
}
