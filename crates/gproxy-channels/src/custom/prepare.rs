use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use gproxy_protocol::{Operation, StreamFraming};
use http::header::{AUTHORIZATION, HeaderName, HeaderValue};
use serde_json::Value;

const FORWARD_HEADERS: &[&str] = &[
    "accept",
    "anthropic-beta",
    "content-type",
    "openai-beta",
    "range",
];

pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let api_key = ctx
        .secret
        .get("api_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or_else(|| ChannelError::Secret("api_key missing".into()))?;
    let path = super::model::path(&ctx);
    let query = query(&ctx);
    let uri = endpoint(&ctx, &path, query.as_deref())?;
    let mut headers = crate::shared::http::allow_headers(ctx.headers, FORWARD_HEADERS);
    auth(&mut headers, super::model::auth(ctx.key), api_key)?;
    let body = super::model::body(&ctx)?;
    let body = super::shape::request(ctx.key, ctx.provider_settings, &mut headers, body)?;
    let mut request = http::Request::builder()
        .method(ctx.method)
        .uri(crate::shared::http::strip_userinfo(uri)?)
        .body(body)
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    *request.headers_mut() = headers;
    Ok(PreparedRequest {
        request,
        framing: framing(&ctx),
        websocket: false,
        profile: None,
    })
}

fn endpoint(
    ctx: &PrepareCtx<'_>,
    path: &str,
    query: Option<&str>,
) -> Result<http::Uri, ChannelError> {
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
        let url =
            crate::shared::openai::endpoint::replace_resource(url, ctx.key.operation, ctx.path);
        return crate::shared::http::exact(&url, query);
    }
    let base = ctx
        .provider_settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .ok_or_else(|| {
            ChannelError::Prepare("custom channel requires base_url or endpoint override".into())
        })?;
    crate::shared::http::join(base, path, query)
}

fn query(ctx: &PrepareCtx<'_>) -> Option<String> {
    let allowed: &[&str] = match super::model::auth(ctx.key) {
        super::model::AuthKind::OpenAi => &["after", "limit", "order", "purpose", "variant"],
        super::model::AuthKind::Claude => &["after_id", "before_id", "limit"],
        super::model::AuthKind::Gemini => &["alt", "pageSize", "pageToken"],
    };
    crate::shared::http::allow_query(ctx.query, allowed)
}

fn framing(ctx: &PrepareCtx<'_>) -> Option<StreamFraming> {
    (ctx.key.operation == Operation::StreamGenerateContent
        && matches!(super::model::auth(ctx.key), super::model::AuthKind::Gemini))
    .then(|| {
        if ctx
            .query
            .is_some_and(|query| query.split('&').any(|part| part == "alt=sse"))
        {
            StreamFraming::Sse
        } else {
            StreamFraming::JsonArray
        }
    })
}

fn auth(
    headers: &mut http::HeaderMap,
    kind: super::model::AuthKind,
    key: &str,
) -> Result<(), ChannelError> {
    let (name, value) = match kind {
        super::model::AuthKind::OpenAi => (AUTHORIZATION, format!("Bearer {key}")),
        super::model::AuthKind::Claude => (HeaderName::from_static("x-api-key"), key.into()),
        super::model::AuthKind::Gemini => (HeaderName::from_static("x-goog-api-key"), key.into()),
    };
    headers.insert(
        name,
        HeaderValue::from_str(&value)
            .map_err(|error| ChannelError::Secret(format!("api_key is invalid: {error}")))?,
    );
    if matches!(kind, super::model::AuthKind::Claude) {
        headers.insert(
            HeaderName::from_static("anthropic-version"),
            HeaderValue::from_static("2023-06-01"),
        );
    }
    Ok(())
}
