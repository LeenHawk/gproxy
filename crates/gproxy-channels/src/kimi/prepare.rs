use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use gproxy_protocol::Operation;
use serde_json::Value;

pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let mode = super::auth::mode(ctx.secret);
    let path = super::model::path(&ctx, mode);
    let query = if ctx.key.operation() == Operation::ListModels {
        crate::policy::request_query(crate::policy::KIMI, &ctx)?
    } else {
        None
    };
    let uri = endpoint(&ctx, &path, query.as_deref())?;
    let mut headers = crate::policy::request_headers(crate::policy::KIMI, &ctx)?;
    let body = super::model::body(&ctx)?;
    super::auth::apply(
        &mut headers,
        ctx.secret,
        super::model::is_anthropic(ctx.key),
        ctx.method,
    )?;
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

fn endpoint(
    ctx: &PrepareCtx<'_>,
    path: &str,
    query: Option<&str>,
) -> Result<http::Uri, ChannelError> {
    if let Some(name) = super::model::endpoint(ctx.key)
        && let Some(url) = ctx
            .provider_settings
            .get("endpoints")
            .and_then(|endpoints| endpoints.get(name))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|url| !url.is_empty())
    {
        let url = url.replace(
            "{model}",
            &crate::shared::http::encode_component(ctx.upstream_model),
        );
        return crate::shared::http::exact(&url, query);
    }
    crate::shared::http::join(
        super::auth::base_url(ctx.provider_settings, ctx.secret),
        path,
        query,
    )
}
