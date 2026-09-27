//! Per-call metering read from the standard response of an operation.
//!
//! A channel's job on the response side is to shape whatever its upstream
//! answered into the standard form of the operation it served: Claude
//! Messages, OpenAI Chat Completions or Responses, Gemini, an embedding or
//! image reply. From there on every response is a standard one, whether it is
//! converted for the client or passed through, so usage has one source: the
//! readers here, chosen by operation and dialect. No reader knows a vendor.
//! Fields a vendor adds beside the standard ones are the vendor's to read.
//!
//! Each reader has two forms. [`whole`] reads a complete response body.
//! [`UsageReader`] watches a stream as it passes, chunk by chunk or message
//! by message, without holding more of it than the event it is decoding:
//! the stream is framed with this crate's own [`SseDecoder`] and
//! [`JsonArrayDecoder`], events are filtered by name or by a substring before
//! anything is parsed, and only the events that carry usage are deserialized
//! — into small envelopes that skip the content around the usage object.
//!
//! A reading is `None` when the response reported no usage, which is not the
//! same as zero consumption; a malformed response is one that reported none.
//! No reader panics on its input. A streamed reading is `Partial` until the
//! event that settles it has been seen, and whenever the stream was cut.
//!
//! What each operation reads:
//!
//! | operation | dialects | source |
//! |---|---|---|
//! | generate, stream generate, compact | Claude, OpenAI Chat, Responses (HTTP and websocket), Gemini | the dialect's usage object |
//! | count tokens | all | only a usage object the upstream attaches; the count itself was never consumed |
//! | embeddings | OpenAI, Gemini | `usage` / `usageMetadata`, input only |
//! | images | OpenAI | `usage`, the number of images, and the image qualifiers |
//! | transcription, translation | OpenAI | `usage` in tokens or seconds |
//! | rerank | OpenAI | `usage` in tokens or search units |
//! | realtime | OpenAI | each `response.done`, summed by response id |
//!
//! [`SseDecoder`]: crate::codec::SseDecoder
//! [`JsonArrayDecoder`]: crate::codec::JsonArrayDecoder

mod claude;
mod common;
mod gemini;
mod media;
mod openai;
mod types;

pub use types::{
    NormalizedUsage, ResponseUsage, TokenUsage, UsageAttempt, UsageCompleteness, UsageStreamEnd,
    UsageTransport,
};

use crate::codec::{CodecLimits, JsonArrayDecoder, SseDecoder, SseFrame};
use crate::connection::StreamFraming;
use crate::{Dialect, Operation, WireFamily};

/// What a response of an operation in a dialect is read as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reader {
    Claude,
    Chat,
    Responses,
    Gemini,
    Realtime,
    OpenAiEmbedding,
    GeminiEmbedding,
    Image,
    Transcription,
    Rerank,
    /// Count tokens: only a usage object the upstream attached, read the way
    /// the dialect's generation is.
    CountTokens(&'static Reader),
}

fn reader(operation: Operation, dialect: Dialect) -> Option<Reader> {
    let generate = match dialect {
        Dialect::Claude => Reader::Claude,
        Dialect::OpenAiChat => Reader::Chat,
        Dialect::OpenAi | Dialect::OpenAiResponsesWebSocket => Reader::Responses,
        Dialect::Gemini => Reader::Gemini,
    };
    let openai = dialect.family() == WireFamily::OpenAi;
    Some(match operation {
        Operation::GenerateContent
        | Operation::StreamGenerateContent
        | Operation::CompactContent => generate,
        Operation::CountTokens => Reader::CountTokens(match generate {
            Reader::Claude => &Reader::Claude,
            Reader::Chat => &Reader::Chat,
            Reader::Gemini => &Reader::Gemini,
            _ => &Reader::Responses,
        }),
        Operation::CreateEmbedding | Operation::BatchCreateEmbedding => match dialect.family() {
            WireFamily::OpenAi => Reader::OpenAiEmbedding,
            WireFamily::Gemini => Reader::GeminiEmbedding,
            WireFamily::Claude => return None,
        },
        Operation::CreateImage | Operation::EditImage if openai => Reader::Image,
        Operation::CreateTranscription | Operation::CreateTranslation if openai => {
            Reader::Transcription
        }
        Operation::Rerank if openai => Reader::Rerank,
        Operation::ConnectRealtime if openai => Reader::Realtime,
        _ => return None,
    })
}

/// Usage from a complete, successful response body of `operation` in
/// `dialect`. The caller decides whether the response succeeded; an error
/// body reports no usage anyway.
pub fn whole(operation: Operation, dialect: Dialect, body: &[u8]) -> Option<NormalizedUsage> {
    read(reader(operation, dialect)?, body)
}

fn read(reader: Reader, body: &[u8]) -> Option<NormalizedUsage> {
    match reader {
        Reader::Claude => claude::whole(body),
        Reader::Chat | Reader::Responses | Reader::Realtime => openai::whole(body),
        Reader::Gemini => gemini::whole(body),
        Reader::OpenAiEmbedding => media::openai_embedding(body),
        Reader::GeminiEmbedding => gemini::embedding(body),
        Reader::Image => media::image(body),
        Reader::Transcription => media::transcription(body),
        Reader::Rerank => media::rerank(body),
        Reader::CountTokens(generate) => read(*generate, body),
    }
}

/// Bounds for watching a stream. The host enforces the real transfer limits;
/// these only keep the reader's own buffer finite. They are generous because
/// the event that carries usage can also carry the whole answer: a Responses
/// `response.completed` repeats every output item, base64 images included,
/// and an event over the bound would lose the usage with it.
const LIMITS: CodecLimits = CodecLimits {
    max_buffer_bytes: 64 * 1024 * 1024,
    max_value_bytes: 64 * 1024 * 1024,
    max_body_bytes: u64::MAX,
    max_line_bytes: 64 * 1024 * 1024,
    max_part_bytes: 0,
    max_parts: 0,
};

/// How the stream arrives.
enum Framing {
    Sse(SseDecoder),
    JsonArray(JsonArrayDecoder),
    /// Websocket messages, one complete event each.
    Messages,
    /// The framing broke; the stream is no longer read, and what was read
    /// before stays as a partial reading.
    Broken,
}

/// The per-dialect state of a watched stream.
enum Watch {
    Claude(claude::Stream),
    Chat(openai::ChatStream),
    Responses(openai::ResponsesStream),
    Gemini(gemini::Stream),
    Transcription(media::TranscriptionStream),
}

impl Watch {
    fn event(&mut self, name: Option<&str>, data: &str) {
        match self {
            Self::Claude(stream) => stream.event(name, data),
            Self::Chat(stream) => stream.event(data),
            Self::Responses(stream) => stream.event(name, data),
            Self::Gemini(stream) => stream.event(data),
            Self::Transcription(stream) => stream.event(name, data),
        }
    }

    /// The reading so far. `settled` says the stream ended on its own,
    /// which only Gemini's reading depends on.
    fn reading(&self, settled: bool) -> Option<NormalizedUsage> {
        match self {
            Self::Claude(stream) => stream.snapshot(),
            Self::Chat(stream) => stream.snapshot(),
            Self::Responses(stream) => stream.snapshot(),
            Self::Gemini(stream) => stream.reading(settled),
            Self::Transcription(stream) => stream.snapshot(),
        }
    }
}

/// Watches one streamed response for its usage.
///
/// Feed it the stream as it passes — raw HTTP chunks with [`push`], or
/// websocket text messages with [`push_message`] — and take the reading with
/// [`finish`]. It borrows what it is given and never alters the stream.
/// Every reading is cumulative: [`snapshot`] and [`finish`] replace earlier
/// readings rather than adding to them.
///
/// [`push`]: UsageReader::push
/// [`push_message`]: UsageReader::push_message
/// [`snapshot`]: UsageReader::snapshot
/// [`finish`]: UsageReader::finish
pub struct UsageReader {
    framing: Framing,
    watch: Watch,
}

impl UsageReader {
    /// A reader for a stream of `operation` in `dialect` arriving over
    /// `transport`, or `None` when that stream reports no usage to watch for:
    /// an operation without a streamed form, or a transport its dialect does
    /// not stream over. An HTTP stream without a declared framing is read the
    /// way the dialect streams by default: a JSON array for Gemini, SSE for
    /// the rest.
    pub fn new(operation: Operation, dialect: Dialect, transport: UsageTransport) -> Option<Self> {
        let reader = reader(operation, dialect)?;
        let watch = match reader {
            Reader::Claude => Watch::Claude(claude::Stream::default()),
            Reader::Chat => Watch::Chat(openai::ChatStream::default()),
            Reader::Responses => {
                Watch::Responses(openai::ResponsesStream::new(openai::Mode::Single))
            }
            Reader::Realtime => {
                Watch::Responses(openai::ResponsesStream::new(openai::Mode::Realtime))
            }
            Reader::Image => Watch::Responses(openai::ResponsesStream::new(openai::Mode::Image)),
            Reader::Gemini => Watch::Gemini(gemini::Stream::default()),
            Reader::Transcription => Watch::Transcription(media::TranscriptionStream::default()),
            Reader::OpenAiEmbedding
            | Reader::GeminiEmbedding
            | Reader::Rerank
            | Reader::CountTokens(_) => return None,
        };
        let framing = match (transport, &watch) {
            (UsageTransport::WebSocket, Watch::Responses(_)) => Framing::Messages,
            (UsageTransport::WebSocket, _) => return None,
            (
                UsageTransport::Http {
                    framing: Some(StreamFraming::JsonArray) | None,
                },
                Watch::Gemini(_),
            ) => Framing::JsonArray(JsonArrayDecoder::new(LIMITS)),
            (
                UsageTransport::Http {
                    framing: Some(StreamFraming::Sse) | None,
                },
                _,
            ) => Framing::Sse(SseDecoder::new(LIMITS)),
            (UsageTransport::Http { .. }, _) => return None,
        };
        Some(Self { framing, watch })
    }

    /// Observe raw HTTP bytes. Chunks need not align with events.
    pub fn push(&mut self, chunk: &[u8]) {
        match &mut self.framing {
            Framing::Sse(decoder) => match decoder.push(chunk) {
                Ok(frames) => {
                    for frame in frames {
                        if let SseFrame::Event(event) = frame {
                            self.watch.event(event.event.as_deref(), &event.data);
                        }
                    }
                }
                Err(_) => self.framing = Framing::Broken,
            },
            Framing::JsonArray(decoder) => match decoder.push(chunk) {
                Ok(records) => {
                    if let Watch::Gemini(stream) = &mut self.watch {
                        for record in &records {
                            stream.record(record);
                        }
                    }
                }
                Err(_) => self.framing = Framing::Broken,
            },
            Framing::Messages | Framing::Broken => {}
        }
    }

    /// Observe one websocket text message, which is one complete event.
    pub fn push_message(&mut self, text: &str) {
        if matches!(self.framing, Framing::Messages) {
            self.watch.event(None, text);
        }
    }

    /// The reading so far. It is `Partial` until the event that settles it
    /// has been seen.
    pub fn snapshot(&self) -> Option<NormalizedUsage> {
        self.reading(false)
    }

    fn reading(&self, settled: bool) -> Option<NormalizedUsage> {
        let broken = matches!(self.framing, Framing::Broken);
        let mut usage = self.watch.reading(settled && !broken)?;
        if broken {
            usage.completeness = UsageCompleteness::Partial;
        }
        Some(usage)
    }

    /// The final reading. A stream that was cut is never complete, whatever
    /// it reported before it stopped.
    pub fn finish(self, end: UsageStreamEnd) -> Option<NormalizedUsage> {
        let mut usage = self.reading(end == UsageStreamEnd::Complete)?;
        if end == UsageStreamEnd::Interrupted {
            usage.completeness = UsageCompleteness::Partial;
        }
        Some(usage)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// `wire` in chunks of `size` bytes, splitting inside characters too.
    pub(crate) fn chunked(wire: &str, size: usize) -> Vec<Vec<u8>> {
        wire.as_bytes()
            .chunks(size.clamp(1, wire.len().max(1)))
            .map(<[u8]>::to_vec)
            .collect()
    }

    pub(crate) fn feed_chunks(
        operation: Operation,
        dialect: Dialect,
        transport: UsageTransport,
        chunks: &[Vec<u8>],
        end: UsageStreamEnd,
    ) -> Option<NormalizedUsage> {
        let mut reader = UsageReader::new(operation, dialect, transport).expect("watchable");
        for chunk in chunks {
            reader.push(chunk);
        }
        reader.finish(end)
    }

    pub(crate) fn feed_sse(
        operation: Operation,
        dialect: Dialect,
        chunks: &[Vec<u8>],
        end: UsageStreamEnd,
    ) -> Option<NormalizedUsage> {
        feed_chunks(
            operation,
            dialect,
            UsageTransport::Http {
                framing: Some(StreamFraming::Sse),
            },
            chunks,
            end,
        )
    }

    pub(crate) fn feed_messages(
        operation: Operation,
        dialect: Dialect,
        messages: &[&str],
        end: UsageStreamEnd,
    ) -> Option<NormalizedUsage> {
        let mut reader =
            UsageReader::new(operation, dialect, UsageTransport::WebSocket).expect("watchable");
        for message in messages {
            reader.push_message(message);
        }
        reader.finish(end)
    }

    #[test]
    fn count_tokens_never_meters_the_count_itself() {
        for (dialect, body) in [
            (Dialect::Claude, r#"{"input_tokens":12}"#),
            (
                Dialect::OpenAi,
                r#"{"object":"response.input_tokens","input_tokens":12}"#,
            ),
            (
                Dialect::Gemini,
                r#"{"totalTokens":12,"cachedContentTokenCount":2}"#,
            ),
        ] {
            assert!(
                whole(Operation::CountTokens, dialect, body.as_bytes()).is_none(),
                "{dialect:?}"
            );
        }
        let attached = whole(
            Operation::CountTokens,
            Dialect::Claude,
            br#"{"input_tokens":12,"usage":{"input_tokens":3,"output_tokens":0}}"#,
        )
        .expect("an attached usage object is reported");
        assert_eq!(attached.tokens.input_tokens, Some(3));
    }

    #[test]
    fn only_operations_with_a_stream_get_a_stream_reader() {
        let sse = UsageTransport::Http {
            framing: Some(StreamFraming::Sse),
        };
        for (operation, dialect) in [
            (Operation::CountTokens, Dialect::Claude),
            (Operation::CreateEmbedding, Dialect::OpenAi),
            (Operation::Rerank, Dialect::OpenAi),
            (Operation::ListModels, Dialect::OpenAi),
            (Operation::DeleteFile, Dialect::OpenAi),
            (Operation::CreateImage, Dialect::Gemini),
        ] {
            assert!(
                UsageReader::new(operation, dialect, sse).is_none(),
                "{operation:?}"
            );
        }
        assert!(
            UsageReader::new(
                Operation::StreamGenerateContent,
                Dialect::Claude,
                UsageTransport::WebSocket
            )
            .is_none()
        );
        assert!(
            UsageReader::new(
                Operation::StreamGenerateContent,
                Dialect::OpenAiChat,
                UsageTransport::Http {
                    framing: Some(StreamFraming::NdJson)
                }
            )
            .is_none()
        );
    }

    #[test]
    fn a_broken_stream_keeps_its_reading_as_partial() {
        let usage = "data: {\"usage\":{\"prompt_tokens\":2,\"completion_tokens\":1}}\n\n";
        let mut reader = UsageReader::new(
            Operation::StreamGenerateContent,
            Dialect::OpenAiChat,
            UsageTransport::Http {
                framing: Some(StreamFraming::Sse),
            },
        )
        .unwrap();
        reader.push(usage.as_bytes());
        reader.push(&[0xff, 0xfe, b'\n']);
        reader.push(usage.as_bytes());
        let reading = reader.finish(UsageStreamEnd::Complete).unwrap();
        assert_eq!(reading.tokens.input_tokens, Some(2));
        assert_eq!(reading.completeness, UsageCompleteness::Partial);
    }

    #[test]
    fn arbitrary_bytes_never_panic() {
        let mut seed = 0x9e37_79b9_u32;
        let mut noise = Vec::new();
        for _ in 0..4096 {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            noise.push(b"{}[]\":,data: event\n\r0123usage_metadata"[seed as usize % 38]);
        }
        for dialect in [
            Dialect::Claude,
            Dialect::OpenAiChat,
            Dialect::OpenAi,
            Dialect::Gemini,
            Dialect::OpenAiResponsesWebSocket,
        ] {
            for operation in [
                Operation::GenerateContent,
                Operation::StreamGenerateContent,
                Operation::CountTokens,
                Operation::CreateEmbedding,
                Operation::CreateImage,
                Operation::CreateTranscription,
                Operation::Rerank,
                Operation::ConnectRealtime,
            ] {
                let _ = whole(operation, dialect, &noise);
                for transport in [
                    UsageTransport::Http { framing: None },
                    UsageTransport::Http {
                        framing: Some(StreamFraming::JsonArray),
                    },
                    UsageTransport::WebSocket,
                ] {
                    if let Some(mut reader) = UsageReader::new(operation, dialect, transport) {
                        for chunk in noise.chunks(13) {
                            reader.push(chunk);
                            reader.push_message(&String::from_utf8_lossy(chunk));
                        }
                        let _ = reader.finish(UsageStreamEnd::Interrupted);
                    }
                }
            }
        }
    }
}
