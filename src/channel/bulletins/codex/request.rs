//! Upstream URL, forwarded-header allow-list, and request preparation.

use crate::channel::http_util::{allow_headers, build_request, exact_url, join_url};
use crate::channel::settings::endpoint_url;
use crate::channel::{ChannelError, PrepareCtx, PreparedRequest};

pub(super) fn prepare(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let access_token = super::token::access_token(ctx.secret)?.to_string();
    let account_id = super::token::account_id(ctx.secret).map(str::to_owned);
    let websocket = crate::channel::responses_websocket::is_target(&ctx.method, ctx.path);
    let path = ctx.path.strip_prefix("/v1").unwrap_or(ctx.path);
    let models_query =
        (path == "/models" || path.starts_with("/models/")).then(|| match ctx.query {
            Some(query) if !query.is_empty() => format!(
                "{query}&client_version={}",
                super::model_metadata::CODEX_VERSION
            ),
            _ => format!("client_version={}", super::model_metadata::CODEX_VERSION),
        });
    let query = models_query.as_deref().or(ctx.query);
    let uri = match endpoint_url(
        ctx.provider_settings,
        ctx.op,
        ctx.stream,
        ctx.upstream_model_id,
    ) {
        Some(url) => exact_url(&url, query)?,
        None => {
            let base = ctx
                .provider_settings
                .get("base_url")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|base| !base.is_empty())
                .unwrap_or(super::model_metadata::DEFAULT_BASE_URL);
            join_url(base, path, query)?
        }
    };
    let headers = allow_headers(
        ctx.headers,
        &[
            "x-codex-beta-features",
            "x-codex-turn-metadata",
            "x-codex-window-id",
            "thread-id",
            "session-id",
            "x-client-request-id",
        ],
    );
    let mut request = build_request(ctx.method, uri, headers, ctx.body)?;
    super::headers::apply(&mut request, &access_token, account_id.as_deref())?;
    if websocket {
        crate::channel::responses_websocket::apply_beta(request.headers_mut());
        *request.uri_mut() = crate::channel::responses_websocket::websocket_uri(request.uri())?;
        return crate::channel::responses_websocket::prepare(request);
    }
    Ok(PreparedRequest::new(request))
}
