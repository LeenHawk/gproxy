//! agent-plan-personal-voice-model and https://www.volcengine.com/docs/6561/1598757.
//! HTTP TTS sends newline-delimited JSON containing base64 audio. Buffer the
//! decoded audio until the terminal success record so late errors cannot look
//! like a successful, truncated audio file. Bill only reported text_words.

use super::{Volcengine, json_response};
use crate::channel::{BaseChannel, ChannelError, OperationContext, PrepareContext};
use crate::channels::shared::compatible::http::invalid_response;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use futures_util::StreamExt;
use gproxy_protocol::{
    HttpBody, Operation, OperationKey, WireResponse,
    codec::{CodecLimits, NdjsonDecoder},
    connection::ByteStream,
};
use http::{HeaderValue, StatusCode, header};
use serde_json::{Value, json};

pub(super) const CHARACTERS_HEADER: &str = "x-gproxy-volcengine-audio-characters";

pub(super) fn request(bytes: &[u8]) -> Result<Vec<u8>, ChannelError> {
    let invalid = |message: &str| ChannelError::InvalidConfig(message.into());
    let mut body: Value = serde_json::from_slice(bytes).map_err(|e| invalid(&e.to_string()))?;
    if body["model"].as_str() != Some("doubao-seed-tts-2.0") {
        return Err(invalid(
            "Agent Plan speech model must be doubao-seed-tts-2.0",
        ));
    }
    if body
        .get("stream_format")
        .and_then(Value::as_str)
        .is_some_and(|s| s != "audio")
    {
        return Err(invalid(
            "Ark HTTP TTS currently returns audio; stream_format must be audio",
        ));
    }
    let text = body
        .get("input")
        .and_then(Value::as_str)
        .ok_or_else(|| invalid("Speech input must be text"))?
        .to_owned();
    let voice = body
        .get("voice")
        .and_then(Value::as_str)
        .or_else(|| body.pointer("/voice/id").and_then(Value::as_str))
        .ok_or_else(|| invalid("Speech voice must name an Ark speaker"))?
        .to_owned();
    let format = match body
        .get("response_format")
        .and_then(Value::as_str)
        .unwrap_or("mp3")
    {
        "mp3" => "mp3",
        "opus" => "ogg_opus",
        "pcm" => "pcm",
        _ => return Err(invalid("Ark HTTP TTS supports mp3, opus and pcm audio")),
    };
    let mut params = body
        .get_mut("req_params")
        .map(Value::take)
        .unwrap_or_else(|| json!({}));
    if !params.is_object() {
        return Err(invalid("req_params must be an object"));
    }
    params["text"] = text.into();
    params["speaker"] = voice.into();
    if params.get("audio_params").is_none() {
        params["audio_params"] = json!({});
    }
    if !params["audio_params"].is_object() {
        return Err(invalid("audio_params must be an object"));
    }
    params["audio_params"]["format"] = format.into();
    if let Some(speed) = body.get("speed") {
        let speed = speed
            .as_f64()
            .filter(|v| (0.5..=2.0).contains(v))
            .ok_or_else(|| invalid("Ark speech speed must be between 0.5 and 2.0"))?;
        params["audio_params"]["speech_rate"] = json!(((speed - 1.0) * 100.0).round() as i64);
    }
    if let Some(instructions) = body.get("instructions").and_then(Value::as_str) {
        let mut additions = match params.get("additions") {
            Some(Value::String(value)) => serde_json::from_str(value)
                .map_err(|e| invalid(&format!("Invalid TTS additions: {e}")))?,
            Some(value) => value.clone(),
            None => json!({}),
        };
        if !additions.is_object() {
            return Err(invalid("TTS additions must encode an object"));
        }
        additions["context_texts"] = json!([instructions]);
        params["additions"] = additions.to_string().into();
    }
    let mut out = json!({"req_params": params});
    if let Some(user) = body.get("user") {
        out["user"] = user.clone();
    }
    Ok(out.to_string().into_bytes())
}

pub(super) async fn call(
    channel: &Volcengine,
    ctx: OperationContext<'_>,
) -> Result<WireResponse<HttpBody>, ChannelError> {
    let request = channel.prepare(PrepareContext {
        provider: ctx.provider,
        credential: ctx.credential,
        operation: OperationKey {
            operation: Operation::CreateSpeech,
            dialect: ctx.dialect,
        },
        request: ctx.request,
        endpoint_override: ctx.endpoint_override,
    })?;
    let HttpBody::Bytes(body) = request.body() else {
        unreachable!("prepare buffers speech")
    };
    let body: Value =
        serde_json::from_slice(body).map_err(|e| ChannelError::InvalidConfig(e.to_string()))?;
    let content_type = match body
        .pointer("/req_params/audio_params/format")
        .and_then(Value::as_str)
    {
        Some("ogg_opus") => "audio/ogg",
        Some("pcm") => "application/octet-stream",
        _ => "audio/mpeg",
    };
    let mut reply = ctx.client.send(request).await?;
    if !reply.status.is_success() {
        return Ok(reply);
    }
    let mut stream: ByteStream = match reply.body {
        HttpBody::Bytes(bytes) => Box::pin(futures_util::stream::once(async { Ok(bytes) })),
        HttpBody::Stream(stream) => stream,
    };
    let mut decoder = NdjsonDecoder::new(CodecLimits {
        max_buffer_bytes: 8 * 1024 * 1024,
        max_value_bytes: 8 * 1024 * 1024,
        max_body_bytes: 64 * 1024 * 1024,
        max_line_bytes: 8 * 1024 * 1024,
        max_part_bytes: 0,
        max_parts: 0,
    });
    let mut audio = Vec::new();
    let mut finished = false;
    let mut words = None;
    loop {
        let chunk = stream.next().await;
        let eof = chunk.is_none();
        let records = match chunk {
            Some(chunk) => decoder.push(&chunk.map_err(|e| invalid_response(e.to_string()))?),
            None => decoder.finish(),
        }
        .map_err(|e| invalid_response(e.to_string()))?;
        for record in records {
            match record.get("code").and_then(Value::as_i64) {
                Some(0) => {
                    if let Some(data) = record.get("data").and_then(Value::as_str) {
                        audio.extend(
                            STANDARD
                                .decode(data)
                                .map_err(|e| invalid_response(e.to_string()))?,
                        );
                    }
                }
                Some(20000000) => {
                    words = record.pointer("/usage/text_words").and_then(Value::as_u64);
                    finished = true;
                }
                Some(code) => {
                    let message = record
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("Ark TTS failed");
                    let status = if message.contains("quota exceeded") {
                        StatusCode::TOO_MANY_REQUESTS
                    } else if code >= 50000000 {
                        StatusCode::BAD_GATEWAY
                    } else {
                        StatusCode::BAD_REQUEST
                    };
                    return Ok(json_response(
                        status,
                        json!({"error": {"type": "upstream_error", "code": code, "message": message}}),
                    ));
                }
                None => return Err(invalid_response("Ark TTS record missing code")),
            }
        }
        if finished {
            break;
        }
        if eof {
            return Err(invalid_response("Ark TTS ended before terminal success"));
        }
    }
    reply.headers.remove(header::CONTENT_LENGTH);
    reply.headers.remove(CHARACTERS_HEADER);
    reply
        .headers
        .insert(header::CONTENT_TYPE, HeaderValue::from_static(content_type));
    if let Some(words) = words {
        reply.headers.insert(
            CHARACTERS_HEADER,
            HeaderValue::from_str(&words.to_string())
                .map_err(|e| invalid_response(e.to_string()))?,
        );
    }
    reply.body = HttpBody::Bytes(audio.into());
    Ok(reply)
}
