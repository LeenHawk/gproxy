use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use http::header::{AUTHORIZATION, CONTENT_TYPE, HeaderName, HeaderValue};
use serde_json::Value;

const DEFAULT_BASE_URL: &str = "https://api.cloudflare.com";
const DEFAULT_GATEWAY_ID: &str = "default";
const FORWARD_HEADERS: &[&str] = &[
    "accept",
    "content-type",
    "cf-aig-skip-cache",
    "cf-aig-cache-ttl",
    "cf-aig-cache-key",
    "cf-aig-collect-log",
    "cf-aig-request-timeout",
    "cf-aig-max-attempts",
    "cf-aig-retry-delay",
    "cf-aig-backoff",
    "cf-aig-metadata",
];

pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let api_key = secret(ctx.secret, "api_key")
        .ok_or_else(|| ChannelError::Secret("api_key missing".into()))?;
    let uri = endpoint(&ctx)?;
    let body = super::model::rewrite(&ctx)?;
    let mut headers = crate::shared::http::allow_headers(ctx.headers, FORWARD_HEADERS);
    if !body.is_empty() && !headers.contains_key(CONTENT_TYPE) {
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    }
    insert(&mut headers, AUTHORIZATION, &format!("Bearer {api_key}"))?;
    insert(
        &mut headers,
        HeaderName::from_static("cf-aig-gateway-id"),
        secret(ctx.secret, "gateway_id").unwrap_or(DEFAULT_GATEWAY_ID),
    )?;
    let mut request = http::Request::builder()
        .method(ctx.method)
        .uri(crate::shared::http::strip_userinfo(uri)?)
        .body(body)
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    *request.headers_mut() = headers;
    Ok(PreparedRequest {
        request,
        framing: None,
        websocket: false,
        profile: None,
    })
}

fn endpoint(ctx: &PrepareCtx<'_>) -> Result<http::Uri, ChannelError> {
    if let Some(name) = super::model::endpoint(ctx.key)
        && let Some(url) = ctx
            .provider_settings
            .get("endpoints")
            .and_then(|endpoints| endpoints.get(name))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|url| !url.is_empty())
    {
        let url = url.replace(
            "{model}",
            &crate::shared::http::encode_component(ctx.upstream_model),
        );
        return crate::shared::http::exact(&url, None);
    }
    let account = secret(ctx.secret, "account_id")
        .ok_or_else(|| ChannelError::Secret("account_id missing".into()))?;
    let path = format!(
        "/client/v4/accounts/{}/ai{}",
        crate::shared::http::encode_component(account),
        ctx.path
    );
    let base = ctx
        .provider_settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(DEFAULT_BASE_URL);
    crate::shared::http::join(base, &path, None)
}

fn secret<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn insert(
    headers: &mut http::HeaderMap,
    name: HeaderName,
    value: &str,
) -> Result<(), ChannelError> {
    headers.insert(
        name,
        HeaderValue::from_str(value)
            .map_err(|error| ChannelError::Secret(format!("credential is invalid: {error}")))?,
    );
    Ok(())
}
