use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use http::header::{HeaderName, HeaderValue};
use serde_json::Value;

const DEFAULT_BASE_URL: &str = "https://api.anthropic.com";
const ANTHROPIC_VERSION: &str = "2023-06-01";
const FORWARD_HEADERS: &[&str] = &[
    "accept",
    "anthropic-beta",
    "anthropic-user-profile-id",
    "content-type",
];
const MODEL_QUERY: &[&str] = &["after_id", "before_id", "limit"];

pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let api_key = ctx
        .secret
        .get("api_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or_else(|| ChannelError::Secret("api_key missing".into()))?;
    let mut headers = crate::shared::http::allow_headers(ctx.headers, FORWARD_HEADERS);
    let body = super::shape::request(&ctx, &mut headers)?;
    let (method, path) = super::model::target(ctx.key, ctx.upstream_model)?;
    let query = model_query(&ctx);
    let uri = endpoint(&ctx, &path, query.as_deref())?;
    headers.insert(
        HeaderName::from_static("x-api-key"),
        HeaderValue::from_str(api_key)
            .map_err(|error| ChannelError::Secret(format!("api_key is invalid: {error}")))?,
    );
    headers.insert(
        HeaderName::from_static("anthropic-version"),
        HeaderValue::from_static(ANTHROPIC_VERSION),
    );
    let mut request = http::Request::builder()
        .method(method)
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

fn endpoint(
    ctx: &PrepareCtx<'_>,
    path: &str,
    query: Option<&str>,
) -> Result<http::Uri, ChannelError> {
    if let Some(url) = endpoint_override(ctx) {
        return crate::shared::http::exact(&url, query);
    }
    let base = ctx
        .provider_settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(DEFAULT_BASE_URL);
    crate::shared::http::join(base, path, query)
}

fn endpoint_override(ctx: &PrepareCtx<'_>) -> Option<String> {
    let name = super::model::endpoint_name(ctx.key)?;
    ctx.provider_settings
        .get("endpoints")?
        .get(name)?
        .as_str()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(|url| {
            url.replace(
                "{model}",
                &crate::shared::http::encode_component(ctx.upstream_model),
            )
        })
}

fn model_query(ctx: &PrepareCtx<'_>) -> Option<String> {
    (ctx.key.operation == gproxy_protocol::Operation::ListModels)
        .then(|| crate::shared::http::allow_query(ctx.query, MODEL_QUERY))
        .flatten()
}
