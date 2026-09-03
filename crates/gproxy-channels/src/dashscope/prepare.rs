use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, OperationKind, WireFamily};
use http::header::{AUTHORIZATION, HeaderValue};
use serde_json::Value;

const DEFAULT_BASE_URL: &str = "https://dashscope.aliyuncs.com";
const IMAGE_PATH: &str = "/api/v1/services/aigc/multimodal-generation/generation";

struct Target {
    method: http::Method,
    path: String,
    endpoint: &'static str,
}

pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    if ctx.stream && super::image::is_operation(ctx.key.operation()) {
        return Err(ChannelError::Prepare(
            "DashScope image generation is synchronous".into(),
        ));
    }
    let api_key = ctx
        .secret
        .get("api_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or_else(|| ChannelError::Secret("api_key missing".into()))?;
    let target = target(&ctx)?;
    let uri = endpoint(&ctx, &target)?;
    let mut headers = crate::policy::request_headers(crate::policy::DASHSCOPE, &ctx)?;
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {api_key}"))
            .map_err(|error| ChannelError::Secret(format!("api_key is invalid: {error}")))?,
    );
    let body = super::model::request_body(&ctx)?;
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
        return Ok(get(
            "/compatible-mode/v1/models".into(),
            "openai_list_models",
        ));
    }
    if ctx.key == OperationKey::family(Operation::GetModel, WireFamily::OpenAi) {
        let model = required_model(ctx.upstream_model)?;
        return Ok(get(
            format!(
                "/compatible-mode/v1/models/{}",
                crate::shared::http::encode_component(model)
            ),
            "openai_get_model",
        ));
    }
    if is_content(ctx.key, ContentGenerationKind::OpenAiChat) {
        return Ok(post(
            "/compatible-mode/v1/chat/completions".into(),
            "openai_chat_completions",
        ));
    }
    if is_content(ctx.key, ContentGenerationKind::OpenAiResponses) {
        return Ok(post(
            "/compatible-mode/v1/responses".into(),
            "openai_responses",
        ));
    }
    if is_content(ctx.key, ContentGenerationKind::ClaudeMessages) {
        return Ok(post(
            "/apps/anthropic/v1/messages".into(),
            "claude_messages",
        ));
    }
    let (path, endpoint) = match ctx.key.operation() {
        Operation::CreateEmbedding => {
            ("/compatible-mode/v1/embeddings".into(), "openai_embeddings")
        }
        Operation::Rerank => ("/compatible-api/v1/reranks".into(), "openai_rerank"),
        Operation::CreateImage => (IMAGE_PATH.into(), "image_generations"),
        Operation::EditImage => (IMAGE_PATH.into(), "image_edits"),
        _ => {
            return Err(ChannelError::Prepare(
                "operation is unsupported by DashScope".into(),
            ));
        }
    };
    Ok(post(path, endpoint))
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

fn required_model(model: &str) -> Result<&str, ChannelError> {
    (!model.trim().is_empty())
        .then_some(model.trim())
        .ok_or_else(|| ChannelError::Prepare("DashScope request has no model".into()))
}

fn is_content(key: OperationKey, kind: ContentGenerationKind) -> bool {
    key.kind() == OperationKind::ContentGeneration(kind)
        && matches!(
            key.operation(),
            Operation::GenerateContent | Operation::StreamGenerateContent
        )
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
