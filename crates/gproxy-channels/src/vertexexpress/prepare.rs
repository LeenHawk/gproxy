use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use gproxy_protocol::{Operation, StreamFraming};
use http::header::{CONTENT_TYPE, HeaderValue};
use serde_json::Value;

const DEFAULT_BASE_URL: &str = "https://aiplatform.googleapis.com";

pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let api_key = ctx
        .secret
        .get("api_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or_else(|| ChannelError::Secret("api_key missing".into()))?;
    let target = super::model::target(&ctx)?;
    let query = query(&ctx, api_key)?;
    let uri = endpoint(&ctx, &target, Some(&query))?;
    let body = super::shape::request(&ctx)?;
    let mut request = http::Request::builder()
        .method(http::Method::POST)
        .uri(crate::shared::http::strip_userinfo(uri)?)
        .body(body)
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    request
        .headers_mut()
        .insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    Ok(PreparedRequest {
        request,
        framing: framing(&ctx),
        websocket: false,
        profile: None,
    })
}

fn endpoint(
    ctx: &PrepareCtx<'_>,
    target: &super::model::Target,
    query: Option<&str>,
) -> Result<http::Uri, ChannelError> {
    if let Some(url) = ctx
        .provider_settings
        .get("endpoints")
        .and_then(|endpoints| endpoints.get(target.endpoint))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
    {
        if url
            .split_once('?')
            .is_some_and(|(_, query)| query.split('&').any(|part| part.starts_with("key=")))
        {
            return Err(ChannelError::Prepare(
                "Vertex Express endpoint override must not embed an API key".into(),
            ));
        }
        let model = crate::shared::http::encode_component(crate::shared::gemini::model::model_id(
            ctx.upstream_model,
        ));
        return crate::shared::http::exact(&url.replace("{model}", &model), query);
    }
    let base = ctx
        .provider_settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(DEFAULT_BASE_URL);
    crate::shared::http::join(base, &target.path, query)
}

fn query(ctx: &PrepareCtx<'_>, api_key: &str) -> Result<String, ChannelError> {
    let caller_query = crate::policy::request_query(crate::policy::VERTEX_EXPRESS, ctx)?;
    let mut parts = caller_query
        .as_deref()
        .unwrap_or_default()
        .split('&')
        .filter(|part| !part.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    parts.push(format!(
        "key={}",
        crate::shared::http::encode_component(api_key)
    ));
    Ok(parts.join("&"))
}

fn framing(ctx: &PrepareCtx<'_>) -> Option<StreamFraming> {
    (ctx.key.operation == Operation::StreamGenerateContent).then(|| {
        if ctx
            .query
            .is_some_and(|query| query.split('&').any(|part| part == "alt=sse"))
        {
            StreamFraming::Sse
        } else {
            StreamFraming::JsonArray
        }
    })
}
