use bytes::Bytes;
use gproxy_channel_api::{BoxFuture, ChannelError, SimpleHttp};
use http::header::{ACCEPT, CONTENT_TYPE};
use serde::Deserialize;
use serde_json::{Value, json};

const CONSOLE_URL: &str = "https://console.opencode.ai";
const CLIENT_ID: &str = "opencode-cli";

pub(super) fn api_key(secret: &Value) -> Result<&str, ChannelError> {
    field(secret, "api_key")
        .or_else(|| field(secret, "access_token"))
        .ok_or_else(|| ChannelError::Secret("api_key missing".into()))
}

pub(super) fn refresh_due(secret: &Value) -> Option<i64> {
    field(secret, "refresh_token")?;
    if field(secret, "access_token").is_none() {
        return Some(i64::MIN);
    }
    if secret.get("expiry_unknown").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    Some(
        secret
            .get("expires_at_ms")
            .and_then(Value::as_i64)
            .map(|expires| expires / 1_000 - 60)
            .unwrap_or(i64::MIN),
    )
}

pub(super) fn refresh<'a>(
    secret: &'a Value,
    settings: &'a Value,
    http: &'a dyn SimpleHttp,
) -> BoxFuture<'a, Result<Value, ChannelError>> {
    let request = match refresh_request(secret, settings) {
        Ok(request) => request,
        Err(error) => return Box::pin(async move { Err(error) }),
    };
    let send = http.send(request);
    Box::pin(async move {
        let response = send.await?;
        if !response.status().is_success() {
            return Err(ChannelError::Refresh(format!(
                "OpenCode token endpoint returned {}",
                response.status()
            )));
        }
        let token: TokenReply = serde_json::from_slice(response.body())
            .map_err(|error| ChannelError::Refresh(format!("OpenCode token JSON: {error}")))?;
        rotate(secret, token)
    })
}

fn refresh_request(secret: &Value, settings: &Value) -> Result<http::Request<Bytes>, ChannelError> {
    let base = field(secret, "console_base_url")
        .or_else(|| field(settings, "console_base_url"))
        .or_else(|| field(settings, "oauth_host"))
        .unwrap_or(CONSOLE_URL);
    let uri = crate::shared::http::join(base, "/auth/device/token", None)?;
    let body = json!({
        "grant_type":"refresh_token",
        "refresh_token":required(secret, "refresh_token")?,
        "client_id":CLIENT_ID,
    });
    http::Request::post(uri)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .body(Bytes::from(body.to_string()))
        .map_err(|error| ChannelError::Refresh(error.to_string()))
}

#[derive(Deserialize)]
struct TokenReply {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    #[serde(default, flatten)]
    rest: serde_json::Map<String, Value>,
}

fn rotate(secret: &Value, token: TokenReply) -> Result<Value, ChannelError> {
    let TokenReply {
        access_token,
        refresh_token,
        expires_in,
        rest,
    } = token;
    drop(rest);
    let access = access_token
        .filter(|token| !token.trim().is_empty())
        .ok_or_else(|| ChannelError::Refresh("OpenCode refresh returned no access_token".into()))?;
    let mut output = secret.clone();
    let object = output
        .as_object_mut()
        .ok_or_else(|| ChannelError::Refresh("OpenCode secret is not an object".into()))?;
    object.insert("api_key".into(), Value::String(access.clone()));
    object.insert("access_token".into(), Value::String(access));
    if let Some(refresh) = refresh_token.filter(|token| !token.trim().is_empty()) {
        object.insert("refresh_token".into(), Value::String(refresh));
    }
    if let Some(seconds) = expires_in {
        object.insert(
            "expires_at_ms".into(),
            Value::from(unix_now_ms().saturating_add(seconds.max(0).saturating_mul(1_000))),
        );
        object.remove("expiry_unknown");
    } else {
        object.remove("expires_at_ms");
        object.insert("expiry_unknown".into(), Value::Bool(true));
    }
    Ok(output)
}

pub(super) fn field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value
        .get(name)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn required<'a>(value: &'a Value, name: &str) -> Result<&'a str, ChannelError> {
    field(value, name).ok_or_else(|| ChannelError::Secret(format!("{name} missing")))
}

fn unix_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_millis()
        .try_into()
        .expect("Unix milliseconds fit i64")
}
