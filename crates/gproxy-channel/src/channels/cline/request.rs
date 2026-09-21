//! URL, authentication and headers for the two methods Cline serves.

use super::auth::{self, CHANNEL_HEADERS, CLIENT_HEADERS};
use super::config::{ClineConfig, base_url};
use crate::channel::{ChannelError, HeaderAllowlist, PrepareContext, forwardable};
use crate::channels::shared::compatible::http::{insert_configured, strip_query_auth};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey};
use http::{HeaderValue, Method, header};

/// The catalogue the Cline extension reads (v3 `cline/prepare.rs::target`).
pub(super) const MODELS_PATH: &str = "/ai/cline/recommended-models";
/// The only generation method; Cline fronts every vendor behind it.
pub(super) const CHAT_PATH: &str = "/chat/completions";

/// The method and path an operation reaches, or `UnsupportedOperation`.
/// Cline's REST surface has exactly two routes, so the client's own path is
/// never trusted.
fn target(operation: OperationKey) -> Result<(Method, &'static str), ChannelError> {
    match (operation.operation, operation.dialect) {
        (Operation::ListModels, _) => Ok((Method::GET, MODELS_PATH)),
        (Operation::GenerateContent | Operation::StreamGenerateContent, Dialect::OpenAiChat) => {
            Ok((Method::POST, CHAT_PATH))
        }
        _ => Err(ChannelError::UnsupportedOperation(operation)),
    }
}

pub(super) fn build(ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
    let config = ClineConfig::from_view(ctx.provider)?;
    let (method, path) = target(ctx.operation)?;
    let url = match ctx.endpoint_override {
        Some(url) => url.to_owned(),
        None => format!("{}{path}", base_url(ctx.provider)),
    };
    let uri = match ctx
        .request
        .query
        .as_deref()
        .map(strip_query_auth)
        .filter(|query| !query.is_empty())
    {
        Some(query) => format!("{url}?{query}"),
        None => url,
    };
    let allowlist = HeaderAllowlist::from_view_for(ctx.provider, CLIENT_HEADERS)?;
    let mut headers = forwardable(&ctx.request.headers, allowlist.as_ref(), CHANNEL_HEADERS);
    auth::apply(&mut headers, &ctx.credential)?;
    if method == Method::POST {
        headers
            .entry(header::CONTENT_TYPE)
            .or_insert_with(|| HeaderValue::from_static("application/json"));
    }
    for (name, value) in &config.headers {
        insert_configured(&mut headers, name, value)?;
    }
    let mut builder = http::Request::builder().method(method).uri(uri);
    if let Some(map) = builder.headers_mut() {
        *map = headers;
    }
    builder
        .body(ctx.request.body)
        .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
}
