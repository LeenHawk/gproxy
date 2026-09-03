use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use gproxy_protocol::{Operation, OperationKey, OperationKind, WireFamily};
use http::header::{AUTHORIZATION, HeaderName, HeaderValue};
use serde_json::Value;

const DEFAULT_BASE_URL: &str = "https://api.deepseek.com";
struct Target {
    method: http::Method,
    path: String,
    endpoint: &'static str,
}

pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let api_key = ctx
        .secret
        .get("api_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or_else(|| ChannelError::Secret("api_key missing".into()))?;
    let target = target(&ctx)?;
    let uri = endpoint(&ctx, &target)?;
    let mut headers = crate::policy::request_headers(crate::policy::DEEPSEEK, &ctx)?;
    apply_auth(&mut headers, ctx.key, api_key)?;
    let body = if matches!(ctx.key.kind(), OperationKind::Family(_)) {
        ctx.body.clone()
    } else {
        let body = super::model::rewrite(&ctx)?;
        if super::model::is_chat(ctx.key) {
            super::shape::request(&body)?
        } else {
            body
        }
    };
    let mut request = http::Request::builder()
        .method(target.method)
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

fn target(ctx: &PrepareCtx<'_>) -> Result<Target, ChannelError> {
    if ctx.key == OperationKey::family(Operation::ListModels, WireFamily::OpenAi) {
        return Ok(get("/v1/models".into(), "openai_list_models"));
    }
    if ctx.key == OperationKey::family(Operation::GetModel, WireFamily::OpenAi) {
        let model = required_model(ctx)?;
        return Ok(get(
            format!(
                "/v1/models/{}",
                crate::shared::http::encode_component(model)
            ),
            "openai_get_model",
        ));
    }
    let (path, endpoint) = if super::model::is_chat(ctx.key) {
        ("/v1/chat/completions", "openai_chat_completions")
    } else if super::model::is_responses(ctx.key) {
        ("/responses", "openai_responses")
    } else if super::model::is_claude(ctx.key) {
        ("/anthropic/v1/messages", "claude_messages")
    } else {
        return Err(ChannelError::Prepare(
            "operation is unsupported by DeepSeek".into(),
        ));
    };
    Ok(post(path.into(), endpoint))
}

fn endpoint(ctx: &PrepareCtx<'_>, target: &Target) -> Result<http::Uri, ChannelError> {
    if let Some(url) = ctx
        .provider_settings
        .get("endpoints")
        .and_then(|endpoints| endpoints.get(target.endpoint))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
    {
        let model = crate::shared::http::encode_component(ctx.upstream_model.trim());
        return crate::shared::http::exact(&url.replace("{model}", &model), None);
    }
    let base = ctx
        .provider_settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(DEFAULT_BASE_URL);
    crate::shared::http::join(base, &target.path, None)
}

fn apply_auth(
    headers: &mut http::HeaderMap,
    key: OperationKey,
    api_key: &str,
) -> Result<(), ChannelError> {
    let value = if super::model::is_claude(key) {
        api_key.to_owned()
    } else {
        format!("Bearer {api_key}")
    };
    let name = if super::model::is_claude(key) {
        HeaderName::from_static("x-api-key")
    } else {
        AUTHORIZATION
    };
    headers.insert(
        name,
        HeaderValue::from_str(&value)
            .map_err(|error| ChannelError::Secret(format!("api_key is invalid: {error}")))?,
    );
    Ok(())
}

fn required_model<'a>(ctx: &'a PrepareCtx<'_>) -> Result<&'a str, ChannelError> {
    (!ctx.upstream_model.trim().is_empty())
        .then_some(ctx.upstream_model.trim())
        .ok_or_else(|| ChannelError::Prepare("DeepSeek request has no model".into()))
}

fn get(path: String, endpoint: &'static str) -> Target {
    Target {
        method: http::Method::GET,
        path,
        endpoint,
    }
}

fn post(path: String, endpoint: &'static str) -> Target {
    Target {
        method: http::Method::POST,
        path,
        endpoint,
    }
}
