use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use http::header::{ACCEPT, CONTENT_TYPE, HeaderValue};

pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let target = super::endpoint::resolve(&ctx)?;
    let body = if super::messages::enabled(&ctx) {
        super::messages::body(&ctx)?
    } else {
        super::shape::request(&ctx, target.compact)?
    };
    let mut headers = crate::policy::request_headers(crate::policy::AWS_BEDROCK, &ctx)?;
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(ACCEPT, HeaderValue::from_static("application/json"));
    let mut request = http::Request::builder()
        .method(target.method)
        .uri(crate::shared::http::strip_userinfo(target.uri)?)
        .body(body)
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    *request.headers_mut() = headers;
    let region = super::endpoint::region(ctx.provider_settings)?;
    super::auth::apply(&mut request, ctx.secret, region)?;
    Ok(PreparedRequest {
        request,
        framing: target.framing,
        websocket: false,
        profile: None,
    })
}
