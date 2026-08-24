use bytes::Bytes;
use gproxy_channel_api::{ChannelError, PrepareCtx};
use gproxy_protocol::Operation;
use http::HeaderMap;
use serde_json::{Map, Value};

pub(super) fn request(
    ctx: &PrepareCtx<'_>,
    headers: &mut HeaderMap,
) -> Result<Bytes, ChannelError> {
    if ctx.stream {
        return Err(ChannelError::Prepare(
            "xAI audio API does not use OpenAI SSE".into(),
        ));
    }
    match ctx.key.operation {
        Operation::CreateSpeech => speech(ctx.body),
        Operation::CreateTranscription => {
            let (body, content_type) = super::super::multipart::stt(ctx.headers, ctx.body)?;
            headers.insert(http::header::CONTENT_TYPE, content_type);
            Ok(body)
        }
        _ => Ok(ctx.body.clone()),
    }
}

fn speech(body: &[u8]) -> Result<Bytes, ChannelError> {
    let mut object = super::json_object(body, "speech")?;
    for name in ["model", "instructions", "stream_format"] {
        object.remove(name);
    }
    if let Some(input) = object.remove("input") {
        object.entry("text").or_insert(input);
    }
    if let Some(voice) = object.remove("voice") {
        object.entry("voice_id").or_insert(voice);
    }
    if let Some(format) = object.remove("response_format") {
        let output = object
            .entry("output_format")
            .or_insert_with(|| Value::Object(Map::new()));
        output
            .as_object_mut()
            .ok_or_else(|| ChannelError::Prepare("output_format must be an object".into()))?
            .entry("codec")
            .or_insert(format);
    }
    super::encode(Value::Object(object))
}
