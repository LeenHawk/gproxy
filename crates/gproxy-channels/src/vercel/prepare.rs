use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use http::header::{AUTHORIZATION, CONTENT_TYPE, HeaderName, HeaderValue};
use serde_json::Value;

const DEFAULT_BASE_URL: &str = "https://ai-gateway.vercel.sh";
pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let api_key = ctx
        .secret
        .get("api_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or_else(|| ChannelError::Secret("api_key missing".into()))?;
    let path = super::model::path(&ctx);
    let uri = endpoint(&ctx, &path)?;
    let mut headers = crate::policy::request_headers(crate::policy::VERCEL, &ctx)?;
    let body = super::model::rewrite(&ctx)?;
    let body = super::shape::request(&ctx, &mut headers, body)?;
    if !body.is_empty() && !headers.contains_key(CONTENT_TYPE) {
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    }
    insert(&mut headers, AUTHORIZATION, &format!("Bearer {api_key}"))?;
    insert(&mut headers, HeaderName::from_static("x-api-key"), api_key)?;
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

fn endpoint(ctx: &PrepareCtx<'_>, path: &str) -> Result<http::Uri, ChannelError> {
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
    let base = ctx
        .provider_settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(DEFAULT_BASE_URL);
    crate::shared::http::join(base, path, None)
}

fn insert(
    headers: &mut http::HeaderMap,
    name: HeaderName,
    value: &str,
) -> Result<(), ChannelError> {
    headers.insert(
        name,
        HeaderValue::from_str(value)
            .map_err(|error| ChannelError::Secret(format!("api_key is invalid: {error}")))?,
    );
    Ok(())
}
