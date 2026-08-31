use bytes::Bytes;
use gproxy_channel_api::{BoxFuture, ChannelError, SimpleHttp};
use http::header::{ACCEPT, CONTENT_TYPE, HeaderName};
use serde::Deserialize;
use serde_json::Value;

pub(super) fn due(secret: &Value) -> Option<i64> {
    super::auth::field(secret, "refresh_token")?;
    if super::auth::field(secret, "access_token").is_none() {
        return Some(i64::MIN);
    }
    if secret.get("expiry_unknown").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    Some(
        secret
            .get("expires_at_ms")
            .and_then(Value::as_i64)
            .map(|expires| expires / 1_000 - 300)
            .unwrap_or(i64::MIN),
    )
}

pub(super) fn refresh<'a>(
    secret: &'a Value,
    settings: &'a Value,
    http: &'a dyn SimpleHttp,
) -> BoxFuture<'a, Result<Value, ChannelError>> {
    let request = match request(secret, settings) {
        Ok(request) => request,
        Err(error) => return Box::pin(async move { Err(error) }),
    };
    let send = http.send(request);
    Box::pin(async move {
        let response = send.await?;
        if !response.status().is_success() {
            return Err(ChannelError::Refresh(format!(
                "WorkBuddy token endpoint returned {}",
                response.status()
            )));
        }
        let envelope: Envelope = serde_json::from_slice(response.body())
            .map_err(|error| ChannelError::Refresh(format!("WorkBuddy token JSON: {error}")))?;
        let Envelope {
            code,
            message,
            data,
            rest,
        } = envelope;
        drop(rest);
        if code != Some(0) {
            return Err(ChannelError::Refresh(format!(
                "WorkBuddy token endpoint: {} ({})",
                message.as_deref().unwrap_or("unknown error"),
                code.map_or_else(|| "missing".into(), |code| code.to_string())
            )));
        }
        rotate(
            secret,
            data.ok_or_else(|| {
                ChannelError::Refresh("WorkBuddy token response missing data".into())
            })?,
        )
    })
}

fn request(secret: &Value, settings: &Value) -> Result<http::Request<Bytes>, ChannelError> {
    let uri = crate::shared::http::join(
        super::auth::base_url(settings),
        "/v2/plugin/auth/token/refresh",
        None,
    )?;
    let mut request = http::Request::post(uri)
        .header(ACCEPT, "application/json")
        .header(CONTENT_TYPE, "application/json")
        .header(
            "x-refresh-token",
            super::auth::required(secret, "refresh_token")?,
        )
        .header("x-auth-refresh-source", "plugin")
        .header("x-product", "SaaS")
        .body(Bytes::from_static(b"{}"))
        .map_err(|error| ChannelError::Refresh(error.to_string()))?;
    if let Some(domain) = super::auth::field(secret, "domain") {
        super::auth::insert(
            request.headers_mut(),
            HeaderName::from_static("x-domain"),
            domain,
        )?;
    }
    Ok(request)
}

#[derive(Deserialize)]
struct Envelope {
    code: Option<i64>,
    #[serde(alias = "msg")]
    message: Option<String>,
    data: Option<Tokens>,
    #[serde(default, flatten)]
    rest: serde_json::Map<String, Value>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Tokens {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<i64>,
    expires_at: Option<i64>,
    refresh_expires_in: Option<i64>,
    refresh_expires_at: Option<i64>,
    domain: Option<String>,
    #[serde(default, flatten)]
    rest: serde_json::Map<String, Value>,
}

fn rotate(secret: &Value, token: Tokens) -> Result<Value, ChannelError> {
    let Tokens {
        access_token,
        refresh_token,
        expires_in,
        expires_at,
        refresh_expires_in,
        refresh_expires_at,
        domain,
        rest,
    } = token;
    drop(rest);
    if access_token.trim().is_empty() {
        return Err(ChannelError::Refresh(
            "WorkBuddy token response missing access_token".into(),
        ));
    }
    let mut output = secret.clone();
    let object = output
        .as_object_mut()
        .ok_or_else(|| ChannelError::Refresh("WorkBuddy secret is not an object".into()))?;
    object.insert("access_token".into(), Value::String(access_token));
    let refresh_token = refresh_token.filter(|token| !token.trim().is_empty());
    if let Some(refresh) = refresh_token.as_ref() {
        object.insert("refresh_token".into(), Value::String(refresh.clone()));
    }
    if let Some(expires) = expires_at.or_else(|| relative(expires_in)) {
        object.insert("expires_at_ms".into(), Value::from(expires));
        object.remove("expiry_unknown");
    } else {
        object.remove("expires_at_ms");
        object.insert("expiry_unknown".into(), Value::Bool(true));
    }
    if let Some(domain) = domain.filter(|domain| !domain.trim().is_empty()) {
        object.insert("domain".into(), Value::String(domain));
    }
    if let Some(expires) = refresh_expires_at.or_else(|| relative(refresh_expires_in)) {
        object.insert("refresh_expires_at_ms".into(), Value::from(expires));
    } else if refresh_token.is_some() {
        object.remove("refresh_expires_at_ms");
    }
    Ok(output)
}

fn relative(seconds: Option<i64>) -> Option<i64> {
    seconds.map(|seconds| unix_now_ms().saturating_add(seconds.max(0).saturating_mul(1_000)))
}

fn unix_now_ms() -> i64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .expect("system clock before epoch")
        .as_millis()
        .try_into()
        .expect("Unix milliseconds fit i64")
}
