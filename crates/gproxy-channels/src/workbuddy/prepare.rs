use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use http::header::{CONTENT_TYPE, HeaderValue};
use serde_json::Value;

pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let path = super::model::path(ctx.key);
    let uri = endpoint(&ctx, path)?;
    let mut headers = crate::policy::request_headers(crate::policy::WORKBUDDY, &ctx)?;
    let body = super::model::body(&ctx, &mut headers)?;
    if !body.is_empty() {
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    }
    super::auth::apply(&mut headers, ctx.secret)?;
    super::identity::apply(&mut headers, ctx.secret)?;
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
    if let Some(name) = super::model::endpoint_name(ctx.key)
        && let Some(url) = ctx
            .provider_settings
            .get("endpoints")
            .and_then(|endpoints| endpoints.get(name))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|url| !url.is_empty())
    {
        return crate::shared::http::exact(
            &url.replace(
                "{model}",
                &crate::shared::http::encode_component(ctx.upstream_model),
            ),
            None,
        );
    }
    crate::shared::http::join(super::auth::base_url(ctx.provider_settings), path, None)
}
