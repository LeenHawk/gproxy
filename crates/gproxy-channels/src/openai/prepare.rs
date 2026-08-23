use gproxy_channel_api::{ChannelError, PrepareCtx, PreparedRequest};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKind};
use http::header::{AUTHORIZATION, HeaderValue};
use http::{HeaderMap, Uri};

const DEFAULT_BASE_URL: &str = "https://api.openai.com";
const FORWARD_HEADERS: &[&str] = &[
    "content-type",
    "accept",
    "openai-beta",
    "openai-organization",
    "openai-project",
];
const FORWARD_QUERY: &[&str] = &["after", "limit", "order", "purpose", "variant"];

pub(super) fn request(ctx: PrepareCtx<'_>) -> Result<PreparedRequest, ChannelError> {
    let key = ctx
        .secret
        .get("api_key")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or_else(|| ChannelError::Secret("api_key missing".into()))?;
    let path = upstream_path(&ctx);
    let query = allow_query(ctx.query);
    let uri = endpoint(&ctx, &path, query.as_deref())?;
    let headers = allow_headers(ctx.headers);
    let body = super::model::shape(
        ctx.key,
        ctx.stream,
        ctx.upstream_model,
        ctx.headers,
        ctx.body,
    )?;
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
        websocket: false,
        profile: None,
    })
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
            replace_resource(url, ctx.key.operation, ctx.path)
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
        | SummarizeMemory
        | GenerateContent
        | StreamGenerateContent
        | Rerank
        | WebSearch
        | CreateRealtimeCall => None,
    }
}

fn replace_resource(mut url: String, operation: Operation, path: &str) -> String {
    let replacement = match operation {
        Operation::RetrieveFile | Operation::RetrieveFileContent | Operation::DeleteFile => {
            file_id(path).map(|id| ("{file_id}", id))
        }
        Operation::RetrieveVideo
        | Operation::DeleteVideo
        | Operation::DownloadVideoContent
        | Operation::RemixVideo => video_id(path).map(|id| ("{video_id}", id)),
        Operation::GetVideoCharacter => character_id(path).map(|id| ("{character_id}", id)),
        _ => None,
    };
    if let Some((slot, value)) = replacement {
        url = url.replace(slot, &encode_component(value));
    }
    url
}

fn file_id(path: &str) -> Option<&str> {
    path.strip_prefix("/v1/files/")?
        .strip_suffix("/content")
        .or_else(|| path.strip_prefix("/v1/files/"))
        .filter(|id| !id.is_empty() && !id.contains('/'))
}

fn video_id(path: &str) -> Option<&str> {
    path.strip_prefix("/v1/videos/")?
        .split('/')
        .next()
        .filter(|id| !id.is_empty())
}

fn character_id(path: &str) -> Option<&str> {
    path.strip_prefix("/v1/videos/characters/")
        .filter(|id| !id.is_empty() && !id.contains('/'))
}

fn allow_headers(source: &HeaderMap) -> HeaderMap {
    let mut headers = HeaderMap::new();
    for (name, value) in source {
        if FORWARD_HEADERS.contains(&name.as_str()) {
            headers.append(name.clone(), value.clone());
        }
    }
    headers
}

fn allow_query(query: Option<&str>) -> Option<String> {
    let kept = query?
        .split('&')
        .filter(|pair| FORWARD_QUERY.contains(&pair.split('=').next().unwrap_or("")))
        .collect::<Vec<_>>();
    (!kept.is_empty()).then(|| kept.join("&"))
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
