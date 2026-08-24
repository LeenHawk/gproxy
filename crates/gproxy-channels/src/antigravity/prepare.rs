use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use gproxy_protocol::{Operation, StreamFraming};
use http::header::{AUTHORIZATION, CONTENT_TYPE, HeaderValue, USER_AGENT};
use serde_json::Value;

const BASE_URL: &str = "https://daily-cloudcode-pa.googleapis.com";
const USER_AGENT_VALUE: &str = "antigravity/cli/1.0.6 linux/amd64";

pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let access = super::auth::access_token(ctx.secret)?;
    let project = super::auth::project_id(ctx.secret)?;
    let (endpoint, path, query, body) = match ctx.key.operation {
        Operation::ListModels => (
            "gemini_list_models",
            "/v1internal:fetchAvailableModels",
            None,
            Bytes::from_static(b"{}"),
        ),
        Operation::CountTokens => (
            "gemini_count_tokens",
            "/v1internal:countTokens",
            None,
            crate::shared::code_assist::wrap_count(ctx.body)?,
        ),
        Operation::GenerateContent | Operation::StreamGenerateContent => {
            let stream = ctx.key.operation == Operation::StreamGenerateContent;
            let body = crate::shared::gemini::model::rewrite(
                ctx.key.operation,
                ctx.body,
                ctx.upstream_model,
            )?;
            let body = crate::shared::code_assist::sanitize(&body)?;
            (
                if stream {
                    "gemini_stream_generate_content"
                } else {
                    "gemini_generate_content"
                },
                if stream {
                    "/v1internal:streamGenerateContent"
                } else {
                    "/v1internal:generateContent"
                },
                stream.then_some("alt=sse"),
                crate::shared::code_assist::wrap(&body, ctx.upstream_model, project)?,
            )
        }
        _ => {
            return Err(ChannelError::Prepare(
                "unsupported Antigravity operation".into(),
            ));
        }
    };
    let uri = endpoint_uri(&ctx, endpoint, path, query)?;
    let mut request = http::Request::builder()
        .method(http::Method::POST)
        .uri(crate::shared::http::strip_userinfo(uri)?)
        .body(body)
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    let headers = request.headers_mut();
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {access}"))
            .map_err(|error| ChannelError::Secret(error.to_string()))?,
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
    Ok(PreparedRequest {
        request,
        framing: (ctx.key.operation == Operation::StreamGenerateContent)
            .then_some(StreamFraming::Sse),
        websocket: false,
        profile: Some(&super::profile::PROFILE),
    })
}

fn endpoint_uri(
    ctx: &PrepareCtx<'_>,
    name: &str,
    path: &str,
    query: Option<&str>,
) -> Result<http::Uri, ChannelError> {
    if let Some(url) = ctx
        .provider_settings
        .get("endpoints")
        .and_then(|endpoints| endpoints.get(name))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
    {
        return crate::shared::http::exact(url, query);
    }
    let base = ctx
        .provider_settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(BASE_URL);
    crate::shared::http::join(base, path, query)
}
