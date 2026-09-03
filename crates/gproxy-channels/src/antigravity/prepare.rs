use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use gproxy_protocol::{Operation, StreamFraming};
use http::header::{AUTHORIZATION, CONTENT_TYPE, HeaderValue, USER_AGENT};
use serde_json::Value;

const BASE_URL: &str = "https://daily-cloudcode-pa.googleapis.com";
pub(super) const USER_AGENT_VALUE: &str = "antigravity/cli/1.0.6 linux/amd64";

pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let access = super::auth::access_token(ctx.secret)?;
    let project = super::auth::project_id(ctx.secret)?;
    let (endpoint, path, query, body, framing) = match ctx.key.operation {
        Operation::ListModels => (
            "gemini_list_models",
            "/v1internal:fetchAvailableModels",
            None,
            Bytes::from_static(b"{}"),
            None,
        ),
        Operation::CountTokens => (
            "gemini_count_tokens",
            "/v1internal:countTokens",
            None,
            crate::shared::code_assist::wrap_count(ctx.body)?,
            None,
        ),
        Operation::GenerateContent | Operation::StreamGenerateContent => {
            let stream = ctx.key.operation == Operation::StreamGenerateContent;
            let body = crate::shared::gemini::model::rewrite(
                ctx.key.operation,
                ctx.body,
                ctx.upstream_model,
            )?;
            let body = crate::shared::code_assist::sanitize(&body)?;
            let body = apply_model_defaults(&body, ctx.upstream_model)?;
            let buffered = stream && buffered_claude_flash(&ctx);
            (
                if stream && !buffered {
                    "gemini_stream_generate_content"
                } else {
                    "gemini_generate_content"
                },
                if stream && !buffered {
                    "/v1internal:streamGenerateContent"
                } else {
                    "/v1internal:generateContent"
                },
                (stream && !buffered).then_some("alt=sse"),
                crate::shared::code_assist::wrap(&body, ctx.upstream_model, project)?,
                stream.then_some(if buffered {
                    StreamFraming::JsonArray
                } else {
                    StreamFraming::Sse
                }),
            )
        }
        _ => {
            return Err(ChannelError::Prepare(
                "unsupported Antigravity operation".into(),
            ));
        }
    };
    let caller_query = crate::policy::request_query(crate::policy::ANTIGRAVITY, &ctx)?;
    let query = crate::shared::http::merge_query(caller_query.as_deref(), query);
    let uri = endpoint_uri(ctx.provider_settings, endpoint, path, query.as_deref())?;
    let mut headers = crate::policy::request_headers(crate::policy::ANTIGRAVITY, &ctx)?;
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {access}"))
            .map_err(|error| ChannelError::Secret(error.to_string()))?,
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(USER_AGENT, HeaderValue::from_static(USER_AGENT_VALUE));
    let mut request = http::Request::builder()
        .method(http::Method::POST)
        .uri(crate::shared::http::strip_userinfo(uri)?)
        .body(body)
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    *request.headers_mut() = headers;
    Ok(PreparedRequest {
        request,
        framing,
        websocket: false,
        profile: Some(&super::profile::PROFILE),
    })
}

fn buffered_claude_flash(ctx: &PrepareCtx<'_>) -> bool {
    crate::shared::gemini::model::model_id(ctx.upstream_model) == "gemini-2.5-flash"
        && ctx
            .headers
            .get(http::header::USER_AGENT)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("claude-cli/"))
}

pub(super) fn apply_model_defaults(body: &Bytes, model: &str) -> Result<Bytes, ChannelError> {
    if crate::shared::gemini::model::model_id(model) != "gemini-3.1-pro-high" {
        return Ok(body.clone());
    }
    let mut value: Value = serde_json::from_slice(body)
        .map_err(|error| ChannelError::Prepare(format!("Gemini body JSON: {error}")))?;
    let object = value
        .as_object_mut()
        .ok_or_else(|| ChannelError::Prepare("Gemini body must be an object".into()))?;
    let config = object
        .entry("generationConfig")
        .or_insert_with(|| Value::Object(serde_json::Map::new()))
        .as_object_mut()
        .ok_or_else(|| ChannelError::Prepare("generationConfig must be an object".into()))?;
    let thinking = config
        .entry("thinkingConfig")
        .or_insert_with(|| Value::Object(serde_json::Map::new()))
        .as_object_mut()
        .ok_or_else(|| ChannelError::Prepare("thinkingConfig must be an object".into()))?;
    // Antigravity exposes the high reasoning tier as a distinct model. Its
    // catalogue supplies 10001 as the default, while explicit valid budgets
    // remain caller-controlled.
    thinking
        .entry("thinkingBudget")
        .or_insert_with(|| Value::from(10_001));
    serde_json::to_vec(&value)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

pub(super) fn endpoint_uri(
    settings: &Value,
    name: &str,
    path: &str,
    query: Option<&str>,
) -> Result<http::Uri, ChannelError> {
    if let Some(url) = settings
        .get("endpoints")
        .and_then(|endpoints| endpoints.get(name))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
    {
        return crate::shared::http::exact(url, query);
    }
    let base = settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(BASE_URL);
    crate::shared::http::join(base, path, query)
}
