use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKind};
use serde_json::Value;

const CLI_BASE: &str = "https://cli-chat-proxy.grok.com/v1";
const MEDIA_BASE: &str = "https://api.x.ai/v1";

pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let public = is_public(ctx.key);
    let path = path(&ctx, public);
    let uri = endpoint(&ctx, &path, public)?;
    let mut headers = crate::shared::http::allow_headers(ctx.headers, &["accept", "content-type"]);
    let body = super::shape::request(&ctx, &mut headers)?;
    let session = serde_json::from_slice::<Value>(&body)
        .ok()
        .and_then(|value| value.get("prompt_cache_key")?.as_str().map(str::to_owned));
    super::auth::apply(
        &mut headers,
        ctx.secret,
        ctx.stream,
        ctx.key.operation == Operation::CreateSpeech,
        session.as_deref(),
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

fn endpoint(ctx: &PrepareCtx<'_>, path: &str, public: bool) -> Result<http::Uri, ChannelError> {
    if let Some(name) = endpoint_name(ctx.key)
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
        let url = video_id(ctx.path)
            .map(|id| url.replace("{video_id}", &crate::shared::http::encode_component(id)))
            .unwrap_or(url);
        return crate::shared::http::exact(&url, None);
    }
    let base = setting(ctx.provider_settings, "base_url").unwrap_or(if public {
        MEDIA_BASE
    } else {
        CLI_BASE
    });
    crate::shared::http::join(base, path, None)
}

fn video_id(path: &str) -> Option<&str> {
    path.strip_prefix("/v1/videos/")?
        .split('/')
        .next()
        .filter(|id| !id.is_empty())
}

fn path(ctx: &PrepareCtx<'_>, public: bool) -> String {
    if public {
        return match ctx.key.operation {
            Operation::CreateSpeech => "/tts".into(),
            Operation::CreateTranscription => "/stt".into(),
            Operation::CreateVideo => "/videos/generations".into(),
            _ => ctx.path.strip_prefix("/v1").unwrap_or(ctx.path).into(),
        };
    }
    if ctx.key.operation == Operation::GetModel && !ctx.upstream_model.is_empty() {
        return format!(
            "/models/{}",
            crate::shared::http::encode_component(ctx.upstream_model)
        );
    }
    ctx.path.strip_prefix("/v1").unwrap_or(ctx.path).into()
}

fn endpoint_name(key: gproxy_protocol::OperationKey) -> Option<&'static str> {
    if let OperationKind::ContentGeneration(kind) = key.kind {
        return match kind {
            ContentGenerationKind::OpenAiChat => Some("openai_chat_completions"),
            ContentGenerationKind::OpenAiResponses => Some("openai_responses"),
            _ => None,
        };
    }
    match key.operation {
        Operation::ListModels => Some("openai_list_models"),
        Operation::GetModel => Some("openai_get_model"),
        Operation::CompactContent => Some("openai_compact"),
        Operation::CreateImage => Some("image_generations"),
        Operation::EditImage => Some("image_edits"),
        Operation::CreateSpeech => Some("openai_audio_speech"),
        Operation::CreateTranscription => Some("openai_audio_transcriptions"),
        Operation::CreateVideo => Some("openai_video_create"),
        Operation::RetrieveVideo => Some("openai_video_retrieve"),
        Operation::EditVideo => Some("openai_video_edit"),
        Operation::ExtendVideo => Some("openai_video_extend"),
        _ => None,
    }
}

fn is_public(key: gproxy_protocol::OperationKey) -> bool {
    key.kind == OperationKind::Family(gproxy_protocol::WireFamily::OpenAi)
        && matches!(
            key.operation,
            Operation::CompactContent
                | Operation::CreateImage
                | Operation::EditImage
                | Operation::CreateSpeech
                | Operation::CreateTranscription
                | Operation::CreateVideo
                | Operation::RetrieveVideo
                | Operation::EditVideo
                | Operation::ExtendVideo
        )
}

fn setting<'a>(settings: &'a Value, name: &str) -> Option<&'a str> {
    settings
        .get(name)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}
