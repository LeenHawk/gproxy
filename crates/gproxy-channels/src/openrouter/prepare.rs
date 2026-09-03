use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKind};
use http::header::{AUTHORIZATION, HeaderValue};
use serde_json::Value;

const DEFAULT_BASE_URL: &str = "https://openrouter.ai/api";
pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let key = ctx
        .secret
        .get("api_key")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or_else(|| ChannelError::Secret("api_key missing".into()))?;
    let path = upstream_path(&ctx);
    let query = allow_query(&ctx)?;
    let uri = endpoint(&ctx, &path, query.as_deref())?;
    let mut headers = crate::policy::request_headers(crate::policy::OPENROUTER, &ctx)?;
    let body = super::shape::request(&ctx, &mut headers)?;
    headers.insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {key}"))
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
    match ctx.key.operation() {
        Operation::GetModel if !ctx.upstream_model.is_empty() => format!(
            "/v1/models/{}",
            crate::shared::http::encode_component(ctx.upstream_model)
        ),
        Operation::CreateImage | Operation::EditImage => "/v1/images".into(),
        _ => ctx.path.into(),
    }
}

fn endpoint(
    ctx: &PrepareCtx<'_>,
    path: &str,
    query: Option<&str>,
) -> Result<http::Uri, ChannelError> {
    if let Some(exact) = endpoint_override(ctx) {
        return crate::shared::http::exact(&exact, query);
    }
    let base = ctx
        .provider_settings
        .get("base_url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(DEFAULT_BASE_URL);
    crate::shared::http::join(base, path, query)
}

fn endpoint_override(ctx: &PrepareCtx<'_>) -> Option<String> {
    let name = endpoint_name(ctx.key)?;
    let url = ctx
        .provider_settings
        .get("endpoints")?
        .get(name)?
        .as_str()?
        .trim();
    (!url.is_empty()).then(|| {
        let model = crate::shared::http::encode_component(ctx.upstream_model);
        let url = url.replace("{model}", &model);
        video_id(ctx.path)
            .map(|id| url.replace("{video_id}", &crate::shared::http::encode_component(id)))
            .unwrap_or(url)
    })
}

fn endpoint_name(key: gproxy_protocol::OperationKey) -> Option<&'static str> {
    if let OperationKind::ContentGeneration(kind) = key.kind() {
        return match kind {
            ContentGenerationKind::OpenAiChat => Some("openai_chat_completions"),
            ContentGenerationKind::OpenAiResponses => Some("openai_responses"),
            ContentGenerationKind::ClaudeMessages => Some("claude_messages"),
            ContentGenerationKind::OpenAiResponsesWebSocket
            | ContentGenerationKind::GeminiGenerateContent => None,
        };
    }
    match key.operation() {
        Operation::ListModels => Some("openai_list_models"),
        Operation::GetModel => Some("openai_get_model"),
        Operation::CreateEmbedding => Some("openai_embeddings"),
        Operation::Rerank => Some("openai_rerank"),
        Operation::CreateImage => Some("image_generations"),
        Operation::EditImage => Some("image_edits"),
        Operation::CreateSpeech => Some("openai_audio_speech"),
        Operation::CreateTranscription => Some("openai_audio_transcriptions"),
        Operation::CreateVideo => Some("openai_video_create"),
        Operation::RetrieveVideo => Some("openai_video_retrieve"),
        Operation::DownloadVideoContent => Some("openai_video_content"),
        _ => None,
    }
}

fn allow_query(ctx: &PrepareCtx<'_>) -> Result<Option<String>, ChannelError> {
    if matches!(
        ctx.key.operation(),
        Operation::ListModels | Operation::DownloadVideoContent
    ) {
        crate::policy::request_query(crate::policy::OPENROUTER, ctx)
    } else {
        Ok(None)
    }
}

fn video_id(path: &str) -> Option<&str> {
    path.strip_prefix("/v1/videos/")?
        .split('/')
        .next()
        .filter(|id| !id.is_empty())
}
