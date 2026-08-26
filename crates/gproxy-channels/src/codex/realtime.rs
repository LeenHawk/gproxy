use bytes::Bytes;
use gproxy_channel_api::{
    ChannelError, PreparedRequest, PreparedSession, RealtimeMeter, SessionPrepareCtx,
};
use http::Method;

pub(super) fn prepare(ctx: SessionPrepareCtx<'_>) -> Result<PreparedSession, ChannelError> {
    let id = crate::shared::openai::realtime::call_id(ctx.response_headers)?;
    let uri = crate::shared::openai::realtime::sideband_uri(&id)?;
    let mut headers = ctx.request_headers.clone();
    let session_id = super::auth::session_id(ctx.secret, &headers);
    super::auth::apply_headers(&mut headers, ctx.secret, &session_id)?;
    headers.remove(http::header::CONTENT_TYPE);
    headers.remove(http::header::CONTENT_LENGTH);
    headers.remove(http::header::ACCEPT);
    let mut request = http::Request::builder()
        .method(Method::GET)
        .uri(uri)
        .body(Bytes::new())
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    *request.headers_mut() = headers;
    Ok(PreparedSession {
        id,
        request: PreparedRequest {
            request,
            framing: None,
            websocket: true,
            profile: Some(&super::profile::CLIENT_PROFILE),
        },
        meter: RealtimeMeter::new(ctx.request_body, ctx.upstream_model),
    })
}
