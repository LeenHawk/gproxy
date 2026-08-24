use bytes::Bytes;
use gproxy_channel_api::{BoxFuture, ChannelError, ClientProfile, SimpleHttp};
use serde_json::Value;

const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";

pub(crate) fn refresh_due(secret: &Value) -> Option<i64> {
    field(secret, "refresh_token")?;
    if field(secret, "access_token").is_none() {
        return Some(i64::MIN);
    }
    secret
        .get("expires_at_ms")
        .and_then(Value::as_i64)
        .filter(|expires| *expires != 0)
        .map(|expires| expires / 1_000 - 60)
}

pub(crate) fn refresh<'a>(
    secret: &'a Value,
    settings: &'a Value,
    http: &'a dyn SimpleHttp,
    profile: &'static ClientProfile,
) -> BoxFuture<'a, Result<Value, ChannelError>> {
    let request = match build_refresh(secret, settings, profile) {
        Ok(request) => request,
        Err(error) => return Box::pin(async move { Err(error) }),
    };
    let send = http.send(request);
    Box::pin(async move {
        let response = send.await?;
        if !response.status().is_success() {
            return Err(ChannelError::Refresh(format!(
                "Google token endpoint returned {}",
                response.status()
            )));
        }
        let token: Value = serde_json::from_slice(response.body())
            .map_err(|error| ChannelError::Refresh(format!("token response JSON: {error}")))?;
        rotate(secret, &token)
    })
}

fn build_refresh(
    secret: &Value,
    settings: &Value,
    profile: &'static ClientProfile,
) -> Result<http::Request<Bytes>, ChannelError> {
    let refresh = required(secret, "refresh_token")?;
    let client_id = field(settings, "oauth_client_id")
        .or_else(|| field(secret, "client_id"))
        .ok_or_else(|| ChannelError::Secret("Google OAuth client_id missing".into()))?;
    let client_secret = field(settings, "oauth_client_secret")
        .or_else(|| field(secret, "client_secret"))
        .ok_or_else(|| ChannelError::Secret("Google OAuth client_secret missing".into()))?;
    let url = field(settings, "oauth_token_url")
        .or_else(|| field(secret, "oauth_token_url"))
        .unwrap_or(TOKEN_URL);
    let body = crate::shared::http::form(&[
        ("grant_type", "refresh_token"),
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("refresh_token", refresh),
    ]);
    let mut request = http::Request::post(url)
        .header(
            http::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .header(http::header::ACCEPT, "application/json")
        .body(Bytes::from(body))
        .map_err(|error| ChannelError::Refresh(error.to_string()))?;
    request.extensions_mut().insert(*profile);
    Ok(request)
}

fn rotate(secret: &Value, token: &Value) -> Result<Value, ChannelError> {
    let access = field(token, "access_token")
        .ok_or_else(|| ChannelError::Refresh("access_token missing".into()))?;
    let mut output = secret.clone();
    let object = output
        .as_object_mut()
        .ok_or_else(|| ChannelError::Refresh("secret must be an object".into()))?;
    object.insert("access_token".into(), Value::String(access.into()));
    if let Some(refresh) = field(token, "refresh_token") {
        object.insert("refresh_token".into(), Value::String(refresh.into()));
    }
    if let Some(expires) = token.get("expires_in").and_then(Value::as_i64) {
        object.insert(
            "expires_at_ms".into(),
            Value::from(
                now_seconds()
                    .saturating_add(expires.max(0))
                    .saturating_mul(1_000),
            ),
        );
    } else {
        object.remove("expires_at_ms");
    }
    Ok(output)
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

fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_secs()
        .try_into()
        .expect("Unix seconds fit i64")
}
