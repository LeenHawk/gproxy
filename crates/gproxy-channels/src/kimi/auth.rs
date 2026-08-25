use bytes::Bytes;
use gproxy_channel_api::{BoxFuture, ChannelError, SimpleHttp};
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderName, HeaderValue};
use serde_json::Value;

const DEFAULT_API_BASE: &str = "https://api.moonshot.cn";
const DEFAULT_CODE_BASE: &str = "https://api.kimi.com/coding/v1";
const DEFAULT_OAUTH_HOST: &str = "https://auth.kimi.com";
const CLIENT_ID: &str = "17e5f671-d194-4dfb-9706-5516cb48c098";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    ApiKey,
    Oauth,
}

pub(super) fn mode(secret: &Value) -> Mode {
    if field(secret, "auth_kind") == Some("oauth")
        || field(secret, "access_token").is_some()
        || field(secret, "refresh_token").is_some()
    {
        Mode::Oauth
    } else {
        Mode::ApiKey
    }
}

pub(super) fn base_url<'a>(settings: &'a Value, secret: &'a Value) -> &'a str {
    field(settings, "base_url")
        .or_else(|| field(secret, "base_url"))
        .unwrap_or(match mode(secret) {
            Mode::ApiKey => DEFAULT_API_BASE,
            Mode::Oauth => DEFAULT_CODE_BASE,
        })
}

pub(super) fn refresh_due(secret: &Value) -> Option<i64> {
    if mode(secret) == Mode::ApiKey {
        return None;
    }
    if field(secret, "access_token").is_none() {
        return Some(i64::MIN);
    }
    secret
        .get("expires_at_ms")
        .and_then(Value::as_i64)
        .map(|expires| expires / 1_000 - 60)
}

pub(super) fn refresh<'a>(
    secret: &'a Value,
    settings: &'a Value,
    http: &'a dyn SimpleHttp,
) -> BoxFuture<'a, Result<Value, ChannelError>> {
    let request = refresh_request(secret, settings);
    let request = match request {
        Ok(request) => request,
        Err(error) => return Box::pin(async move { Err(error) }),
    };
    let send = http.send(request);
    Box::pin(async move {
        let response = send.await?;
        if !response.status().is_success() {
            let snippet: String = String::from_utf8_lossy(response.body())
                .chars()
                .take(256)
                .collect();
            return Err(ChannelError::Refresh(format!(
                "Kimi token endpoint {}: {snippet}",
                response.status()
            )));
        }
        let token: Value = serde_json::from_slice(response.body())
            .map_err(|error| ChannelError::Refresh(format!("Kimi token JSON: {error}")))?;
        rotate(secret, &token)
    })
}

fn refresh_request(secret: &Value, settings: &Value) -> Result<http::Request<Bytes>, ChannelError> {
    let refresh = required(secret, "refresh_token")?;
    let device = required(secret, "device_id")?;
    let host = field(settings, "oauth_host")
        .or_else(|| field(secret, "oauth_host"))
        .unwrap_or(DEFAULT_OAUTH_HOST);
    let uri = crate::shared::http::join(host, "/api/oauth/token", None)?;
    let body = crate::shared::http::form(&[
        ("client_id", CLIENT_ID),
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh),
    ]);
    let mut request = http::Request::post(uri)
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(ACCEPT, "application/json")
        .body(Bytes::from(body))
        .map_err(|error| ChannelError::Refresh(error.to_string()))?;
    super::identity::apply(request.headers_mut(), device)?;
    Ok(request)
}

fn rotate(secret: &Value, token: &Value) -> Result<Value, ChannelError> {
    let access = required(token, "access_token")?;
    let refresh = required(token, "refresh_token")?;
    let expires = token
        .get("expires_in")
        .and_then(Value::as_i64)
        .unwrap_or(3_600)
        .max(0);
    let mut output = secret.clone();
    let root = output
        .as_object_mut()
        .ok_or_else(|| ChannelError::Refresh("Kimi secret is not an object".into()))?;
    root.insert("access_token".into(), Value::String(access.into()));
    root.insert("refresh_token".into(), Value::String(refresh.into()));
    root.insert(
        "expires_at_ms".into(),
        Value::from(unix_now_ms().saturating_add(expires.saturating_mul(1_000))),
    );
    Ok(output)
}

pub(super) fn apply(
    headers: &mut http::HeaderMap,
    secret: &Value,
    anthropic: bool,
    method: &http::Method,
) -> Result<(), ChannelError> {
    match mode(secret) {
        Mode::ApiKey => bearer(headers, required(secret, "api_key")?),
        Mode::Oauth => {
            let token = required(secret, "access_token")?;
            if anthropic {
                insert(headers, HeaderName::from_static("x-api-key"), token)?;
                headers.insert(
                    HeaderName::from_static("anthropic-version"),
                    HeaderValue::from_static("2023-06-01"),
                );
            } else {
                bearer(headers, token)?;
            }
            if method != http::Method::GET && !headers.contains_key(CONTENT_TYPE) {
                headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
            }
            headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
            super::identity::apply(headers, required(secret, "device_id")?)?;
            Ok(())
        }
    }
}

fn bearer(headers: &mut http::HeaderMap, token: &str) -> Result<(), ChannelError> {
    insert(headers, AUTHORIZATION, &format!("Bearer {token}"))
}
fn insert(
    headers: &mut http::HeaderMap,
    name: HeaderName,
    value: &str,
) -> Result<(), ChannelError> {
    headers.insert(
        name,
        HeaderValue::from_str(value)
            .map_err(|error| ChannelError::Secret(format!("Kimi credential: {error}")))?,
    );
    Ok(())
}
fn required<'a>(value: &'a Value, name: &str) -> Result<&'a str, ChannelError> {
    field(value, name).ok_or_else(|| ChannelError::Secret(format!("{name} missing")))
}
fn field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value
        .get(name)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}
fn unix_now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_millis()
        .try_into()
        .expect("Unix milliseconds fit i64")
}
