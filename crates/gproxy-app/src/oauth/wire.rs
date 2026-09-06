use bytes::Bytes;
use gproxy_channel_api::{OAuthBrowserUser, OAuthError, OAuthService, OAuthTokenSet};
use http::{HeaderValue, Response, StatusCode, header, request::Parts};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

use crate::host::AppHost;

pub(super) fn parse<T: DeserializeOwned>(parts: &Parts, body: &[u8]) -> Result<T, OAuthError> {
    if parts
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.starts_with("application/json"))
    {
        serde_json::from_slice(body).map_err(|_| OAuthError::InvalidRequest)
    } else {
        let pairs: std::collections::BTreeMap<String, String> =
            serde_urlencoded::from_bytes(body).map_err(|_| OAuthError::InvalidRequest)?;
        serde_json::from_value(json!(pairs)).map_err(|_| OAuthError::InvalidRequest)
    }
}

pub(super) fn query<T: DeserializeOwned>(parts: &Parts) -> Result<T, OAuthError> {
    if parts.uri.query().is_some_and(|query| query.len() > 8192) {
        return Err(OAuthError::InvalidRequest);
    }
    serde_urlencoded::from_str(parts.uri.query().unwrap_or_default())
        .map_err(|_| OAuthError::InvalidRequest)
}

pub(super) fn string<'a>(value: &'a Value, name: &str) -> Result<&'a str, OAuthError> {
    value
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 4096)
        .ok_or(OAuthError::InvalidRequest)
}

pub(super) fn issuer(parts: &Parts) -> Result<String, OAuthError> {
    let authority = parts
        .headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .ok_or(OAuthError::InvalidRequest)?
        .parse::<http::uri::Authority>()
        .map_err(|_| OAuthError::InvalidRequest)?;
    if authority.as_str().contains('@') {
        return Err(OAuthError::InvalidRequest);
    }
    let scheme = parts
        .headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok())
        .or(parts.uri.scheme_str())
        .unwrap_or("http");
    if !matches!(scheme, "http" | "https") {
        return Err(OAuthError::InvalidRequest);
    }
    Ok(format!("{scheme}://{authority}"))
}

pub(super) async fn browser(
    host: &AppHost,
    parts: &Parts,
    write: bool,
) -> Result<OAuthBrowserUser, OAuthError> {
    if write {
        let origin = parts
            .headers
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok())
            .ok_or(OAuthError::AccessDenied)?;
        if origin != issuer(parts)? {
            return Err(OAuthError::AccessDenied);
        }
    }
    host.browser_user(&parts.headers)
        .await?
        .ok_or(OAuthError::AccessDenied)
}

pub(super) fn json_response(status: StatusCode, value: &impl Serialize) -> Response<Bytes> {
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::CACHE_CONTROL, "no-store")
        .header(header::PRAGMA, "no-cache")
        .header(header::REFERRER_POLICY, "no-referrer")
        .body(Bytes::from(
            serde_json::to_vec(value).expect("OAuth JSON serializes"),
        ))
        .expect("static response headers")
}

pub(super) fn error(status: StatusCode, code: &str) -> Response<Bytes> {
    json_response(
        status,
        &gproxy_admin::dto::OAuthErrorDto { error: code.into() },
    )
}

pub(super) fn redirect(location: &str) -> Result<Response<Bytes>, OAuthError> {
    let mut response = json_response(StatusCode::FOUND, &json!({}));
    response.headers_mut().insert(
        header::LOCATION,
        HeaderValue::from_str(location).map_err(|_| OAuthError::InvalidRequest)?,
    );
    Ok(response)
}

pub(super) fn encode(value: &str) -> String {
    form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

pub(super) fn tokens(value: OAuthTokenSet) -> Response<Bytes> {
    let mut body = json!({"token_type":"Bearer", "access_token":value.access_token, "refresh_token":value.refresh_token, "expires_in":value.expires_in});
    if !value.id_token.is_empty() {
        body["id_token"] = json!(value.id_token);
    }
    json_response(StatusCode::OK, &body)
}
