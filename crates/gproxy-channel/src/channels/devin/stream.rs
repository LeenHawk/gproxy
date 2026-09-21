//! `GetChatMessageResponse` frames translated into OpenAI Chat Completions.
//!
//! The response is a multi-frame Connect stream, each frame a protobuf
//! message whose fields are deltas:
//!
//! | Field | Meaning | Becomes |
//! |---|---|---|
//! | #3 | answer text | `delta.content` |
//! | #5 | stop enum | `finish_reason` |
//! | #7 | metadata sub-message | `usage` |
//! | #9 | thinking text | `delta.reasoning_content` |
//!
//! The reference file's own header says the deltas ride #9; its later
//! calibration corrects that (`devin-connect.js`, the "Response frame
//! decoding" block: *"Earlier code read #9 as the content — that was the
//! thinking stream. The answer the caller actually wants is #3"*). The
//! decoder follows the correction and keeps #9 as reasoning.
//!
//! Text is decoded across frame boundaries: a multi-byte character can be
//! split between two frames, and decoding each frame alone would turn it into
//! replacement characters.
//!
//! The caller's `stop` sequences are enforced **here**, because the wire has
//! no field for them: the reference records that the OpenAI path accepted
//! `stop` and never enforced it, so a client fencing its output with a
//! sentinel got over-long completions, and fixes it with local truncation
//! (`samples/windsurfapi/src/stop-sequences.js`). [`StopGate`] is that fix,
//! including its held tail — a stop sequence can straddle two frames.

use futures_util::StreamExt as _;
use gproxy_protocol::connection::{ByteStream, Bytes, TransportError};
use serde_json::{Value, json};

use super::connect::{FrameReader, trailer_error};
use super::{error, proto};
use crate::channel::ChannelError;

// GetChatMessageResponse
const RES_CONTENT: u32 = 3;
const RES_FINISH: u32 = 5;
const RES_METADATA: u32 = 7;
const RES_REASONING: u32 = 9;

// The #7 metadata sub-message.
const META_PROMPT_TOKENS: u32 = 2;
const META_COMPLETION_TOKENS: u32 = 3;
/// Cache-creation input, calibrated on a paid account against Anthropic-family
/// selectors; absent on GPT-family selectors and on free accounts, where the
/// counter is zero and protobuf omits zero scalars.
const META_CACHE_WRITE_TOKENS: u32 = 4;
/// Cache-read input, calibrated on the same paid account.
const META_CACHE_READ_TOKENS: u32 = 5;
/// `actual_model_uid`: the concrete model behind whatever was asked for,
/// frame-verified on two model families. It must not be echoed as the
/// response `model` — clients compare that against what they requested — but
/// it is the only signal of what actually ran, so it travels as a usage
/// dimension where metering and pricing can see it.
const META_ACTUAL_MODEL: u32 = 9;

/// The key `actual_model_uid` is reported under, in the usage object and as a
/// `NormalizedUsage` dimension.
pub const ACTUAL_MODEL_KEY: &str = "actual_model";

/// What the upstream reported about one turn's consumption.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    /// Fresh input only: a paid capture shows `prompt_tokens` excluding both
    /// cached and cache-created tokens.
    pub prompt: u64,
    pub completion: u64,
    pub cache_read: Option<u64>,
    pub cache_write: Option<u64>,
}

impl Usage {
    /// The OpenAI-shaped object, where `prompt_tokens` includes cache reads
    /// and `cached_tokens` is the detail inside it.
    pub fn to_json(self) -> Value {
        let prompt = self.prompt.saturating_add(self.cache_read.unwrap_or(0));
        let mut usage = json!({
            "prompt_tokens": prompt,
            "completion_tokens": self.completion,
            // Cache creation is billable generation-side input, so it stays in
            // the total while staying out of prompt_tokens.
            "total_tokens": prompt
                .saturating_add(self.completion)
                .saturating_add(self.cache_write.unwrap_or(0)),
        });
        if let Some(cached) = self.cache_read {
            usage["prompt_tokens_details"] = json!({"cached_tokens": cached});
        }
        if let Some(created) = self.cache_write {
            usage["cache_creation_input_tokens"] = json!(created);
        }
        usage
    }
}

/// Accumulates bytes that may end mid-character.
#[derive(Debug, Default)]
struct Utf8Stream {
    pending: Vec<u8>,
}

impl Utf8Stream {
    fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut out = String::new();
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(text) => {
                    out.push_str(text);
                    self.pending.clear();
                    return out;
                }
                Err(error) => {
                    let valid = error.valid_up_to();
                    out.push_str(
                        std::str::from_utf8(&self.pending[..valid]).expect("validated prefix"),
                    );
                    match error.error_len() {
                        // Genuinely invalid: drop the bad bytes rather than
                        // stall the stream waiting for a completion that will
                        // never arrive.
                        Some(len) => {
                            out.push(char::REPLACEMENT_CHARACTER);
                            self.pending.drain(..valid + len);
                        }
                        // An incomplete sequence: keep it for the next frame.
                        None => {
                            self.pending.drain(..valid);
                            return out;
                        }
                    }
                }
            }
        }
    }
}

/// Local enforcement of the caller's `stop` sequences, ported from
/// `stop-sequences.js`. A sequence can straddle two content frames, so the
/// last `max_len - 1` bytes are held back until they are known not to be the
/// head of one. Once a sequence completes, everything from it onwards is
/// suppressed and the turn finishes as `stop`; the matched text itself is not
/// returned, which is OpenAI's semantics.
#[derive(Debug, Default)]
pub(super) struct StopGate {
    sequences: Vec<String>,
    hold: usize,
    buffer: String,
    done: bool,
}

impl StopGate {
    pub(super) fn new(sequences: Vec<String>) -> Self {
        let hold = sequences
            .iter()
            .map(String::len)
            .max()
            .unwrap_or(0)
            .saturating_sub(1);
        Self {
            sequences,
            hold,
            buffer: String::new(),
            done: false,
        }
    }

    fn active(&self) -> bool {
        !self.sequences.is_empty()
    }

    /// The safe-to-send prefix of `chunk`, and whether a stop sequence just
    /// completed.
    fn push(&mut self, chunk: &str) -> (String, bool) {
        if !self.active() {
            return (chunk.to_owned(), false);
        }
        if self.done {
            return (String::new(), false);
        }
        self.buffer.push_str(chunk);
        // The earliest match wins, so the answer is the shortest correct
        // prefix when several sequences appear.
        let cut = self
            .sequences
            .iter()
            .filter_map(|sequence| self.buffer.find(sequence.as_str()))
            .min();
        if let Some(cut) = cut {
            let emit = self.buffer[..cut].to_owned();
            self.buffer.clear();
            self.done = true;
            return (emit, true);
        }
        if self.buffer.len() <= self.hold {
            return (String::new(), false);
        }
        let mut split = self.buffer.len() - self.hold;
        while split > 0 && !self.buffer.is_char_boundary(split) {
            split -= 1;
        }
        let emit = self.buffer[..split].to_owned();
        self.buffer.drain(..split);
        (emit, false)
    }

    /// The stream ended with no match: release whatever is still held.
    fn flush(&mut self) -> String {
        if self.done {
            return String::new();
        }
        std::mem::take(&mut self.buffer)
    }
}

/// One turn's translation state.
pub(super) struct Codec {
    id: String,
    model: String,
    created: u64,
    max_tokens: Option<u64>,
    content: Utf8Stream,
    reasoning: Utf8Stream,
    stop: StopGate,
    /// A stop sequence completed, so the rest of the answer is suppressed.
    stopped: bool,
    text: String,
    thinking: String,
    started: bool,
    finish: Option<u64>,
    usage: Option<Usage>,
    /// `#7.9`, the model that actually served this turn.
    actual_model: Option<String>,
    closed: bool,
}

impl Codec {
    pub(super) fn new(
        id: String,
        model: String,
        created: u64,
        max_tokens: Option<u64>,
        stop: Vec<String>,
    ) -> Self {
        Self {
            id,
            model,
            created,
            max_tokens,
            content: Utf8Stream::default(),
            reasoning: Utf8Stream::default(),
            stop: StopGate::new(stop),
            stopped: false,
            text: String::new(),
            thinking: String::new(),
            started: false,
            finish: None,
            usage: None,
            actual_model: None,
            closed: false,
        }
    }

    /// Chunks for one response frame. A frame this decoder cannot parse is an
    /// upstream protocol error, not an empty delta: the host is better served
    /// by a failed stream than by a silently short answer.
    pub(super) fn frame(&mut self, payload: &[u8]) -> Result<Vec<Value>, ChannelError> {
        let fields = proto::parse(payload)?;
        let mut chunks = Vec::new();
        if !self.started {
            self.started = true;
            chunks.push(self.chunk(json!({"role": "assistant", "content": ""}), None));
        }
        if let Some(bytes) = proto::bytes_of(&fields, RES_REASONING) {
            let text = self.reasoning.push(bytes);
            if !text.is_empty() {
                self.thinking.push_str(&text);
                chunks.push(self.chunk(json!({ "reasoning_content": text }), None));
            }
        }
        if let Some(bytes) = proto::bytes_of(&fields, RES_CONTENT) {
            let text = self.content.push(bytes);
            if !text.is_empty() {
                let (emit, hit) = self.stop.push(&text);
                if !emit.is_empty() {
                    self.text.push_str(&emit);
                    chunks.push(self.chunk(json!({ "content": emit }), None));
                }
                self.stopped |= hit;
            }
        }
        if let Some(finish) = proto::varint_of(&fields, RES_FINISH) {
            self.finish = Some(finish);
        }
        if let Some(metadata) = proto::bytes_of(&fields, RES_METADATA) {
            let metadata = proto::parse(metadata)?;
            if let Some(actual) = proto::text_of(&metadata, META_ACTUAL_MODEL)
                .map(str::trim)
                .filter(|model| !model.is_empty())
            {
                self.actual_model = Some(actual.to_owned());
            }
            // `completion_tokens` only rides the terminal metadata frame;
            // without it the pair is not yet a usage reading.
            if let Some(completion) = proto::varint_of(&metadata, META_COMPLETION_TOKENS) {
                self.usage = Some(Usage {
                    prompt: proto::varint_of(&metadata, META_PROMPT_TOKENS).unwrap_or(0),
                    completion,
                    cache_read: proto::varint_of(&metadata, META_CACHE_READ_TOKENS),
                    cache_write: proto::varint_of(&metadata, META_CACHE_WRITE_TOKENS),
                });
            }
        }
        Ok(chunks)
    }

    /// The usage object for this turn, with the served model attached when
    /// the upstream named one.
    fn usage_json(&self) -> Option<Value> {
        let mut usage = self.usage?.to_json();
        if let Some(actual) = &self.actual_model {
            usage[ACTUAL_MODEL_KEY] = json!(actual);
        }
        Some(usage)
    }

    /// The terminal chunk. Usage rides it whenever the upstream reported a
    /// terminal metadata frame; an absent block means unreported rather than
    /// zero, so no `usage` key is written at all.
    pub(super) fn close(&mut self) -> Vec<Value> {
        if self.closed {
            return Vec::new();
        }
        self.closed = true;
        let mut chunks = Vec::new();
        if !self.started {
            self.started = true;
            chunks.push(self.chunk(json!({"role": "assistant", "content": ""}), None));
        }
        // No stop sequence matched, so the held tail was ordinary output.
        let tail = self.stop.flush();
        if !tail.is_empty() {
            self.text.push_str(&tail);
            chunks.push(self.chunk(json!({ "content": tail }), None));
        }
        let mut last = self.chunk(json!({}), Some(self.finish_reason()));
        if let Some(usage) = self.usage_json() {
            last["usage"] = usage;
        }
        chunks.push(last);
        chunks
    }

    /// Truncation is read from the usage, not from the stop enum. Only two of
    /// the upstream's values are pinned by captures (2 on a free completion, 4
    /// on a paid one) and both mean a clean stop, so every value maps to
    /// `stop`; `length` is inferred when the answer lands exactly on the cap
    /// the caller asked for, which is the test an OpenAI client would apply
    /// itself. Reporting a complete answer as truncated is the more harmful
    /// error, so the check is deliberately an equality. A locally enforced
    /// stop sequence outranks both: the answer really did stop where the
    /// caller asked, whatever the upstream went on to generate.
    fn finish_reason(&self) -> &'static str {
        if self.stopped {
            return "stop";
        }
        match (self.max_tokens, self.usage) {
            (Some(cap), Some(usage)) if usage.completion == cap => "length",
            _ => "stop",
        }
    }

    fn chunk(&self, delta: Value, finish: Option<&str>) -> Value {
        json!({
            "id": self.id,
            "object": "chat.completion.chunk",
            "created": self.created,
            "model": self.model,
            "choices": [{
                "index": 0,
                "delta": delta,
                "finish_reason": finish,
            }],
        })
    }

    /// The buffered answer as one non-streaming `chat.completion` body.
    pub(super) fn completion(&self) -> Value {
        let finish = self.finish_reason();
        let mut message = json!({"role": "assistant", "content": self.text});
        if !self.thinking.is_empty() {
            message["reasoning_content"] = json!(self.thinking);
        }
        let mut body = json!({
            "id": self.id,
            "object": "chat.completion",
            "created": self.created,
            "model": self.model,
            "choices": [{"index": 0, "message": message, "finish_reason": finish}],
        });
        if let Some(usage) = self.usage_json() {
            body["usage"] = usage;
        }
        body
    }
}

/// `data: {json}\n\n`, and the `[DONE]` sentinel OpenAI clients wait for.
pub(super) fn encode(chunks: &[Value]) -> Bytes {
    let mut out = String::new();
    for chunk in chunks {
        out.push_str("data: ");
        out.push_str(&chunk.to_string());
        out.push_str("\n\n");
    }
    Bytes::from(out)
}

pub(super) fn done() -> Bytes {
    Bytes::from_static(b"data: [DONE]\n\n")
}

/// One generation, driven from the upstream Connect stream.
pub(super) struct Turn {
    upstream: ByteStream,
    reader: FrameReader,
    codec: Codec,
    /// The trailer arrived, so the answer is complete.
    ended: bool,
    finished: bool,
}

impl Turn {
    pub(super) fn new(upstream: ByteStream, codec: Codec) -> Self {
        Self {
            upstream,
            reader: FrameReader::new(),
            codec,
            ended: false,
            finished: false,
        }
    }

    /// The next batch of translated chunks, or `None` once the turn is over.
    pub(super) async fn step(&mut self) -> Option<Result<Vec<Value>, ChannelError>> {
        loop {
            if self.finished {
                return None;
            }
            let Some(next) = self.upstream.next().await else {
                self.finished = true;
                if !self.ended {
                    // Connect always terminates a stream with an end-of-stream
                    // frame. A socket that simply stops delivered a partial
                    // answer, and reporting it as a finished turn would make a
                    // truncated reply the next turn's context.
                    return Some(Err(ChannelError::InvalidResponse(
                        "devin stream ended without its Connect trailer".into(),
                    )));
                }
                let chunks = self.codec.close();
                return (!chunks.is_empty()).then_some(Ok(chunks));
            };
            let chunk = match next {
                Ok(chunk) => chunk,
                Err(error) => {
                    self.finished = true;
                    return Some(Err(ChannelError::InvalidResponse(error.to_string())));
                }
            };
            let frames = match self.reader.push(&chunk) {
                Ok(frames) => frames,
                Err(error) => {
                    self.finished = true;
                    return Some(Err(error));
                }
            };
            let mut chunks = Vec::new();
            for frame in frames {
                if frame.end_stream {
                    self.ended = true;
                    if let Some(failure) = trailer_error(&frame.payload) {
                        self.finished = true;
                        // Classified rather than flattened: the trailer is
                        // where a transient fault arrives dressed as
                        // `permission_denied`. See `error.rs`.
                        return Some(Err(error::from_trailer(
                            failure.code.as_deref(),
                            &failure.message,
                        )));
                    }
                    chunks.extend(self.codec.close());
                    self.finished = true;
                    break;
                }
                match self.codec.frame(&frame.payload) {
                    Ok(translated) => chunks.extend(translated),
                    Err(error) => {
                        self.finished = true;
                        return Some(Err(error));
                    }
                }
            }
            if !chunks.is_empty() {
                return Some(Ok(chunks));
            }
            if self.finished {
                return None;
            }
        }
    }

    /// Pull the first batch before the response headers are committed. An
    /// upstream refusal usually arrives as the *first* thing on the stream —
    /// the trailer of an otherwise empty 200 — and until a byte has been
    /// written this channel can still answer it with a status the host can
    /// classify instead of a broken stream. Once content is flowing the
    /// status is spent, and a later failure can only fail the stream.
    pub(super) async fn prime(&mut self) -> Result<Vec<Value>, ChannelError> {
        match self.step().await {
            Some(result) => result,
            None => Ok(Vec::new()),
        }
    }

    /// The Chat Completions SSE body, starting from the already-pulled batch.
    pub(super) fn into_stream(self, primed: Vec<Value>) -> ByteStream {
        Box::pin(futures_util::stream::unfold(
            (self, Some(primed), false),
            |(mut turn, primed, sent_done)| async move {
                if let Some(primed) = primed {
                    let encoded = encode(&primed);
                    return Some((Ok(encoded), (turn, None, sent_done)));
                }
                if sent_done {
                    return None;
                }
                match turn.step().await {
                    Some(Ok(chunks)) => Some((Ok(encode(&chunks)), (turn, None, false))),
                    Some(Err(error)) => {
                        let boxed: TransportError = Box::new(error::as_stream_failure(error));
                        Some((Err(boxed), (turn, None, true)))
                    }
                    None => Some((Ok(done()), (turn, None, true))),
                }
            },
        ))
    }

    /// Drain the turn into one `chat.completion` body. A failure keeps its
    /// class: `mod.rs` turns a classified refusal into an answer.
    pub(super) async fn collect(mut self) -> Result<Value, ChannelError> {
        while let Some(item) = self.step().await {
            item?;
        }
        Ok(self.codec.completion())
    }
}
