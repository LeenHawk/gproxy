use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use gproxy_protocol::Operation;
use http::header::{CONTENT_TYPE, HeaderValue};

const MODEL_QUERY: &[&str] = &["pageSize", "pageToken"];

pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let access_token = super::auth::access_token(ctx.secret)?;
    let target = super::model::target(&ctx)?;
    let query = query(&ctx, &target);
    let uri = endpoint(&ctx, &target, query.as_deref())?;
    let mut headers = if super::model::is_claude(ctx.key) {
        crate::shared::http::allow_headers(ctx.headers, &["anthropic-beta"])
    } else {
        http::HeaderMap::new()
    };
    super::auth::apply(&mut headers, access_token)?;
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    let body = super::shape::request(&ctx)?;
    let mut request = http::Request::builder()
        .method(target.method)
        .uri(crate::shared::http::strip_userinfo(uri)?)
        .body(body)
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    *request.headers_mut() = headers;
    Ok(PreparedRequest {
        request,
        framing: (ctx.key.operation == Operation::StreamGenerateContent)
            .then_some(gproxy_protocol::StreamFraming::Sse),
        websocket: false,
        profile: None,
    })
}

fn endpoint(
    ctx: &PrepareCtx<'_>,
    target: &super::model::Target,
    query: Option<&str>,
) -> Result<http::Uri, ChannelError> {
    if let Some(url) = endpoint_override(ctx, target.endpoint)? {
        if target.query == Some("alt=sse")
            && let Some(alt) = url
                .split_once('?')
                .map(|(_, query)| query)
                .and_then(|query| query.split('&').find(|part| part.starts_with("alt=")))
        {
            if alt != "alt=sse" {
                return Err(ChannelError::Prepare(
                    "Vertex streaming endpoint override must use alt=sse".into(),
                ));
            }
            return crate::shared::http::exact(&url, None);
        }
        return crate::shared::http::exact(&url, query);
    }
    let base = super::endpoint::default_base(ctx.provider_settings, ctx.secret)?;
    crate::shared::http::join(&base, &target.path, query)
}

fn endpoint_override(ctx: &PrepareCtx<'_>, name: &str) -> Result<Option<String>, ChannelError> {
    let Some(url) = ctx
        .provider_settings
        .get("endpoints")
        .and_then(|endpoints| endpoints.get(name))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
    else {
        return Ok(None);
    };
    if url.is_empty() {
        return Ok(None);
    }
    let model = crate::shared::http::encode_component(super::model::model_id(ctx.upstream_model));
    let project = super::endpoint::project_id(ctx.secret)?;
    let location = super::endpoint::location(ctx.provider_settings, ctx.secret)?;
    Ok(Some(
        url.replace("{model}", &model)
            .replace("{project}", &crate::shared::http::encode_component(project))
            .replace(
                "{location}",
                &crate::shared::http::encode_component(location),
            ),
    ))
}

fn query(ctx: &PrepareCtx<'_>, target: &super::model::Target) -> Option<String> {
    let mut parts = Vec::new();
    if ctx.key.operation == Operation::ListModels
        && let Some(query) = crate::shared::http::allow_query(ctx.query, MODEL_QUERY)
    {
        parts.push(query);
    }
    if let Some(query) = target.query {
        parts.push(query.into());
    }
    (!parts.is_empty()).then(|| parts.join("&"))
}
