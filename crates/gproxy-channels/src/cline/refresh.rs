use bytes::Bytes;
use gproxy_channel_api::{BoxFuture, ChannelError, SimpleHttp};
use http::header::{ACCEPT, CONTENT_TYPE};
use serde_json::Value;

const EXPIRY_SKEW_SECONDS: i64 = 300;

pub(super) fn due(secret: &Value) -> Option<i64> {
    super::auth::field(secret, "refresh_token")?;
    let token = match super::auth::field(secret, "access_token") {
        Some(token) => token,
        None => return Some(i64::MIN),
    };
    super::auth::token_expiry(token)
        .map(|expiry| expiry - EXPIRY_SKEW_SECONDS)
        .or(Some(i64::MIN))
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
                "Cline refresh endpoint returned {}",
                response.status()
            )));
        }
        let value: Value = serde_json::from_slice(response.body())
            .map_err(|error| ChannelError::Refresh(format!("Cline refresh JSON: {error}")))?;
        rotate(secret, &value)
    })
}

fn refresh_request(secret: &Value, settings: &Value) -> Result<http::Request<Bytes>, ChannelError> {
    let token = super::auth::field(secret, "refresh_token")
        .ok_or_else(|| ChannelError::Refresh("refresh_token missing".into()))?;
    let uri = crate::shared::http::join(super::prepare::base_url(settings), "/auth/refresh", None)?;
    let body = serde_json::to_vec(&serde_json::json!({
        "refreshToken":token, "grantType":"refresh_token"
    }))
    .map_err(|error| ChannelError::Refresh(error.to_string()))?;
    http::Request::post(uri)
        .header(CONTENT_TYPE, "application/json")
        .header(ACCEPT, "application/json")
        .body(Bytes::from(body))
        .map_err(|error| ChannelError::Refresh(error.to_string()))
}

fn rotate(secret: &Value, response: &Value) -> Result<Value, ChannelError> {
    if response.get("success").and_then(Value::as_bool) != Some(true) {
        return Err(ChannelError::Refresh(
            "Cline refresh response was not successful".into(),
        ));
    }
    let data = response
        .get("data")
        .and_then(Value::as_object)
        .ok_or_else(|| ChannelError::Refresh("Cline refresh response has no data".into()))?;
    let access = data
        .get("accessToken")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| ChannelError::Refresh("Cline response missing accessToken".into()))?;
    let mut output = secret.clone();
    let root = output
        .as_object_mut()
        .ok_or_else(|| ChannelError::Refresh("Cline secret must be an object".into()))?;
    root.insert("api_key".into(), Value::String(access.into()));
    root.insert("access_token".into(), Value::String(access.into()));
    if let Some(refresh) = data
        .get("refreshToken")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
    {
        root.insert("refresh_token".into(), Value::String(refresh.into()));
    }
    if let Some(info) = data.get("userInfo").and_then(Value::as_object) {
        copy(info, root, "clineUserId", "user_id");
        copy(info, root, "email", "email");
    }
    Ok(output)
}

fn copy(
    source: &serde_json::Map<String, Value>,
    target: &mut serde_json::Map<String, Value>,
    from: &str,
    to: &str,
) {
    if let Some(value) = source
        .get(from)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
    {
        target.insert(to.into(), Value::String(value.into()));
    }
}
