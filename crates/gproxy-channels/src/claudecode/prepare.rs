use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest, SurfaceRequest};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};
use http::{HeaderMap, Method};
use serde_json::Value;

pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let token = super::auth::access_token(ctx.secret)?;
    let session_id = super::auth::session_id(ctx.secret, ctx.headers);
    let mut headers = crate::policy::request_headers(crate::policy::CLAUDE_CODE, &ctx)?;
    let body = shape_body(&ctx, &mut headers, &session_id)?;
    let (method, path) = upstream_target(ctx.key, ctx.upstream_model)?;
    let query = query(&ctx)?;
    let uri = endpoint(&ctx, &path, query.as_deref())?;
    super::auth::apply_headers(
        &mut headers,
        token,
        &session_id,
        ctx.headers
            .get(http::header::USER_AGENT)
            .and_then(|value| value.to_str().ok()),
    )?;
    let mut request = http::Request::builder()
        .method(method)
        .uri(crate::shared::http::strip_userinfo(uri)?)
        .body(body)
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    *request.headers_mut() = headers;
    Ok(PreparedRequest {
        request,
        framing: None,
        websocket: false,
        profile: Some(&super::profile::CLIENT_PROFILE),
    })
}

pub(super) fn surface(
    source: &SurfaceRequest,
    websocket: bool,
    provider_settings: &Value,
    secret: &Value,
) -> Result<PreparedRequest, ChannelError> {
    let token = super::auth::access_token(secret)?;
    let session_id = super::auth::session_id(secret, &source.headers);
    let policy = crate::policy::CLAUDE_CODE
        .effective_traffic_policy(provider_settings)
        .map_err(ChannelError::Prepare)?;
    let mut headers = resource_headers(
        policy.filter_request_headers(&source.headers),
        &source.upstream_path,
    );
    let content_type = headers.get(http::header::CONTENT_TYPE).cloned();
    let accept = headers.get(http::header::ACCEPT).cloned();
    let caller_query = policy.filter_request_query(source.query.as_deref());
    let query = surface_query(&source.upstream_path, caller_query.as_deref());
    let base = provider_settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(super::auth::DEFAULT_BASE_URL);
    let uri = crate::shared::http::join(base, &source.upstream_path, query.as_deref())?;
    super::auth::apply_headers(
        &mut headers,
        token,
        &session_id,
        source
            .headers
            .get(http::header::USER_AGENT)
            .and_then(|value| value.to_str().ok()),
    )?;
    if let Some(content_type) = content_type {
        headers.insert(http::header::CONTENT_TYPE, content_type);
    }
    if let Some(accept) = accept {
        headers.insert(http::header::ACCEPT, accept);
    }
    let mut request = http::Request::builder()
        .method(&source.method)
        .uri(crate::shared::http::strip_userinfo(uri)?)
        .body(source.body.clone())
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    *request.headers_mut() = headers;
    Ok(PreparedRequest {
        request,
        framing: None,
        websocket,
        profile: Some(&super::profile::CLIENT_PROFILE),
    })
}

fn shape_body(
    ctx: &PrepareCtx<'_>,
    headers: &mut HeaderMap,
    session_id: &str,
) -> Result<Bytes, ChannelError> {
    if !is_messages(ctx.key) && !is_count_tokens(ctx.key) {
        return Ok(ctx.body.clone());
    }
    let mut body = super::hygiene::json_object(ctx.body)?;
    if !ctx.upstream_model.is_empty() {
        body.as_object_mut()
            .expect("JSON object was validated")
            .insert("model".into(), Value::String(ctx.upstream_model.into()));
    }
    if is_messages(ctx.key) {
        super::hygiene::messages(&mut body, headers);
        crate::shared::claude::fallback::apply(&mut body, headers, ctx.provider_settings);
        if !gproxy_channel_api::has_fallback_credit(&body) {
            super::cch::inject(&mut body, ctx.secret, session_id);
        }
    } else {
        super::hygiene::count_tokens(&body, headers);
    }
    serde_json::to_vec(&body)
        .map(Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

fn upstream_target(
    key: OperationKey,
    model: &str,
) -> Result<(&'static Method, String), ChannelError> {
    if key == family(Operation::ListModels) {
        Ok((&Method::GET, "/v1/models".into()))
    } else if key == family(Operation::GetModel) {
        Ok((
            &Method::GET,
            format!(
                "/v1/models/{}",
                crate::shared::http::encode_component(model)
            ),
        ))
    } else if key == family(Operation::CountTokens) {
        Ok((&Method::POST, "/v1/messages/count_tokens".into()))
    } else if is_messages(key) {
        Ok((&Method::POST, "/v1/messages".into()))
    } else {
        Err(ChannelError::Prepare(
            "operation is unsupported by Claude Code".into(),
        ))
    }
}

fn endpoint(
    ctx: &PrepareCtx<'_>,
    path: &str,
    query: Option<&str>,
) -> Result<http::Uri, ChannelError> {
    if let Some(url) = endpoint_override(ctx) {
        return crate::shared::http::exact(&url, query);
    }
    let base = ctx
        .provider_settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(super::auth::DEFAULT_BASE_URL);
    crate::shared::http::join(base, path, query)
}

fn endpoint_override(ctx: &PrepareCtx<'_>) -> Option<String> {
    let name = if ctx.key == family(Operation::ListModels) {
        "claude_list_models"
    } else if ctx.key == family(Operation::GetModel) {
        "claude_get_model"
    } else if ctx.key == family(Operation::CountTokens) {
        "claude_count_tokens"
    } else if is_messages(ctx.key) {
        "claude_messages"
    } else {
        return None;
    };
    ctx.provider_settings
        .get("endpoints")?
        .get(name)?
        .as_str()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(|url| {
            url.replace(
                "{model}",
                &crate::shared::http::encode_component(ctx.upstream_model),
            )
        })
}

fn query(ctx: &PrepareCtx<'_>) -> Result<Option<String>, ChannelError> {
    let query = crate::policy::request_query(crate::policy::CLAUDE_CODE, ctx)?;
    let mut kept = query
        .as_deref()
        .unwrap_or_default()
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if (is_messages(ctx.key) || is_count_tokens(ctx.key))
        && !kept
            .iter()
            .any(|pair| pair.split('=').next() == Some("beta"))
    {
        kept.insert(0, "beta=true".into());
    }
    Ok((!kept.is_empty()).then(|| kept.join("&")))
}

fn resource_headers(mut headers: HeaderMap, path: &str) -> HeaderMap {
    let beta = if path.starts_with("/v1/skills") {
        "skills-2025-10-02"
    } else {
        "files-api-2025-04-14"
    };
    append_beta(&mut headers, beta);
    headers
}

fn surface_query(path: &str, query: Option<&str>) -> Option<String> {
    let mut kept = query
        .unwrap_or_default()
        .split('&')
        .filter(|part| !part.is_empty() && part.split('=').next() != Some("key"))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    if path.starts_with("/v1/skills") && !kept.iter().any(|part| part == "beta=true") {
        kept.insert(0, "beta=true".into());
    }
    (!kept.is_empty()).then(|| kept.join("&"))
}

fn append_beta(headers: &mut HeaderMap, beta: &str) {
    let existing = headers
        .get("anthropic-beta")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    let mut values = existing
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    if !values.contains(&beta) {
        values.push(beta);
    }
    if let Ok(value) = http::HeaderValue::from_str(&values.join(",")) {
        headers.insert("anthropic-beta", value);
    }
}

fn family(operation: Operation) -> OperationKey {
    OperationKey::family(operation, WireFamily::Claude)
}

fn is_count_tokens(key: OperationKey) -> bool {
    key == family(Operation::CountTokens)
}

fn is_messages(key: OperationKey) -> bool {
    key.kind()
        == gproxy_protocol::OperationKind::ContentGeneration(ContentGenerationKind::ClaudeMessages)
        && matches!(
            key.operation(),
            Operation::GenerateContent | Operation::StreamGenerateContent
        )
}
