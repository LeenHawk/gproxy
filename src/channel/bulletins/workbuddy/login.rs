use std::sync::Arc;

use bytes::Bytes;
use http::Request;
use http::header::{ACCEPT, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::{Value, json};

use super::auth;
use crate::channel::{ChannelError, DeviceInit, DevicePoll};
use crate::http::client::UpstreamClient;

const PLATFORM: &str = "workbuddy";
const EXPIRY_SKEW_MS: i64 = 300_000;

#[derive(Deserialize)]
struct Envelope<T> {
    #[serde(default)]
    code: i64,
    #[serde(default)]
    msg: String,
    data: Option<T>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthState {
    state: String,
    auth_url: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Tokens {
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    expires_in: Option<i64>,
    expires_at: Option<i64>,
    refresh_expires_in: Option<i64>,
    refresh_expires_at: Option<i64>,
    domain: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Account {
    uid: String,
    nickname: Option<String>,
    #[serde(rename = "type")]
    account_type: Option<String>,
    enterprise_id: Option<String>,
    enterprise_name: Option<String>,
    department_full_name: Option<String>,
}

pub(super) async fn device_start(
    client: &Arc<dyn UpstreamClient>,
    settings: &Value,
) -> Result<DeviceInit, ChannelError> {
    let url = format!(
        "{}/v2/plugin/auth/state?platform={PLATFORM}",
        auth::base_url(settings)
    );
    let envelope: Envelope<AuthState> = send_json(
        client,
        json_request(http::Method::POST, &url, Bytes::from_static(b"{}"))?,
        "auth state",
    )
    .await?;
    let state = envelope_data(envelope, "auth state")?;
    Ok(DeviceInit {
        device_code: state.state,
        user_code: "No code required".into(),
        verification_url: state.auth_url,
        interval_secs: 1,
    })
}

pub(super) async fn device_poll(
    client: &Arc<dyn UpstreamClient>,
    settings: &Value,
    state: &str,
) -> Result<DevicePoll, ChannelError> {
    let token_url = format!(
        "{}/v2/plugin/auth/token?state={}",
        auth::base_url(settings),
        crate::channel::oauth::percent_encode(state)
    );
    let token_envelope: Envelope<Tokens> = send_json(
        client,
        json_request(http::Method::GET, &token_url, Bytes::new())?,
        "auth token",
    )
    .await?;
    if token_envelope.code == 11217 {
        return Ok(DevicePoll::Pending);
    }
    let tokens = envelope_data(token_envelope, "auth token")?;

    let account_url = format!(
        "{}/v2/plugin/login/account?state={}",
        auth::base_url(settings),
        crate::channel::oauth::percent_encode(state)
    );
    let mut request = json_request(http::Method::GET, &account_url, Bytes::new())?;
    auth::insert_named(
        &mut request,
        "authorization",
        &format!("Bearer {}", tokens.access_token),
    )?;
    if let Some(domain) = tokens.domain.as_deref() {
        auth::insert_named(&mut request, "x-domain", domain)?;
    }
    let account_envelope: Envelope<Account> = send_json(client, request, "login account").await?;
    if account_envelope.code == 12151 {
        return Ok(DevicePoll::Pending);
    }
    let account = envelope_data(account_envelope, "login account")?;
    Ok(DevicePoll::Ready(secret_from(
        tokens,
        account,
        &Value::Null,
    )))
}

pub(super) async fn refresh(
    client: &Arc<dyn UpstreamClient>,
    secret: &Value,
    settings: &Value,
) -> Result<Value, ChannelError> {
    let url = format!("{}/v2/plugin/auth/token/refresh", auth::base_url(settings));
    let mut request = json_request(http::Method::POST, &url, Bytes::from_static(b"{}"))?;
    auth::apply_refresh(&mut request, secret)?;
    let envelope: Envelope<Tokens> = send_json(client, request, "token refresh").await?;
    let tokens = envelope_data(envelope, "token refresh")?;
    let account = Account {
        uid: auth::field(secret, "user_id")
            .unwrap_or_default()
            .to_string(),
        nickname: string_field(secret, "nickname"),
        account_type: string_field(secret, "account_type"),
        enterprise_id: string_field(secret, "enterprise_id"),
        enterprise_name: string_field(secret, "enterprise_name"),
        department_full_name: string_field(secret, "department_full_name"),
    };
    Ok(secret_from(tokens, account, secret))
}

pub(super) fn needs_refresh(secret: &Value) -> bool {
    if auth::field(secret, "refresh_token").is_none() {
        return false;
    }
    let expires_at = secret
        .get("expires_at_ms")
        .and_then(Value::as_i64)
        .unwrap_or_default();
    crate::util::time::unix_now().saturating_mul(1000) > expires_at - EXPIRY_SKEW_MS
}

fn secret_from(tokens: Tokens, account: Account, previous: &Value) -> Value {
    let now = crate::util::time::unix_now().saturating_mul(1000);
    let expires_at = tokens
        .expires_at
        .unwrap_or_else(|| now.saturating_add(tokens.expires_in.unwrap_or(3600) * 1000));
    let refresh_expires_at = tokens
        .refresh_expires_at
        .unwrap_or_else(|| now.saturating_add(tokens.refresh_expires_in.unwrap_or(0) * 1000));
    let mut out = previous.clone();
    if !out.is_object() {
        out = json!({});
    }
    let obj = out.as_object_mut().expect("object");
    obj.insert("access_token".into(), Value::String(tokens.access_token));
    if !tokens.refresh_token.is_empty() {
        obj.insert("refresh_token".into(), Value::String(tokens.refresh_token));
    }
    obj.insert("expires_at_ms".into(), json!(expires_at));
    obj.insert("refresh_expires_at_ms".into(), json!(refresh_expires_at));
    if let Some(domain) = tokens.domain.filter(|value| !value.is_empty()) {
        obj.insert("domain".into(), Value::String(domain));
    }
    insert_optional(obj, "user_id", Some(account.uid));
    insert_optional(obj, "nickname", account.nickname);
    insert_optional(obj, "account_type", account.account_type);
    insert_optional(obj, "enterprise_id", account.enterprise_id);
    insert_optional(obj, "enterprise_name", account.enterprise_name);
    insert_optional(obj, "department_full_name", account.department_full_name);
    out
}

fn insert_optional(obj: &mut serde_json::Map<String, Value>, key: &str, value: Option<String>) {
    if let Some(value) = value.filter(|value| !value.is_empty()) {
        obj.insert(key.into(), Value::String(value));
    }
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    auth::field(value, key).map(str::to_string)
}

fn json_request(
    method: http::Method,
    url: &str,
    body: Bytes,
) -> Result<Request<Bytes>, ChannelError> {
    Request::builder()
        .method(method)
        .uri(url)
        .header(ACCEPT, "application/json")
        .header(CONTENT_TYPE, "application/json")
        .header("x-product", "SaaS")
        .body(body)
        .map_err(|error| ChannelError::Build(format!("login request build: {error}")))
}

async fn send_json<T: serde::de::DeserializeOwned>(
    client: &Arc<dyn UpstreamClient>,
    request: Request<Bytes>,
    what: &str,
) -> Result<T, ChannelError> {
    let response = client
        .send(request)
        .await
        .map_err(|error| ChannelError::Build(format!("{what} request failed: {error}")))?;
    serde_json::from_slice(&response.into_body())
        .map_err(|error| ChannelError::Build(format!("{what} response parse: {error}")))
}

fn envelope_data<T>(envelope: Envelope<T>, what: &str) -> Result<T, ChannelError> {
    if envelope.code != 0 {
        return Err(ChannelError::Build(format!(
            "{what}: {} ({})",
            envelope.msg, envelope.code
        )));
    }
    envelope
        .data
        .ok_or_else(|| ChannelError::Build(format!("{what}: missing data")))
}
