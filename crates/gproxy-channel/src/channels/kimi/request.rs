//! Request preparation for both modes.

use super::auth::{self, Mode};
use super::config::KimiConfig;
use crate::channel::{ChannelError, HeaderAllowlist, PrepareContext, forwardable};
use crate::channels::shared::compatible::http::{insert_configured, strip_query_auth};
use gproxy_protocol::{Dialect, HttpBody, WireRequest};
use http::{HeaderName, HeaderValue, header};

const ANTHROPIC_VERSION: &str = "2023-06-01";

pub(super) fn build(
    ctx: PrepareContext<'_>,
) -> Result<(http::request::Builder, WireRequest<HttpBody>), ChannelError> {
    let config = KimiConfig::from_view(ctx.provider)?;
    let mode = auth::mode(&ctx.credential);
    let token = auth::token(&ctx.credential, mode)?;
    let mut request = ctx.request;
    let url = match ctx.endpoint_override {
        Some(url) => url.to_owned(),
        None => format!(
            "{}{}",
            auth::base_url(ctx.provider, &ctx.credential, mode),
            auth::path(mode, &request.path)
        ),
    };
    let uri = match request
        .query
        .take()
        .map(|query| strip_query_auth(&query))
        .filter(|query| !query.is_empty())
    {
        Some(query) => format!("{url}?{query}"),
        None => url,
    };
    let allowlist = HeaderAllowlist::from_view(ctx.provider)?;
    // The CLI identity is the channel's to state; a client may not forge it.
    let mut headers = forwardable(&request.headers, allowlist.as_ref(), auth::IDENTITY_HEADERS);
    if mode == Mode::Subscription && ctx.operation.dialect == Dialect::Claude {
        headers.insert(
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_str(token).map_err(|_| ChannelError::InvalidCredential)?,
        );
        headers
            .entry(HeaderName::from_static("anthropic-version"))
            .or_insert_with(|| HeaderValue::from_static(ANTHROPIC_VERSION));
    } else {
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| ChannelError::InvalidCredential)?,
        );
    }
    if mode == Mode::Subscription {
        auth::identity(&mut headers, &config, &ctx.credential)?;
    }
    for (name, value) in &config.headers {
        insert_configured(&mut headers, name, value)?;
    }
    let mut builder = http::Request::builder()
        .method(request.method.clone())
        .uri(uri);
    if let Some(map) = builder.headers_mut() {
        *map = headers;
    }
    Ok((builder, request))
}
