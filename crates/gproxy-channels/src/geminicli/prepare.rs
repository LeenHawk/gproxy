use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use gproxy_protocol::{Operation, StreamFraming};
use http::header::{ACCEPT, AUTHORIZATION, CONTENT_TYPE, HeaderName, HeaderValue, USER_AGENT};
use serde_json::{Value, json};

const BASE_URL: &str = "https://cloudcode-pa.googleapis.com";

pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let access = super::auth::access_token(ctx.secret)?;
    let project = super::auth::project_id(ctx.secret)?;
    let (endpoint, path, query, method, body) = match ctx.key.operation() {
        Operation::ListModels => (
            "gemini_list_models",
            "/v1internal:retrieveUserQuota",
            None,
            http::Method::POST,
            Bytes::from(json!({"project":project}).to_string()),
        ),
        Operation::CountTokens => (
            "gemini_count_tokens",
            "/v1internal:countTokens",
            None,
            http::Method::POST,
            crate::shared::code_assist::wrap_count(ctx.body)?,
        ),
        Operation::GenerateContent | Operation::StreamGenerateContent => {
            let stream = ctx.key.operation() == Operation::StreamGenerateContent;
            let body = crate::shared::gemini::model::rewrite(
                ctx.key.operation(),
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
                http::Method::POST,
                crate::shared::code_assist::wrap(&body, ctx.upstream_model, project)?,
            )
        }
        _ => {
            return Err(ChannelError::Prepare(
                "unsupported Gemini CLI operation".into(),
            ));
        }
    };
    let caller_query = crate::policy::request_query(crate::policy::GEMINI_CLI, &ctx)?;
    let query = crate::shared::http::merge_query(caller_query.as_deref(), query);
    let uri = endpoint_uri(ctx.provider_settings, endpoint, path, query.as_deref())?;
    let mut headers = crate::policy::request_headers(crate::policy::GEMINI_CLI, &ctx)?;
    apply_headers(
        &mut headers,
        access,
        ctx.upstream_model,
        ctx.key.operation() == Operation::ListModels,
    )?;
    let mut request = http::Request::builder()
        .method(method)
        .uri(crate::shared::http::strip_userinfo(uri)?)
        .body(body)
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    *request.headers_mut() = headers;
    Ok(PreparedRequest {
        request,
        framing: (ctx.key.operation() == Operation::StreamGenerateContent)
            .then_some(StreamFraming::Sse),
        websocket: false,
        profile: Some(&super::profile::PROFILE),
    })
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

pub(super) fn apply_headers(
    headers: &mut http::HeaderMap,
    token: &str,
    model: &str,
    quota: bool,
) -> Result<(), ChannelError> {
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {token}"))
            .map_err(|error| ChannelError::Secret(error.to_string()))?,
    );
    headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    headers.insert(
        ACCEPT,
        HeaderValue::from_static(if quota { "application/json" } else { "*/*" }),
    );
    headers.insert(
        USER_AGENT,
        HeaderValue::from_str(&user_agent(model))
            .map_err(|error| ChannelError::Prepare(error.to_string()))?,
    );
    if !quota {
        headers.insert(
            HeaderName::from_static("x-goog-api-client"),
            HeaderValue::from_static("gl-node/22.20.0"),
        );
    }
    Ok(())
}

fn user_agent(model: &str) -> String {
    let suffix = if model.trim().is_empty() {
        String::new()
    } else {
        format!("/{}", model.trim())
    };
    format!("GeminiCLI-tui/0.55.1{suffix} (linux; x64; terminal) google-api-nodejs-client/10.9.0")
}
