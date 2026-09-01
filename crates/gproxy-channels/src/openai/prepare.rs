use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKind};
use http::Uri;
use http::header::{AUTHORIZATION, HeaderValue};

const DEFAULT_BASE_URL: &str = "https://api.openai.com";
pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let key = ctx
        .secret
        .get("api_key")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or_else(|| ChannelError::Secret("api_key missing".into()))?;
    let path = upstream_path(&ctx);
    let query = crate::policy::request_query(crate::policy::OPENAI_API, &ctx)?;
    let uri = endpoint(&ctx, &path, query.as_deref())?;
    let headers = crate::policy::request_headers(crate::policy::OPENAI_API, &ctx)?;
    let body = openai_cache(&ctx)?;
    let body = super::model::shape(ctx.key, ctx.stream, ctx.upstream_model, ctx.headers, &body)?;
    let mut request = http::Request::builder()
        .method(ctx.method)
        .uri(strip_userinfo(uri)?)
        .body(body)
        .map_err(|error| ChannelError::Prepare(error.to_string()))?;
    *request.headers_mut() = headers;
    request.headers_mut().insert(
        AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {key}"))
            .map_err(|error| ChannelError::Secret(format!("api_key is invalid: {error}")))?,
    );
    Ok(PreparedRequest {
        request,
        framing: None,
        websocket: false,
        profile: None,
    })
}

fn openai_cache(ctx: &PrepareCtx<'_>) -> Result<bytes::Bytes, ChannelError> {
    let OperationKind::ContentGeneration(kind) = ctx.key.kind else {
        return Ok(ctx.body.clone());
    };
    if ctx
        .provider_settings
        .get("enable_openai_magic_cache")
        .and_then(serde_json::Value::as_bool)
        != Some(true)
    {
        return Ok(ctx.body.clone());
    }
    let mut value = serde_json::from_slice(ctx.body)
        .map_err(|error| ChannelError::Prepare(format!("request body JSON: {error}")))?;
    crate::shared::openai::cache::apply(&mut value, kind);
    serde_json::to_vec(&value)
        .map(bytes::Bytes::from)
        .map_err(|error| ChannelError::Prepare(error.to_string()))
}

fn upstream_path(ctx: &PrepareCtx<'_>) -> String {
    if ctx.key.operation == Operation::GetModel && !ctx.upstream_model.is_empty() {
        format!("/v1/models/{}", encode_component(ctx.upstream_model))
    } else {
        ctx.path.to_owned()
    }
}

fn endpoint(ctx: &PrepareCtx<'_>, path: &str, query: Option<&str>) -> Result<Uri, ChannelError> {
    if let Some(url) = endpoint_override(ctx) {
        return exact_url(&url, query);
    }
    let base = ctx
        .provider_settings
        .get("base_url")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|base| !base.is_empty())
        .unwrap_or(DEFAULT_BASE_URL);
    absolute_url(&format!("{}{}", base.trim_end_matches('/'), path), query)
}

fn endpoint_override(ctx: &PrepareCtx<'_>) -> Option<String> {
    let name = endpoint_name(ctx.key, ctx.stream)?;
    ctx.provider_settings
        .get("endpoints")?
        .get(name)?
        .as_str()
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(|url| {
            let url = url.replace("{model}", &encode_component(ctx.upstream_model));
            crate::shared::openai::endpoint::replace_resource(url, ctx.key.operation, ctx.path)
        })
}

fn endpoint_name(key: gproxy_protocol::OperationKey, _stream: bool) -> Option<&'static str> {
    if let OperationKind::ContentGeneration(kind) = key.kind {
        return match kind {
            ContentGenerationKind::OpenAiChat => Some("openai_chat_completions"),
            ContentGenerationKind::OpenAiResponses => Some("openai_responses"),
            ContentGenerationKind::OpenAiResponsesWebSocket
            | ContentGenerationKind::ClaudeMessages
            | ContentGenerationKind::GeminiGenerateContent => None,
        };
    }
    use Operation::*;
    match key.operation {
        ListModels => Some("openai_list_models"),
        GetModel => Some("openai_get_model"),
        CompactContent => Some("openai_compact"),
        CreateEmbedding => Some("openai_embeddings"),
        CreateImage => Some("image_generations"),
        EditImage => Some("image_edits"),
        CreateSpeech => Some("openai_audio_speech"),
        CreateTranscription => Some("openai_audio_transcriptions"),
        CreateTranslation => Some("openai_audio_translations"),
        CreateFile => Some("openai_file_create"),
        ListFiles => Some("openai_file_list"),
        RetrieveFile => Some("openai_file_retrieve"),
        RetrieveFileContent => Some("openai_file_content"),
        DeleteFile => Some("openai_file_delete"),
        CreateVideo => Some("openai_video_create"),
        RetrieveVideo => Some("openai_video_retrieve"),
        ListVideos => Some("openai_video_list"),
        DeleteVideo => Some("openai_video_delete"),
        DownloadVideoContent => Some("openai_video_content"),
        RemixVideo => Some("openai_video_remix"),
        CreateVideoCharacter => Some("openai_video_character_create"),
        GetVideoCharacter => Some("openai_video_character_get"),
        EditVideo => Some("openai_video_edit"),
        ExtendVideo => Some("openai_video_extend"),
        CountTokens
        | BatchCreateEmbedding
        | SummarizeMemory
        | GenerateContent
        | StreamGenerateContent
        | GuardianReview
        | GuardianClassify
        | Rerank
        | WebSearch
        | CreateRealtimeCall
        | ConnectRealtime => None,
    }
}

fn absolute_url(url: &str, query: Option<&str>) -> Result<Uri, ChannelError> {
    let uri: Uri = url
        .parse()
        .map_err(|error| ChannelError::Prepare(format!("bad upstream URL: {error}")))?;
    exact_uri(uri, query)
}

fn exact_url(url: &str, query: Option<&str>) -> Result<Uri, ChannelError> {
    let uri: Uri = url
        .parse()
        .map_err(|error| ChannelError::Prepare(format!("bad endpoint override: {error}")))?;
    exact_uri(uri, query)
}

fn exact_uri(mut uri: Uri, query: Option<&str>) -> Result<Uri, ChannelError> {
    if uri.scheme().is_none() || uri.authority().is_none() {
        return Err(ChannelError::Prepare(
            "upstream URL must be absolute".into(),
        ));
    }
    let Some(query) = query.filter(|query| !query.is_empty()) else {
        return Ok(uri);
    };
    let merged = match uri.query() {
        Some(existing) => format!("{}?{existing}&{query}", uri.path()),
        None => format!("{}?{query}", uri.path()),
    };
    let mut parts = uri.into_parts();
    parts.path_and_query = Some(
        merged
            .parse()
            .map_err(|error| ChannelError::Prepare(format!("bad endpoint query: {error}")))?,
    );
    uri = Uri::from_parts(parts)
        .map_err(|error| ChannelError::Prepare(format!("bad endpoint: {error}")))?;
    Ok(uri)
}

fn strip_userinfo(uri: Uri) -> Result<Uri, ChannelError> {
    let Some(authority) = uri.authority() else {
        return Ok(uri);
    };
    if !authority.as_str().contains('@') {
        return Ok(uri);
    }
    let clean = authority.port_u16().map_or_else(
        || authority.host().to_owned(),
        |port| format!("{}:{port}", authority.host()),
    );
    let mut parts = uri.into_parts();
    parts.authority = Some(
        clean
            .parse()
            .map_err(|error| ChannelError::Prepare(format!("bad authority: {error}")))?,
    );
    Uri::from_parts(parts).map_err(|error| ChannelError::Prepare(error.to_string()))
}

fn encode_component(value: &str) -> String {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push('%');
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
    }
    encoded
}
