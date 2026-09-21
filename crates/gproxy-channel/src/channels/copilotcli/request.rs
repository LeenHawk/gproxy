//! URL and headers for the two methods the Copilot backend serves.

use super::auth;
use super::config::CopilotCliConfig;
use super::identity::{CHANNEL_HEADERS, CLI_HEADERS};
use crate::channel::{ChannelError, HeaderAllowlist, PrepareContext, forwardable};
use crate::channels::shared::compatible::http::{insert_configured, strip_query_auth};
use gproxy_protocol::{Dialect, HttpBody, Operation, OperationKey};
use http::{HeaderValue, Method, header};

/// The Copilot backend answers exactly two routes for a CLI, so the client's
/// own path is never trusted (v3 `copilotcli/prepare.rs::target`).
fn target(operation: OperationKey) -> Result<(Method, &'static str), ChannelError> {
    match (operation.operation, operation.dialect) {
        (Operation::ListModels, _) => Ok((Method::GET, "/models")),
        (Operation::GenerateContent | Operation::StreamGenerateContent, Dialect::OpenAiChat) => {
            Ok((Method::POST, "/chat/completions"))
        }
        _ => Err(ChannelError::UnsupportedOperation(operation)),
    }
}

pub(super) fn build(ctx: PrepareContext<'_>) -> Result<http::Request<HttpBody>, ChannelError> {
    let config = CopilotCliConfig::from_view(ctx.provider)?;
    let (method, path) = target(ctx.operation)?;
    let url = match ctx.endpoint_override {
        Some(url) => url.to_owned(),
        None => format!(
            "{}{path}",
            auth::base_url(&config, ctx.provider, &ctx.credential)
        ),
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
    let allowlist = HeaderAllowlist::from_view_for(ctx.provider, CLI_HEADERS)?;
    let mut headers = forwardable(&ctx.request.headers, allowlist.as_ref(), CHANNEL_HEADERS);
    // `x-initiator` is read from the conversation, which only a buffered
    // body has; a streamed one is left to the safe answer.
    let body = ctx.request.body;
    let buffered = match &body {
        HttpBody::Bytes(bytes) => Some(bytes.clone()),
        HttpBody::Stream(_) => None,
    };
    super::identity::apply(&mut headers, &ctx.credential, buffered.as_deref())?;
    if method == Method::POST {
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
    }
    for (name, value) in &config.headers {
        insert_configured(&mut headers, name, value)?;
    }
    let mut builder = http::Request::builder().method(method).uri(uri);
    if let Some(map) = builder.headers_mut() {
        *map = headers;
    }
    builder
        .body(body)
        .map_err(|error| ChannelError::InvalidConfig(error.to_string()))
}
