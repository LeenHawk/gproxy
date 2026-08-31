use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKey, WireFamily};
use http::header::{AUTHORIZATION, CONTENT_TYPE, HeaderValue};
use serde_json::Value;

const DEFAULT_BASE_URL: &str = "https://integrate.api.nvidia.com";
pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let api_key = ctx
        .secret
        .get("api_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or_else(|| ChannelError::Secret("api_key missing".into()))?;
    let path = upstream_path(&ctx);
    let uri = endpoint(&ctx, &path)?;
    let body = super::model::rewrite(&ctx)?;
    let mut headers = crate::policy::request_headers(crate::policy::OPENAI_COMPATIBLE, &ctx)?;
    if !body.is_empty() && !headers.contains_key(CONTENT_TYPE) {
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
    }
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {api_key}"))
            .map_err(|error| ChannelError::Secret(format!("api_key is invalid: {error}")))?,
    );
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

fn upstream_path(ctx: &PrepareCtx<'_>) -> String {
    if ctx.key == family(Operation::GetModel) && !ctx.upstream_model.is_empty() {
        format!(
            "/v1/models/{}",
            crate::shared::http::encode_component(ctx.upstream_model)
        )
    } else {
        ctx.path.to_owned()
    }
}

fn endpoint(ctx: &PrepareCtx<'_>, path: &str) -> Result<http::Uri, ChannelError> {
    if let Some(url) = endpoint_override(ctx) {
        return crate::shared::http::exact(&url, None);
    }
    let base = ctx
        .provider_settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(DEFAULT_BASE_URL);
    crate::shared::http::join(base, path, None)
}

fn endpoint_override(ctx: &PrepareCtx<'_>) -> Option<String> {
    let name = endpoint_name(ctx.key)?;
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

fn endpoint_name(key: OperationKey) -> Option<&'static str> {
    if key == family(Operation::ListModels) {
        Some("openai_list_models")
    } else if key == family(Operation::GetModel) {
        Some("openai_get_model")
    } else if key == family(Operation::CreateEmbedding) {
        Some("openai_embeddings")
    } else if is_chat(key) {
        Some("openai_chat_completions")
    } else {
        None
    }
}

fn family(operation: Operation) -> OperationKey {
    OperationKey::family(operation, WireFamily::OpenAi)
}

fn is_chat(key: OperationKey) -> bool {
    key.kind == gproxy_protocol::OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChat)
        && matches!(
            key.operation,
            Operation::GenerateContent | Operation::StreamGenerateContent
        )
}
