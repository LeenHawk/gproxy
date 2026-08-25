use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use bytes::Bytes;
use gproxy_channel_api::{BoxFuture, ChannelError, SimpleHttp};
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderName, HeaderValue, USER_AGENT};
use serde_json::Value;

const CLIENT_ID: &str = "b1a00492-073a-47ea-816f-4c329264a828";
const TOKEN_URL: &str = "https://auth.x.ai/oauth2/token";
const SKEW_SECONDS: i64 = 60;

pub(super) fn access_token(secret: &Value) -> Result<&str, ChannelError> {
    field(secret, "access_token")
        .ok_or_else(|| ChannelError::Secret("OAuth access_token missing".into()))
}

pub(super) fn refresh_due(secret: &Value) -> Option<i64> {
    if field(secret, "access_token").is_none() {
        return Some(i64::MIN);
    }
    secret
        .get("expires_at_ms")
        .and_then(Value::as_i64)
        .filter(|expires| *expires != 0)
        .map(|expires| expires / 1_000 - SKEW_SECONDS)
}

pub(super) fn refresh<'a>(
    secret: &'a Value,
    settings: &'a Value,
    http: &'a dyn SimpleHttp,
) -> BoxFuture<'a, Result<Value, ChannelError>> {
    let request = match build_refresh(secret, settings) {
        Ok(request) => request,
        Err(error) => return Box::pin(async move { Err(error) }),
    };
    let send = http.send(request);
    Box::pin(async move {
        let response = send.await?;
        if !response.status().is_success() {
            return Err(ChannelError::Refresh(format!(
                "xAI token endpoint returned {}",
                response.status()
            )));
        }
        let token: Value = serde_json::from_slice(response.body())
            .map_err(|error| ChannelError::Refresh(format!("token response JSON: {error}")))?;
        rotate(secret, &token)
    })
}

fn build_refresh(secret: &Value, settings: &Value) -> Result<http::Request<Bytes>, ChannelError> {
    let refresh = field(secret, "refresh_token")
        .ok_or_else(|| ChannelError::Secret("refresh_token missing".into()))?;
    let body = crate::shared::http::form(&[
        ("grant_type", "refresh_token"),
        (
            "client_id",
            field(settings, "oauth_client_id").unwrap_or(CLIENT_ID),
        ),
        ("refresh_token", refresh),
    ]);
    let url = field(settings, "oauth_token_url")
        .or_else(|| field(secret, "token_endpoint"))
        .unwrap_or(TOKEN_URL);
    http::Request::post(url)
        .header(CONTENT_TYPE, "application/x-www-form-urlencoded")
        .header(ACCEPT, "application/json")
        .body(Bytes::from(body))
        .map_err(|error| ChannelError::Refresh(error.to_string()))
}

fn rotate(secret: &Value, token: &Value) -> Result<Value, ChannelError> {
    let access = field(token, "access_token")
        .ok_or_else(|| ChannelError::Refresh("access_token missing".into()))?;
    let expires = token
        .get("expires_in")
        .and_then(Value::as_i64)
        .ok_or_else(|| ChannelError::Refresh("token response missing expires_in".into()))?;
    let mut output = secret.clone();
    let object = output
        .as_object_mut()
        .ok_or_else(|| ChannelError::Refresh("secret must be an object".into()))?;
    object.insert("access_token".into(), Value::String(access.into()));
    if let Some(refresh) = field(token, "refresh_token") {
        object.insert("refresh_token".into(), Value::String(refresh.into()));
    }
    if let Some(id_token) = field(token, "id_token") {
        if let Some(subject) = jwt_claim(id_token, "sub") {
            object.insert("sub".into(), Value::String(subject));
        }
        if let Some(email) = jwt_claim(id_token, "email") {
            object.insert("user_email".into(), Value::String(email));
        }
        object.insert("id_token".into(), Value::String(id_token.into()));
    }
    object.insert(
        "expires_at_ms".into(),
        Value::from(
            now_seconds()
                .saturating_add(expires.max(0))
                .saturating_mul(1_000),
        ),
    );
    Ok(output)
}

pub(super) fn apply(
    headers: &mut http::HeaderMap,
    secret: &Value,
    stream: bool,
    audio: bool,
    session: Option<&str>,
) -> Result<(), ChannelError> {
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {}", access_token(secret)?))
            .map_err(|error| ChannelError::Secret(error.to_string()))?,
    );
    if !headers.contains_key(CONTENT_TYPE) {
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    }
    headers.insert(
        ACCEPT,
        HeaderValue::from_static(if audio {
            "audio/*"
        } else if stream {
            "text/event-stream"
        } else {
            "application/json"
        }),
    );
    for (name, value) in [
        ("x-xai-token-auth", "xai-grok-cli"),
        ("x-authenticateresponse", "authenticate-response"),
        ("x-grok-client-version", "1.0.0"),
        ("x-grok-client-identifier", "grok-shell"),
        ("x-grok-client-mode", "headless"),
    ] {
        headers.insert(
            HeaderName::from_static(name),
            HeaderValue::from_static(value),
        );
    }
    headers.insert(USER_AGENT, HeaderValue::from_static("grok-shell/1.0.0"));
    if let Some(session) = session {
        headers.insert(
            HeaderName::from_static("x-grok-conv-id"),
            HeaderValue::from_str(session)
                .map_err(|error| ChannelError::Prepare(error.to_string()))?,
        );
    }
    if let Some(user) = field(secret, "sub") {
        headers.insert(
            HeaderName::from_static("x-grok-user-id"),
            HeaderValue::from_str(user)
                .map_err(|error| ChannelError::Prepare(error.to_string()))?,
        );
    }
    Ok(())
}

pub(super) fn field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn now_seconds() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_secs()
        .try_into()
        .expect("Unix seconds fit i64")
}

fn jwt_claim(token: &str, name: &str) -> Option<String> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice::<Value>(&bytes)
        .ok()?
        .get(name)?
        .as_str()
        .map(str::to_owned)
}
