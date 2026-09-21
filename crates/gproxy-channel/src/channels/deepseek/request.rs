//! The three surfaces' paths and the authentication each one wants.

use super::config::{DEFAULT_BASE_URL, DeepSeekConfig};
use crate::channel::{ChannelError, HeaderAllowlist, PrepareContext, forwardable};
use crate::channels::shared::compatible::http::{insert_configured, strip_query_auth};
use gproxy_protocol::{Dialect, Operation, WireRequest};
use http::{HeaderName, HeaderValue, header};

/// Operations whose path is the dialect's own conversation endpoint; only
/// those move between the three surfaces. Model listing and anything else
/// keeps the path the caller sent.
fn conversational(operation: Operation) -> bool {
    matches!(
        operation,
        Operation::GenerateContent
            | Operation::StreamGenerateContent
            | Operation::CountTokens
            | Operation::CompactContent
    )
}

/// `/v1/chat/completions` as sent, `/v1/responses` without its `/v1`, and the
/// Messages path under `/anthropic`.
pub(super) fn path(operation: Operation, dialect: Dialect, path: &str) -> String {
    let path = if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    };
    if !conversational(operation) {
        return path;
    }
    match dialect {
        Dialect::Claude => format!("/anthropic{path}"),
        Dialect::OpenAi | Dialect::OpenAiResponsesWebSocket => path
            .strip_prefix("/v1/")
            .map(|rest| format!("/{rest}"))
            .unwrap_or(path),
        _ => path,
    }
}

/// Build everything but the body, so HTTP and websocket preparation share it.
pub(super) fn build<B>(
    ctx: PrepareContext<'_, B>,
) -> Result<(http::request::Builder, WireRequest<B>), ChannelError> {
    let config = DeepSeekConfig::from_view(ctx.provider)?;
    let key = ctx
        .credential
        .secret
        .get("api_key")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or(ChannelError::InvalidCredential)?;
    let mut request = ctx.request;
    let url = match ctx.endpoint_override {
        Some(url) => url.to_owned(),
        None => {
            let base = ctx
                .provider
                .base_url
                .map(str::trim)
                .filter(|base| !base.is_empty())
                .unwrap_or(DEFAULT_BASE_URL)
                .trim_end_matches('/');
            format!(
                "{base}{}",
                path(
                    ctx.operation.operation,
                    ctx.operation.dialect,
                    &request.path
                )
            )
        }
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
    let mut headers = forwardable(&request.headers, allowlist.as_ref(), &[]);
    if ctx.operation.dialect == Dialect::Claude {
        // DeepSeek's Anthropic-compatible surface takes the key the way
        // Anthropic does, but asks for no anthropic-version.
        headers.insert(
            HeaderName::from_static("x-api-key"),
            HeaderValue::from_str(key).map_err(|_| ChannelError::InvalidCredential)?,
        );
    } else {
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {key}"))
                .map_err(|_| ChannelError::InvalidCredential)?,
        );
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
