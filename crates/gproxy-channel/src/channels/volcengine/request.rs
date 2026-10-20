use super::{Config, DEFAULT_BASE_URL, Volcengine, audio, video};
use crate::channel::{
    BaseChannel, ChannelError, ChannelHeaders, HeaderAllowlist, PrepareContext, forwardable,
};
use crate::channels::shared::compatible::http::{insert_configured, strip_query_auth};
use gproxy_protocol::{Dialect, HttpBody, Operation};
use http::{HeaderValue, header};

pub(super) fn prepare(
    channel: &Volcengine,
    ctx: PrepareContext<'_>,
) -> Result<http::Request<HttpBody>, ChannelError> {
    let operation = ctx.operation.operation;
    if !channel
        .native_dialects(ctx.provider, operation)
        .contains(&ctx.operation.dialect)
    {
        return Err(ChannelError::UnsupportedOperation(ctx.operation));
    }
    let config = Config::read(ctx.provider)?;
    let key = ctx
        .credential
        .secret
        .get("api_key")
        .and_then(serde_json::Value::as_str)
        .filter(|key| !key.trim().is_empty())
        .ok_or(ChannelError::InvalidCredential)?;
    let suffix = match operation {
        Operation::GenerateContent | Operation::StreamGenerateContent => {
            match ctx.operation.dialect {
                Dialect::Claude => "/v1/messages",
                Dialect::OpenAi => "/v3/responses",
                _ => "/v3/chat/completions",
            }
            .to_owned()
        }
        Operation::CreateEmbedding => "/v3/embeddings".into(),
        Operation::CreateImage => "/v3/images/generations".into(),
        Operation::CreateVideo | Operation::ListVideos => "/v3/contents/generations/tasks".into(),
        Operation::RetrieveVideo | Operation::DeleteVideo | Operation::DownloadVideoContent => {
            format!(
                "/v3/contents/generations/tasks/{}",
                video::task_id(&ctx.request.path)?
            )
        }
        Operation::CreateSpeech => String::new(),
        _ => return Err(ChannelError::UnsupportedOperation(ctx.operation)),
    };
    let mut url = ctx.endpoint_override.map(str::to_owned).unwrap_or_else(|| {
        if operation == Operation::CreateSpeech {
            format!(
                "{}/api/v3/plan/tts/unidirectional",
                config.speech_base_url.trim_end_matches('/')
            )
        } else {
            format!(
                "{}{}{suffix}",
                ctx.provider
                    .base_url
                    .unwrap_or(DEFAULT_BASE_URL)
                    .trim_end_matches('/'),
                channel.prefix()
            )
        }
    });
    if let Some(query) = ctx
        .request
        .query
        .as_deref()
        .map(strip_query_auth)
        .filter(|q| !q.is_empty())
    {
        url.push(if url.contains('?') { '&' } else { '?' });
        url.push_str(&query);
    }
    let allowlist = HeaderAllowlist::from_view_for(
        ctx.provider,
        ChannelHeaders::native(ctx.operation.dialect),
    )?;
    let mut headers = forwardable(
        &ctx.request.headers,
        allowlist.as_ref(),
        &["x-api-resource-id", "x-security-token"],
    );
    for (name, value) in &config.headers {
        insert_configured(&mut headers, name, value)?;
    }
    // Authentication is always supplied by the selected credential.
    headers.remove("authorization");
    headers.remove("x-api-key");
    headers.remove("x-security-token");
    if operation == Operation::CreateSpeech {
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(key).map_err(|_| ChannelError::InvalidCredential)?,
        );
        headers.insert(
            "x-api-resource-id",
            HeaderValue::from_static("seed-tts-2.0"),
        );
        headers.insert(
            "x-control-require-usage-tokens-return",
            HeaderValue::from_static("text_words"),
        );
    } else {
        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {key}"))
                .map_err(|_| ChannelError::InvalidCredential)?,
        );
    }
    let body = match (operation, ctx.request.body) {
        (Operation::CreateVideo, HttpBody::Bytes(bytes)) => {
            HttpBody::Bytes(video::request(&bytes)?.into())
        }
        (Operation::CreateSpeech, HttpBody::Bytes(bytes)) => {
            HttpBody::Bytes(audio::request(&bytes)?.into())
        }
        (Operation::CreateVideo | Operation::CreateSpeech, HttpBody::Stream(_)) => {
            return Err(ChannelError::InvalidConfig(
                "Ark video and speech require a buffered JSON request".into(),
            ));
        }
        (_, body) => body,
    };
    headers
        .entry(header::CONTENT_TYPE)
        .or_insert(HeaderValue::from_static("application/json"));
    let mut request = http::Request::builder()
        .method(ctx.request.method)
        .uri(url)
        .body(body)
        .map_err(|e| ChannelError::InvalidConfig(e.to_string()))?;
    *request.headers_mut() = headers;
    Ok(request)
}
