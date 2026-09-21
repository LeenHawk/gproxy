//! xAI's own paths and the speech body they go with.

use super::config::{DEFAULT_BASE_URL, XaiConfig};
use crate::channel::{ChannelError, ChannelHeaders, HeaderAllowlist, PrepareContext, forwardable};
use crate::channels::shared::compatible::http::{insert_configured, strip_query_auth};
use gproxy_protocol::{HttpBody, Operation, WireRequest, connection::Bytes};
use http::{HeaderValue, header};
use serde_json::{Map, Value};

/// The conversation id Grok's own client threads through a session. A
/// provider allow-list narrows what other headers a client may add; it must
/// not drop this one.
pub const CLIENT_HEADERS: ChannelHeaders = ChannelHeaders {
    names: &["x-grok-conv-id"],
    prefixes: &[],
};

/// xAI does not follow OpenAI's audio and video layout: speech is `/v1/tts`,
/// transcription `/v1/stt` and video creation `/v1/videos/generations`.
/// Everything else keeps the path the caller's dialect uses.
pub(super) fn path(operation: Operation, path: &str) -> String {
    match operation {
        Operation::CreateSpeech => "/v1/tts".into(),
        Operation::CreateTranscription => "/v1/stt".into(),
        Operation::CreateVideo => "/v1/videos/generations".into(),
        _ if path.starts_with('/') => path.to_owned(),
        _ => format!("/{path}"),
    }
}

/// xAI's speech API is not OpenAI's: the text is `text`, the voice is
/// `voice_id` and the container is `output_format.codec`. `model`,
/// `instructions` and `stream_format` have no counterpart and are dropped
/// (v3 `xai/shape/audio.rs`).
fn speech(body: &[u8]) -> Result<Bytes, ChannelError> {
    let mut object: Map<String, Value> = serde_json::from_slice(body)
        .map_err(|_| ChannelError::InvalidConfig("xAI speech requests must be JSON".into()))?;
    for name in ["model", "instructions", "stream_format"] {
        object.remove(name);
    }
    if let Some(input) = object.remove("input") {
        object.insert("text".into(), input);
    }
    if let Some(voice) = object.remove("voice") {
        object.insert("voice_id".into(), voice);
    }
    if let Some(format) = object.remove("response_format") {
        object.insert("output_format".into(), serde_json::json!({"codec": format}));
    }
    Ok(Bytes::from(Value::Object(object).to_string()))
}

pub(super) fn build(
    ctx: PrepareContext<'_>,
) -> Result<(http::request::Builder, WireRequest<HttpBody>), ChannelError> {
    let config = XaiConfig::from_view(ctx.provider)?;
    let key = ctx
        .credential
        .secret
        .get("api_key")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|key| !key.is_empty())
        .ok_or(ChannelError::InvalidCredential)?;
    let operation = ctx.operation.operation;
    let mut request = ctx.request;
    let url = match ctx.endpoint_override {
        Some(url) => url.to_owned(),
        None => {
            let base = ctx
                .provider
                .base_url
                .map(str::trim)
                .filter(|base| !base.is_empty())
                .unwrap_or(DEFAULT_BASE_URL)
                .trim_end_matches('/');
            format!("{base}{}", path(operation, &request.path))
        }
    };
    let uri = match request
        .query
        .take()
        .map(|query| strip_query_auth(&query))
        .filter(|query| !query.is_empty())
    {
        Some(query) => format!("{url}?{query}"),
        None => url,
    };
    let allowlist = HeaderAllowlist::from_view_for(ctx.provider, CLIENT_HEADERS)?;
    let mut headers = forwardable(&request.headers, allowlist.as_ref(), &[]);
    headers.insert(
        header::AUTHORIZATION,
        HeaderValue::from_str(&format!("Bearer {key}"))
            .map_err(|_| ChannelError::InvalidCredential)?,
    );
    for (name, value) in &config.headers {
        insert_configured(&mut headers, name, value)?;
    }
    if operation == Operation::CreateSpeech {
        let HttpBody::Bytes(bytes) = &request.body else {
            return Err(ChannelError::InvalidConfig(
                "xAI speech requests need a buffered JSON body".into(),
            ));
        };
        request.body = HttpBody::Bytes(speech(bytes)?);
        headers.insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/json"),
        );
    }
    let mut builder = http::Request::builder()
        .method(request.method.clone())
        .uri(uri);
    if let Some(map) = builder.headers_mut() {
        *map = headers;
    }
    Ok((builder, request))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_speech_request_is_renamed_into_xais_own_fields() {
        let body = json!({"model": "grok-voice", "input": "hello", "voice": "ara",
            "response_format": "mp3", "stream_format": "sse", "speed": 1.2})
        .to_string();
        let out: Value = serde_json::from_slice(&speech(body.as_bytes()).unwrap()).unwrap();
        assert_eq!(
            out,
            json!({"text": "hello", "voice_id": "ara",
                "output_format": {"codec": "mp3"}, "speed": 1.2})
        );
    }
}
