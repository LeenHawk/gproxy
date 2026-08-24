use bytes::Bytes;
use gproxy_channel_api::{BoxFuture, ChannelError, SimpleHttp};
use http::header::{ACCEPT, CONTENT_TYPE, USER_AGENT};
use serde_json::Value;

const DEFAULT_AUTH_BASE: &str = "https://prod.us-east-1.auth.desktop.kiro.dev";
const EXPIRY_SKEW_SECONDS: i64 = 60;

pub(super) fn access_token(secret: &Value) -> Result<&str, ChannelError> {
    field(secret, "access_token").ok_or_else(|| ChannelError::Secret("access_token missing".into()))
}

pub(super) fn profile_arn<'a>(secret: &'a Value, settings: &'a Value) -> Option<&'a str> {
    field(secret, "profile_arn").or_else(|| field(settings, "profile_arn"))
}

pub(super) fn refresh_due(secret: &Value) -> Option<i64> {
    if field(secret, "access_token").is_none() {
        return Some(i64::MIN);
    }
    match secret.get("expires_at_ms").and_then(Value::as_i64) {
        Some(expiry) if expiry != 0 => Some(expiry / 1_000 - EXPIRY_SKEW_SECONDS),
        _ => None,
    }
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
            return Err(ChannelError::Refresh(format!(
                "Kiro auth endpoint {}",
                response.status()
            )));
        }
        let token: Value = serde_json::from_slice(response.body())
            .map_err(|error| ChannelError::Refresh(format!("Kiro auth JSON: {error}")))?;
        rotate(secret, &token)
    })
}

fn refresh_request(secret: &Value, settings: &Value) -> Result<http::Request<Bytes>, ChannelError> {
    let refresh = field(secret, "refresh_token")
        .ok_or_else(|| ChannelError::Refresh("refresh_token missing".into()))?;
    let (url, body) = if is_sso(secret) {
        let region = field(secret, "region").unwrap_or("us-east-1");
        super::endpoint::validate_region(region)?;
        (
            format!("https://oidc.{region}.amazonaws.com/token"),
            serde_json::json!({
                "clientId":required(secret,"client_id")?,
                "clientSecret":required(secret,"client_secret")?,
                "refreshToken":refresh,
                "grantType":"refresh_token"
            }),
        )
    } else {
        let base = field(settings, "auth_base_url").unwrap_or(DEFAULT_AUTH_BASE);
        (
            format!("{}/refreshToken", base.trim_end_matches('/')),
            serde_json::json!({"refreshToken":refresh}),
        )
    };
    let body =
        serde_json::to_vec(&body).map_err(|error| ChannelError::Refresh(error.to_string()))?;
    let mut request = http::Request::post(url)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .header(USER_AGENT, "Kiro-CLI")
        .body(Bytes::from(body))
        .map_err(|error| ChannelError::Refresh(error.to_string()))?;
    request
        .extensions_mut()
        .insert(super::profile::CLIENT_PROFILE);
    Ok(request)
}

fn rotate(secret: &Value, token: &Value) -> Result<Value, ChannelError> {
    let access = field(token, "accessToken")
        .ok_or_else(|| ChannelError::Refresh("response missing accessToken".into()))?;
    let mut output = secret.clone();
    let root = output
        .as_object_mut()
        .ok_or_else(|| ChannelError::Refresh("Kiro secret must be an object".into()))?;
    root.insert("access_token".into(), Value::String(access.into()));
    for (source, target) in [
        ("refreshToken", "refresh_token"),
        ("profileArn", "profile_arn"),
    ] {
        if let Some(value) = field(token, source) {
            root.insert(target.into(), Value::String(value.into()));
        }
    }
    if let Some(expires) = token.get("expiresIn").and_then(Value::as_i64) {
        root.insert(
            "expires_at_ms".into(),
            Value::from(unix_now_ms().saturating_add(expires.max(0).saturating_mul(1_000))),
        );
    } else {
        root.remove("expires_at_ms");
    }
    Ok(output)
}

fn is_sso(secret: &Value) -> bool {
    field(secret, "client_id").is_some() && field(secret, "client_secret").is_some()
}

fn required<'a>(value: &'a Value, name: &str) -> Result<&'a str, ChannelError> {
    field(value, name).ok_or_else(|| ChannelError::Refresh(format!("{name} missing")))
}

pub(super) fn field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value
        .get(name)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn unix_now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_millis()
        .try_into()
        .expect("Unix milliseconds fit i64")
}
