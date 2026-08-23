use bytes::Bytes;
use gproxy_channel_api::{
    ChannelError, Frame, NormalizedUsage, StreamCtx, StreamDecoder, StreamEnd, StreamTail,
};
use gproxy_protocol::{ContentGenerationKind, Operation, OperationKind};
use rust_decimal::Decimal;

use super::redact::B64Redactor;

pub(super) struct OpenAiSseDecoder {
    kind: Kind,
    buffer: Vec<u8>,
    redactor: B64Redactor,
    usage: Option<NormalizedUsage>,
    audio_bytes: u64,
    audio_bytes_per_second: Option<u64>,
}

#[derive(Clone, Copy)]
enum Kind {
    Chat,
    Responses,
    Image,
    Transcription,
    Speech,
}

impl OpenAiSseDecoder {
    pub(super) fn for_operation(ctx: StreamCtx<'_>) -> Option<Self> {
        let kind = match (ctx.key.operation, ctx.key.kind) {
            (
                Operation::StreamGenerateContent,
                OperationKind::ContentGeneration(ContentGenerationKind::OpenAiChat),
            ) => Kind::Chat,
            (
                Operation::StreamGenerateContent,
                OperationKind::ContentGeneration(ContentGenerationKind::OpenAiResponses),
            ) => Kind::Responses,
            (
                Operation::CreateImage | Operation::EditImage,
                OperationKind::Family(gproxy_protocol::WireFamily::OpenAi),
            ) => Kind::Image,
            (
                Operation::CreateTranscription,
                OperationKind::Family(gproxy_protocol::WireFamily::OpenAi),
            ) => Kind::Transcription,
            (
                Operation::CreateSpeech,
                OperationKind::Family(gproxy_protocol::WireFamily::OpenAi),
            ) => Kind::Speech,
            _ => return None,
        };
        let audio_bytes_per_second = matches!(kind, Kind::Speech)
            .then(|| speech_bytes_per_second(ctx.request_body))
            .flatten();
        Some(Self {
            kind,
            buffer: Vec::new(),
            redactor: B64Redactor::default(),
            usage: None,
            audio_bytes: 0,
            audio_bytes_per_second,
        })
    }

    fn drain(&mut self) {
        while let Some((end, delimiter)) = delimiter(&self.buffer) {
            let raw = self.buffer.drain(..end + delimiter).collect::<Vec<_>>();
            self.observe(&raw[..end]);
        }
    }

    fn observe(&mut self, raw: &[u8]) {
        let Some((event, data)) = frame(raw) else {
            return;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&data) else {
            return;
        };
        let usage = match self.kind {
            Kind::Chat => value.get("usage").and_then(super::usage::from_usage),
            Kind::Responses => {
                let completed = event.as_deref() == Some("response.completed")
                    || value.get("type").and_then(serde_json::Value::as_str)
                        == Some("response.completed");
                completed
                    .then(|| value.pointer("/response/usage"))
                    .flatten()
                    .and_then(super::usage::from_usage)
            }
            Kind::Image => super::usage::from_image_value(&value),
            Kind::Transcription => (value.get("type").and_then(serde_json::Value::as_str)
                == Some("transcript.text.done"))
            .then(|| super::usage::from_transcription_value(&value))
            .flatten(),
            Kind::Speech => {
                let audio = value
                    .get("delta")
                    .or_else(|| value.get("audio"))
                    .and_then(serde_json::Value::as_str);
                if let Some(audio) = audio {
                    self.audio_bytes = self
                        .audio_bytes
                        .saturating_add(base64_decoded_len(audio) as u64);
                }
                None
            }
        };
        if usage.is_some() {
            self.usage = usage;
        }
    }
}

impl StreamDecoder for OpenAiSseDecoder {
    fn push(&mut self, chunk: Bytes) -> Result<Vec<Frame>, ChannelError> {
        self.buffer.extend(self.redactor.push(&chunk));
        if self.buffer.len() > 16 * 1024 * 1024 {
            return Err(ChannelError::Decode(
                "OpenAI SSE frame exceeds 16 MiB after media redaction".into(),
            ));
        }
        self.drain();
        if chunk.is_empty() {
            Ok(Vec::new())
        } else {
            Ok(vec![Frame(chunk)])
        }
    }

    fn finish(&mut self, end: StreamEnd) -> Result<StreamTail, ChannelError> {
        if end == StreamEnd::Complete && !self.buffer.is_empty() {
            let raw = std::mem::take(&mut self.buffer);
            self.observe(&raw);
        } else {
            self.buffer.clear();
        }
        if self.usage.is_none()
            && let Some(bytes_per_second) = self.audio_bytes_per_second
            && self.audio_bytes > 0
        {
            let mut usage = NormalizedUsage::default();
            usage.metrics.insert(
                "audio_seconds".into(),
                Decimal::from(self.audio_bytes) / Decimal::from(bytes_per_second),
            );
            self.usage = Some(usage);
        }
        Ok(StreamTail {
            frames: Vec::new(),
            usage: self.usage.take(),
        })
    }
}

fn speech_bytes_per_second(request: &[u8]) -> Option<u64> {
    let request = serde_json::from_slice::<serde_json::Value>(request).ok()?;
    let format = request
        .get("response_format")
        .or_else(|| request.get("format"))
        .and_then(serde_json::Value::as_str)?;
    (format == "pcm").then_some(48_000)
}

fn base64_decoded_len(value: &str) -> usize {
    let bytes = value.trim().as_bytes();
    let padding = bytes.iter().rev().take_while(|byte| **byte == b'=').count();
    bytes.len().saturating_mul(3) / 4 - padding.min(2)
}

fn delimiter(buffer: &[u8]) -> Option<(usize, usize)> {
    let lf = find(buffer, b"\n\n").map(|index| (index, 2));
    let crlf = find(buffer, b"\r\n\r\n").map(|index| (index, 4));
    match (lf, crlf) {
        (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
        (left, right) => left.or(right),
    }
}

fn frame(raw: &[u8]) -> Option<(Option<String>, String)> {
    let text = std::str::from_utf8(raw).ok()?;
    let mut event = None;
    let mut data = Vec::new();
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("event:") {
            event = Some(value.trim_start().to_owned());
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.strip_prefix(' ').unwrap_or(value));
        }
    }
    (!data.is_empty()).then(|| (event, data.join("\n")))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|candidate| candidate == needle)
}
