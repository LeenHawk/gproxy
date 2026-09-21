//! Which of the four prefixes an operation belongs to, and the one bearer
//! token all of them take.

use super::config::{
    ANTHROPIC_MODE, COMPATIBLE_MODE, DEFAULT_BASE_URL, DashScopeConfig, RERANK_PATH,
};
use super::image;
use crate::channel::{ChannelError, HeaderAllowlist, PrepareContext, forwardable};
use crate::channels::shared::compatible::http::{insert_configured, strip_query_auth};
use gproxy_protocol::{Dialect, HttpBody, Operation, WireRequest};
use http::{HeaderValue, header};

fn conversational(operation: Operation) -> bool {
    matches!(
        operation,
        Operation::GenerateContent
            | Operation::StreamGenerateContent
            | Operation::CountTokens
            | Operation::CompactContent
    )
}

pub(super) fn image_operation(operation: Operation) -> bool {
    matches!(operation, Operation::CreateImage | Operation::EditImage)
}

/// One origin, four layouts: the OpenAI-compatible mode for most of it, the
/// Anthropic-compatible mode for Messages, a rerank prefix of its own and the
/// native multimodal-generation endpoint for images.
pub(super) fn path(operation: Operation, dialect: Dialect, path: &str) -> String {
    let path = if path.starts_with('/') {
        path.to_owned()
    } else {
        format!("/{path}")
    };
    if image_operation(operation) {
        return image::PATH.to_owned();
    }
    if operation == Operation::Rerank {
        return RERANK_PATH.to_owned();
    }
    if conversational(operation) && dialect == Dialect::Claude {
        return format!("{ANTHROPIC_MODE}{path}");
    }
    format!("{COMPATIBLE_MODE}{path}")
}

pub(super) fn build(
    ctx: PrepareContext<'_>,
) -> Result<(http::request::Builder, WireRequest<HttpBody>), ChannelError> {
    let config = DashScopeConfig::from_view(ctx.provider)?;
    let key = ctx
        .credential
        .secret
        .get("api_key")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or(ChannelError::InvalidCredential)?;
    let operation = ctx.operation.operation;
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
                path(operation, ctx.operation.dialect, &request.path)
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
    // Every surface, the Anthropic-compatible one included, takes the key as
    // a bearer token and asks for no anthropic-version.
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {key}"))
            .map_err(|_| ChannelError::InvalidCredential)?,
    );
    for (name, value) in &config.headers {
        insert_configured(&mut headers, name, value)?;
    }
    if image_operation(operation) {
        let HttpBody::Bytes(bytes) = &request.body else {
            return Err(ChannelError::InvalidConfig(
                "DashScope image requests need a buffered JSON body; multipart uploads are not converted".into(),
            ));
        };
        request.body = HttpBody::Bytes(image::request(bytes, operation == Operation::EditImage)?);
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
    }
    let mut builder = http::Request::builder()
        .method(request.method.clone())
        .uri(uri);
    if let Some(map) = builder.headers_mut() {
        *map = headers;
    }
    Ok((builder, request))
}
