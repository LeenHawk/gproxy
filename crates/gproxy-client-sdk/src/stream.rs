//! Event streams own the response body. Drop (or `cancel`) releases the body
//! and cancels transport consumption; there is no background SDK task.
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
use crate::dialects;
use crate::{Error, Result, ir::*};
use futures_core::Stream;
use futures_util::StreamExt;
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
use futures_util::stream;
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
use gproxy_protocol::{
    HttpBody,
    codec::{SseDecoder, SseFrame},
    connection::ByteStream,
};
use serde::{Deserialize, Serialize};
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
use std::collections::VecDeque;
use std::{collections::BTreeMap, pin::Pin};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BlockId {
    pub item: u32,
    pub part: u32,
}
#[cfg(any(feature = "claude", feature = "openai", feature = "gemini"))]
impl BlockId {
    fn item(item: u32) -> Self {
        Self { item, part: 0 }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Event {
    Start {
        warnings: Vec<Warning>,
    },
    Warning {
        warning: Warning,
    },
    ResponseMetadata {
        metadata: ResponseMetadata,
    },
    Source {
        source: Source,
    },
    TextDelta {
        id: BlockId,
        delta: String,
    },
    ReasoningDelta {
        id: BlockId,
        delta: String,
    },
    AudioDelta {
        id: BlockId,
        bytes: Vec<u8>,
        format: AudioFormat,
    },
    TranscriptDelta {
        id: BlockId,
        delta: String,
    },
    Citation {
        id: BlockId,
        citation: Citation,
    },
    ToolCallStart {
        id: BlockId,
        call_id: String,
        name: String,
        provider_executed: bool,
    },
    ToolCallArgsDelta {
        id: BlockId,
        delta: String,
    },
    ToolCallEnd {
        id: BlockId,
    },
    /// Authoritative final content, including provider_metadata. Consumers can
    /// display deltas immediately and persist only this complete block.
    BlockEnd {
        id: BlockId,
        block: Block,
    },
    Finish {
        reason: FinishReason,
        usage: Usage,
    },
    Error {
        error: Error,
    },
}

#[cfg(not(target_arch = "wasm32"))]
pub type EventStream = Pin<Box<dyn Stream<Item = Event> + Send + 'static>>;
#[cfg(target_arch = "wasm32")]
pub type EventStream = Pin<Box<dyn Stream<Item = Event> + 'static>>;

pub struct GenerationStream {
    inner: EventStream,
}
impl GenerationStream {
    pub fn cancel(self) {
        drop(self);
    }
    pub async fn aggregate(self) -> Result<GenerateResult> {
        aggregate(self).await
    }
}
impl Stream for GenerationStream {
    type Item = Event;
    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Event>> {
        self.inner.as_mut().poll_next(cx)
    }
}

pub async fn aggregate(events: impl Stream<Item = Event>) -> Result<GenerateResult> {
    futures_util::pin_mut!(events);
    let mut content = BTreeMap::<BlockId, Block>::new();
    let mut response = ResponseMetadata::default();
    let mut warnings = Vec::new();
    let mut sources = Vec::new();
    let mut citations = BTreeMap::<BlockId, Vec<Citation>>::new();
    while let Some(event) = events.next().await {
        match event {
            Event::Start { warnings: initial } => warnings.extend(initial),
            Event::Warning { warning } => warnings.push(warning),
            Event::ResponseMetadata { metadata } => response.merge(metadata),
            Event::Source { source } => sources.push(source),
            Event::BlockEnd { id, block } => {
                content.insert(id, block);
            }
            Event::Citation { id, citation } => {
                citations.entry(id).or_default().push(citation);
            }
            Event::Finish { reason, usage } => {
                for (id, links) in citations {
                    if let Some(Block {
                        content: Content::Text(text),
                        ..
                    }) = content.get_mut(&id)
                    {
                        for link in links {
                            if !text.citations.contains(&link) {
                                text.citations.push(link);
                            }
                        }
                    }
                }
                return Ok(GenerateResult {
                    response,
                    warnings,
                    sources,
                    content: content.into_values().collect(),
                    finish_reason: reason,
                    usage,
                });
            }
            Event::Error { error } => return Err(error),
            _ => {}
        }
    }
    Err(Error::invalid(
        "generation stream ended without a finish event",
    ))
}

/// Native event decoder, also useful when a host already owns SSE framing.
/// JSON is decoded into the native event enum exactly once.
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
pub struct StreamDecoder {
    state: State,
    started: bool,
    terminal: bool,
    initial_warnings: Vec<Warning>,
    seen_warnings: Vec<Warning>,
    response: ResponseMetadata,
    sources: Vec<Source>,
}
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
enum State {
    #[cfg(feature = "claude")]
    Claude {
        blocks: BTreeMap<u32, gproxy_protocol::claude::generate_content::ResponseContentBlock>,
        texts: BTreeMap<u32, Block>,
        args: BTreeMap<u32, String>,
        usage: Option<Usage>,
        reason: Option<gproxy_protocol::claude::generate_content::StopReason>,
        stopped: bool,
    },
    #[cfg(feature = "openai")]
    Openai { stopped: bool },
    #[cfg(feature = "chat")]
    Chat(Box<dialects::chat::StreamState>),
    #[cfg(feature = "gemini")]
    Gemini {
        next_index: u32,
        pending_audio: Option<Box<(BlockId, Block)>>,
        has_tool_calls: bool,
        usage: Usage,
        reason: Option<gproxy_protocol::gemini::FinishReason>,
    },
}
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
impl StreamDecoder {
    pub fn new(dialect: Dialect) -> Self {
        Self::with_audio_format(dialect, None)
    }
    pub fn with_audio_format(dialect: Dialect, _format: Option<AudioFormat>) -> Self {
        Self {
            started: false,
            terminal: false,
            initial_warnings: Vec::new(),
            seen_warnings: Vec::new(),
            response: ResponseMetadata::default(),
            sources: Vec::new(),
            state: match dialect {
                #[cfg(feature = "claude")]
                Dialect::Claude => State::Claude {
                    blocks: BTreeMap::new(),
                    texts: BTreeMap::new(),
                    args: BTreeMap::new(),
                    usage: None,
                    reason: None,
                    stopped: false,
                },
                #[cfg(feature = "openai")]
                Dialect::Openai => State::Openai { stopped: false },
                #[cfg(feature = "chat")]
                Dialect::OpenaiChat => {
                    State::Chat(Box::new(dialects::chat::StreamState::new(_format)))
                }
                #[cfg(feature = "gemini")]
                Dialect::Gemini => State::Gemini {
                    next_index: 0,
                    pending_audio: None,
                    has_tool_calls: false,
                    usage: Usage::default(),
                    reason: None,
                },
            },
        }
    }
    pub fn with_warnings(
        dialect: Dialect,
        format: Option<AudioFormat>,
        warnings: Vec<Warning>,
    ) -> Self {
        let mut decoder = Self::with_audio_format(dialect, format);
        decoder.initial_warnings = warnings;
        decoder
    }
    fn start(&mut self) -> Vec<Event> {
        if self.started {
            Vec::new()
        } else {
            self.started = true;
            vec![Event::Start {
                warnings: std::mem::take(&mut self.initial_warnings),
            }]
        }
    }
    fn observed_usage(&self) -> Usage {
        match &self.state {
            #[cfg(feature = "claude")]
            State::Claude { usage, .. } => usage.clone().unwrap_or_default(),
            #[cfg(feature = "chat")]
            State::Chat(chat) => chat.usage(),
            #[cfg(feature = "gemini")]
            State::Gemini { usage, .. } => usage.clone(),
            #[cfg(feature = "openai")]
            State::Openai { .. } => Usage::default(),
        }
    }
    pub fn push_json(&mut self, data: &str) -> Result<Vec<Event>> {
        if self.terminal {
            return Ok(Vec::new());
        }
        let mut out = self.start();
        let events = match self.decode_json(data) {
            Ok(events) => events,
            Err(error)
                if error.body.is_some()
                    && !matches!(
                        error.kind,
                        crate::ErrorKind::InvalidData | crate::ErrorKind::Network
                    ) =>
            {
                vec![
                    Event::Error { error },
                    Event::Finish {
                        reason: FinishReason::new(FinishReasonKind::Error, None),
                        usage: self.observed_usage(),
                    },
                ]
            }
            Err(error) => return Err(error.with_body(data.as_bytes())),
        };
        for event in events {
            match event {
                Event::ResponseMetadata { metadata } => {
                    let before = self.response.clone();
                    self.response.merge(metadata);
                    if self.response != before {
                        out.push(Event::ResponseMetadata {
                            metadata: self.response.clone(),
                        });
                    }
                }
                Event::Source { source } => {
                    if !self.sources.contains(&source) {
                        self.sources.push(source.clone());
                        out.push(Event::Source { source });
                    }
                }
                Event::Warning { warning } => {
                    if !self.seen_warnings.contains(&warning) {
                        self.seen_warnings.push(warning.clone());
                        out.push(Event::Warning { warning });
                    }
                }
                Event::Error { error } => out.push(Event::Error {
                    error: error.with_body(data.as_bytes()),
                }),
                Event::Finish { reason, usage } => {
                    self.terminal = true;
                    out.push(Event::Finish { reason, usage });
                }
                event => out.push(event),
            }
        }
        Ok(out)
    }
    fn decode_json(&mut self, data: &str) -> Result<Vec<Event>> {
        match &mut self.state {
            #[cfg(feature = "chat")]
            State::Chat(chat) => chat.push(data),
            #[cfg(feature = "claude")]
            State::Claude {
                blocks,
                texts,
                args,
                usage,
                reason,
                stopped,
            } => {
                use gproxy_protocol::claude::{generate_content as c, stream as s};
                let event: s::StreamEvent = serde_json::from_str(data).map_err(Error::invalid)?;
                let mut out = Vec::new();
                match event {
                    s::StreamEvent::MessageStart(event) => {
                        out.push(Event::ResponseMetadata {
                            metadata: ResponseMetadata {
                                id: Some(event.message.id.clone()),
                                model: Some(event.message.model.clone()),
                                timestamp: None,
                            },
                        });
                        *usage = Some(dialects::claude::parse_usage(&event.message.usage)?);
                    }
                    s::StreamEvent::ContentBlockStart(event) => {
                        let index = index(event.index)?;
                        let id = BlockId::item(index);
                        match &event.content_block {
                            c::ResponseContentBlock::ToolUse(value) => {
                                out.push(Event::ToolCallStart {
                                    id,
                                    call_id: value.id.clone(),
                                    name: value.name.clone(),
                                    provider_executed: false,
                                })
                            }
                            c::ResponseContentBlock::ServerToolUse(value) => {
                                out.push(Event::ToolCallStart {
                                    id,
                                    call_id: value.id.clone(),
                                    name: serde_json::from_value(
                                        serde_json::to_value(value.name).map_err(Error::invalid)?,
                                    )
                                    .map_err(Error::invalid)?,
                                    provider_executed: true,
                                })
                            }
                            c::ResponseContentBlock::Text(value) if !value.text.is_empty() => out
                                .push(Event::TextDelta {
                                    id,
                                    delta: value.text.clone(),
                                }),
                            c::ResponseContentBlock::Thinking(value)
                                if !value.thinking.is_empty() =>
                            {
                                out.push(Event::ReasoningDelta {
                                    id,
                                    delta: value.thinking.clone(),
                                })
                            }
                            _ => {}
                        }
                        blocks.insert(index, event.content_block);
                    }
                    s::StreamEvent::ContentBlockDelta(event) => {
                        let index = index(event.index)?;
                        let id = BlockId::item(index);
                        let block = blocks
                            .get_mut(&index)
                            .ok_or_else(|| Error::invalid("Claude delta before block start"))?;
                        match (block, event.delta) {
                            (
                                c::ResponseContentBlock::Text(value),
                                s::ContentBlockDelta::Text(delta),
                            ) => {
                                value.text.push_str(&delta.text);
                                out.push(Event::TextDelta {
                                    id,
                                    delta: delta.text,
                                });
                            }
                            (
                                c::ResponseContentBlock::Thinking(value),
                                s::ContentBlockDelta::Thinking(delta),
                            ) => {
                                value.thinking.push_str(&delta.thinking);
                                out.push(Event::ReasoningDelta {
                                    id,
                                    delta: delta.thinking,
                                });
                            }
                            (
                                c::ResponseContentBlock::Thinking(value),
                                s::ContentBlockDelta::Signature(delta),
                            ) => value.signature.push_str(&delta.signature),
                            (
                                c::ResponseContentBlock::ToolUse(_)
                                | c::ResponseContentBlock::ServerToolUse(_),
                                s::ContentBlockDelta::InputJson(delta),
                            ) => {
                                args.entry(index).or_default().push_str(&delta.partial_json);
                                out.push(Event::ToolCallArgsDelta {
                                    id,
                                    delta: delta.partial_json,
                                });
                            }
                            (
                                c::ResponseContentBlock::Text(value),
                                s::ContentBlockDelta::Citations(delta),
                            ) => {
                                value
                                    .citations
                                    .get_or_insert_with(|| Some(Vec::new()))
                                    .get_or_insert_with(Vec::new)
                                    .push(delta.citation.clone());
                                out.push(Event::Citation {
                                    id,
                                    citation: dialects::citations::parse_claude(delta.citation)?,
                                });
                            }
                            (
                                c::ResponseContentBlock::Compaction(value),
                                s::ContentBlockDelta::Compaction(delta),
                            ) => {
                                if let Some(Some(text)) = delta.content {
                                    value
                                        .content
                                        .get_or_insert_with(|| Some(String::new()))
                                        .get_or_insert_with(String::new)
                                        .push_str(&text);
                                }
                                if let Some(Some(text)) = delta.encrypted_content {
                                    value
                                        .encrypted_content
                                        .get_or_insert_with(|| Some(String::new()))
                                        .get_or_insert_with(String::new)
                                        .push_str(&text);
                                }
                            }
                            _ => {
                                return Err(Error::invalid(
                                    "Claude delta does not match its native block type",
                                ));
                            }
                        }
                    }
                    s::StreamEvent::ContentBlockStop(event) => {
                        let index = index(event.index)?;
                        let id = BlockId::item(index);
                        let mut block = blocks
                            .remove(&index)
                            .ok_or_else(|| Error::invalid("Claude stop before block start"))?;
                        if let Some(arguments) = args.remove(&index) {
                            match &mut block {
                                c::ResponseContentBlock::ToolUse(value) => {
                                    value.input =
                                        serde_json::from_str(&arguments).map_err(Error::invalid)?
                                }
                                c::ResponseContentBlock::ServerToolUse(value) => {
                                    value.input =
                                        serde_json::from_str(&arguments).map_err(Error::invalid)?
                                }
                                _ => {
                                    return Err(Error::invalid(
                                        "tool arguments belong to a non-tool block",
                                    ));
                                }
                            }
                        }
                        if matches!(
                            block,
                            c::ResponseContentBlock::ToolUse(_)
                                | c::ResponseContentBlock::ServerToolUse(_)
                        ) {
                            out.push(Event::ToolCallEnd { id });
                        }
                        let sources = crate::sources::claude_block(&block)?;
                        for source in sources {
                            out.push(Event::Source { source });
                        }
                        let block = dialects::claude::parse_block(block)?;
                        if matches!(block.content, Content::Text(_)) {
                            texts.insert(index, block);
                        } else {
                            out.push(Event::BlockEnd { id, block });
                        }
                    }
                    s::StreamEvent::MessageDelta(event) => {
                        let current = usage.as_mut().ok_or_else(|| {
                            Error::invalid("Claude message delta before message start")
                        })?;
                        current.output_tokens = Some(dialects::count(event.usage.output_tokens)?);
                        if let Some(value) = event.usage.input_tokens {
                            current.input_tokens = dialects::optional_count(value)?;
                        }
                        if let Some(value) = event.usage.cache_read_input_tokens {
                            current.cache_read_tokens = dialects::optional_count(value)?;
                        }
                        if let Some(value) = event.usage.cache_creation_input_tokens {
                            current.cache_write_tokens = dialects::optional_count(value)?;
                        }
                        if let Some(value) = event.usage.output_tokens_details {
                            current.reasoning_tokens =
                                dialects::optional_count(value.map(|v| v.thinking_tokens))?;
                        }
                        if let Some(Some(value)) = event.delta.stop_reason {
                            *reason = Some(value);
                            for (index, mut block) in std::mem::take(texts) {
                                if let Content::Text(text) = &mut block.content {
                                    text.refusal = value == c::StopReason::Refusal;
                                }
                                out.push(Event::BlockEnd {
                                    id: BlockId::item(index),
                                    block,
                                });
                            }
                        }
                    }
                    s::StreamEvent::MessageStop(_) => {
                        if !blocks.is_empty() {
                            return Err(Error::invalid("Claude stopped before block completion"));
                        }
                        let reason = reason.ok_or_else(|| {
                            Error::invalid("Claude stopped without a finish reason")
                        })?;
                        let usage = usage
                            .take()
                            .ok_or_else(|| Error::invalid("Claude stopped before message start"))?;
                        *stopped = true;
                        out.push(Event::Finish {
                            reason: dialects::claude::finish_reason(reason),
                            usage,
                        });
                    }
                    s::StreamEvent::Error(_) => {
                        return Err(Error::from_event(data.to_owned()));
                    }
                    s::StreamEvent::Ping(_) => {}
                }
                Ok(out)
            }
            #[cfg(feature = "openai")]
            State::Openai { stopped } => {
                use gproxy_protocol::openai::responses::{response as r, stream as s};
                let event: s::StreamEvent = serde_json::from_str(data).map_err(Error::invalid)?;
                let mut out = Vec::new();
                match event {
                    s::StreamEvent::Created(value) => out.push(Event::ResponseMetadata {
                        metadata: dialects::openai::response_metadata(&value.response),
                    }),
                    s::StreamEvent::InProgress(value) => out.push(Event::ResponseMetadata {
                        metadata: dialects::openai::response_metadata(&value.response),
                    }),
                    s::StreamEvent::OutputTextDelta(value) => out.push(Event::TextDelta {
                        id: BlockId {
                            item: index(value.output_index)?,
                            part: index(value.content_index)?,
                        },
                        delta: value.delta,
                    }),
                    s::StreamEvent::OutputTextAnnotationAdded(value) => {
                        let annotation =
                            serde_json::from_value(value.annotation).map_err(Error::invalid)?;
                        let citation = dialects::citations::parse_responses(annotation)?;
                        {
                            out.push(Event::Citation {
                                id: BlockId {
                                    item: index(value.output_index)?,
                                    part: index(value.content_index)?,
                                },
                                citation,
                            });
                        }
                    }
                    s::StreamEvent::RefusalDelta(value) => out.push(Event::TextDelta {
                        id: BlockId {
                            item: index(value.output_index)?,
                            part: index(value.content_index)?,
                        },
                        delta: value.delta,
                    }),
                    s::StreamEvent::ReasoningTextDelta(value) => out.push(Event::ReasoningDelta {
                        id: BlockId::item(index(value.output_index)?),
                        delta: value.delta,
                    }),
                    s::StreamEvent::ReasoningSummaryTextDelta(value) => {
                        out.push(Event::ReasoningDelta {
                            id: BlockId::item(index(value.output_index)?),
                            delta: value.delta,
                        })
                    }
                    s::StreamEvent::OutputItemAdded(value) => {
                        let call = match value.item {
                            r::ResponseOutputItem::FunctionCall(call) => {
                                Some((call.call_id, call.name, false))
                            }
                            r::ResponseOutputItem::WebSearchCall(call) => {
                                Some((call.id, "web_search".into(), true))
                            }
                            r::ResponseOutputItem::FileSearchCall(call) => {
                                Some((call.id, "file_search".into(), true))
                            }
                            r::ResponseOutputItem::CodeInterpreterCall(call) => {
                                Some((call.id, "code_interpreter".into(), true))
                            }
                            r::ResponseOutputItem::ComputerCall(call) => {
                                Some((call.call_id, "computer".into(), true))
                            }
                            _ => None,
                        };
                        if let Some((call_id, name, provider_executed)) = call {
                            out.push(Event::ToolCallStart {
                                id: BlockId::item(index(value.output_index)?),
                                call_id,
                                name,
                                provider_executed,
                            });
                        }
                    }
                    s::StreamEvent::FunctionCallArgumentsDelta(value) => {
                        out.push(Event::ToolCallArgsDelta {
                            id: BlockId::item(index(value.output_index)?),
                            delta: value.delta,
                        })
                    }
                    s::StreamEvent::OutputItemDone(value) => {
                        let item = index(value.output_index)?;
                        for source in crate::sources::openai_item(&value.item)? {
                            out.push(Event::Source { source });
                        }
                        for (part, block) in dialects::openai::parse_item(value.item, item)?
                            .into_iter()
                            .enumerate()
                        {
                            let id = BlockId {
                                item,
                                part: part as u32,
                            };
                            if matches!(block.content, Content::ToolCall(_)) {
                                out.push(Event::ToolCallEnd { id });
                            }
                            out.push(Event::BlockEnd { id, block });
                        }
                    }
                    s::StreamEvent::Completed(value) => {
                        out.extend(responses_finish(&value.response, stopped)?)
                    }
                    s::StreamEvent::Incomplete(value) => {
                        out.extend(responses_finish(&value.response, stopped)?)
                    }
                    s::StreamEvent::Failed(value) => {
                        out.extend(responses_finish(&value.response, stopped)?)
                    }
                    s::StreamEvent::Error(_) => {
                        return Err(Error::from_event(data.to_owned()));
                    }
                    _ => {}
                }
                Ok(out)
            }
            #[cfg(feature = "gemini")]
            State::Gemini {
                next_index,
                pending_audio,
                has_tool_calls,
                usage,
                reason,
            } => {
                use gproxy_protocol::gemini as g;
                let chunk: g::GenerateContentResponseBody =
                    serde_json::from_str(data).map_err(Error::invalid)?;
                let mut out = vec![Event::ResponseMetadata {
                    metadata: dialects::gemini::response_metadata(&chunk),
                }];
                if let Err(error) = dialects::gemini::check_response(&chunk) {
                    out.push(Event::Error { error });
                    out.push(Event::Finish {
                        reason: FinishReason::new(
                            FinishReasonKind::Error,
                            chunk
                                .candidates
                                .as_ref()
                                .and_then(|v| v.first())
                                .and_then(|c| c.finish_reason)
                                .and_then(|reason| {
                                    dialects::gemini::finish_reason(Some(reason), false).raw
                                }),
                        ),
                        usage: dialects::gemini::parse_usage(chunk.usage_metadata.as_ref())?,
                    });
                    return Ok(out);
                }
                if chunk
                    .candidates
                    .as_ref()
                    .is_some_and(|values| values.len() > 1)
                {
                    out.push(Event::Warning {
                        warning: Warning::new(
                            WarningKind::Compatibility,
                            "response.candidates",
                            "Only the first candidate is returned",
                        ),
                    });
                }
                if let Some(candidate) = chunk.candidates.into_iter().flatten().next() {
                    for source in crate::sources::gemini_candidate(&candidate)? {
                        out.push(Event::Source { source });
                    }
                    for (part, citation) in dialects::citations::gemini_citations(&candidate)? {
                        out.push(Event::Citation {
                            id: BlockId::item(part.try_into().map_err(Error::invalid)?),
                            citation,
                        });
                    }
                    for part in candidate
                        .content
                        .into_iter()
                        .flat_map(|c| c.parts.into_iter().flatten())
                    {
                        let id = BlockId::item(*next_index);
                        if let Some(pending) = pending_audio.as_mut()
                            && let Some(delta) =
                                dialects::gemini::merge_transcription(&mut pending.1, &part)?
                        {
                            out.push(Event::TranscriptDelta {
                                id: pending.0,
                                delta,
                            });
                            *next_index += 1;
                            continue;
                        }
                        if let Some(pending) = pending_audio.take() {
                            let (id, block) = *pending;
                            out.push(Event::BlockEnd { id, block });
                        }
                        let block = dialects::gemini::parse_block(part, id.item)?;
                        match &block.content {
                            Content::Text(Text { text, .. }) => out.push(Event::TextDelta {
                                id,
                                delta: text.clone(),
                            }),
                            Content::Reasoning(Reasoning { text }) => {
                                out.push(Event::ReasoningDelta {
                                    id,
                                    delta: text.clone(),
                                })
                            }
                            Content::Audio(Audio {
                                source: Asset::Inline { bytes },
                                format,
                                transcript,
                            }) => {
                                out.push(Event::AudioDelta {
                                    id,
                                    bytes: bytes.clone(),
                                    format: format.clone(),
                                });
                                if let Some(transcript) = transcript {
                                    out.push(Event::TranscriptDelta {
                                        id,
                                        delta: transcript.clone(),
                                    });
                                }
                            }
                            Content::ToolCall(ToolCall {
                                id: call_id,
                                name,
                                arguments,
                                provider_executed,
                            }) => {
                                *has_tool_calls = true;
                                out.push(Event::ToolCallStart {
                                    id,
                                    call_id: call_id.clone(),
                                    name: name.clone(),
                                    provider_executed: *provider_executed,
                                });
                                out.push(Event::ToolCallArgsDelta {
                                    id,
                                    delta: arguments.clone(),
                                });
                                out.push(Event::ToolCallEnd { id });
                            }
                            _ => {}
                        }
                        if matches!(block.content, Content::Audio(_)) {
                            *pending_audio = Some(Box::new((id, block)));
                        } else {
                            out.push(Event::BlockEnd { id, block });
                        }
                        *next_index += 1;
                    }
                    if let Some(value) = candidate.finish_reason {
                        *reason = Some(value);
                    }
                }
                if let Some(value) = chunk.usage_metadata {
                    if value.prompt_token_count.is_some() {
                        usage.input_tokens = dialects::optional_count(value.prompt_token_count)?;
                    }
                    if value.candidates_token_count.is_some() {
                        usage.output_tokens =
                            dialects::optional_count(value.candidates_token_count)?;
                    }
                    if value.thoughts_token_count.is_some() {
                        usage.reasoning_tokens =
                            dialects::optional_count(value.thoughts_token_count)?;
                    }
                    if value.cached_content_token_count.is_some() {
                        usage.cache_read_tokens =
                            dialects::optional_count(value.cached_content_token_count)?;
                    }
                }
                Ok(out)
            }
        }
    }
    pub fn finish(mut self) -> Result<Vec<Event>> {
        let mut out = self.start();
        if self.terminal {
            return Ok(out);
        }
        match self.state {
            #[cfg(feature = "chat")]
            State::Chat(chat) => out.extend(chat.finish()?),
            #[cfg(feature = "claude")]
            State::Claude {
                blocks,
                texts,
                args,
                usage,
                reason,
                ..
            } => {
                let refused =
                    reason == Some(gproxy_protocol::claude::generate_content::StopReason::Refusal);
                for (index, mut block) in texts {
                    if let Content::Text(text) = &mut block.content {
                        text.refusal = refused;
                    }
                    out.push(Event::BlockEnd {
                        id: BlockId::item(index),
                        block,
                    });
                }
                for (index, native) in blocks {
                    let mut block = dialects::claude::parse_block(native)?;
                    if let Content::Text(text) = &mut block.content {
                        text.refusal = refused;
                    }
                    if let (Some(arguments), Content::ToolCall(call)) =
                        (args.get(&index), &mut block.content)
                    {
                        call.arguments = arguments.clone();
                    }
                    out.push(Event::BlockEnd {
                        id: BlockId::item(index),
                        block,
                    });
                }
                out.push(Event::Finish {
                    reason: FinishReason::new(
                        FinishReasonKind::Interrupted,
                        reason.and_then(|reason| dialects::claude::finish_reason(reason).raw),
                    ),
                    usage: usage.unwrap_or_default(),
                });
            }
            #[cfg(feature = "openai")]
            State::Openai { .. } => out.push(Event::Finish {
                reason: FinishReason::new(FinishReasonKind::Interrupted, None),
                usage: Usage::default(),
            }),
            #[cfg(feature = "gemini")]
            State::Gemini {
                reason,
                has_tool_calls,
                usage,
                pending_audio,
                ..
            } => {
                if let Some(pending) = pending_audio {
                    let (id, block) = *pending;
                    out.push(Event::BlockEnd { id, block });
                }
                let mut finish = dialects::gemini::finish_reason(reason, has_tool_calls);
                if reason.is_none()
                    || reason == Some(gproxy_protocol::gemini::FinishReason::Unspecified)
                {
                    finish.unified = FinishReasonKind::Interrupted;
                }
                out.push(Event::Finish {
                    reason: finish,
                    usage,
                });
            }
        }
        Ok(out)
    }
}
#[cfg(feature = "openai")]
fn responses_finish(
    response: &gproxy_protocol::openai::responses::response::GenerateContentResponseBody,
    stopped: &mut bool,
) -> Result<Vec<Event>> {
    let mut out = vec![Event::ResponseMetadata {
        metadata: dialects::openai::response_metadata(response),
    }];
    match dialects::openai::finish_metadata(response) {
        Ok((reason, usage)) => out.push(Event::Finish { reason, usage }),
        Err(error) => {
            out.push(Event::Error { error });
            out.push(Event::Finish {
                reason: dialects::openai::error_finish_reason(response),
                usage: dialects::openai::parse_usage(response)?,
            });
        }
    }
    *stopped = true;
    Ok(out)
}
#[cfg(any(feature = "claude", feature = "openai"))]
fn index(value: i64) -> Result<u32> {
    value.try_into().map_err(Error::invalid)
}
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
pub(crate) fn decode(
    body: HttpBody,
    request: &GenerateRequest,
    limits: gproxy_protocol::codec::CodecLimits,
    warnings: Vec<Warning>,
    http_status: u16,
    headers: http::HeaderMap,
) -> Result<GenerationStream> {
    #[cfg(feature = "chat")]
    let format = if request.model.dialect == Dialect::OpenaiChat {
        dialects::chat::output_format(request)?
    } else {
        None
    };
    #[cfg(not(feature = "chat"))]
    let format = None;
    struct Reader {
        bytes: ByteStream,
        sse: SseDecoder,
        decoder: Option<StreamDecoder>,
        queue: VecDeque<Event>,
        done: bool,
        http_status: u16,
        headers: http::HeaderMap,
    }
    let bytes = match body {
        HttpBody::Stream(bytes) => bytes,
        HttpBody::Bytes(bytes) => Box::pin(stream::once(async { Ok(bytes) })) as ByteStream,
    };
    let mut decoder = StreamDecoder::with_warnings(request.model.dialect, format, warnings);
    let queue = decoder.start().into();
    let state = Reader {
        bytes,
        sse: SseDecoder::new(limits),
        decoder: Some(decoder),
        queue,
        done: false,
        http_status,
        headers,
    };
    let inner = stream::unfold(state, |mut state| async move {
        loop {
            if let Some(event) = state.queue.pop_front() {
                let event = match event {
                    Event::Error { error } => Event::Error {
                        error: error.with_head(state.http_status, &state.headers),
                    },
                    event => event,
                };
                if matches!(event, Event::Finish { .. } | Event::Error { .. }) {
                    state.done = true;
                }
                return Some((event, state));
            }
            if state.done {
                return None;
            }
            let (frames, eof) = match state.bytes.next().await {
                Some(Ok(bytes)) => (state.sse.push(&bytes).map_err(Error::invalid), false),
                Some(Err(error)) => (Err(Error::network_cause(error.as_ref())), false),
                None => (state.sse.finish().map_err(Error::invalid), true),
            };
            let ended = frames
                .as_ref()
                .is_ok_and(|f| f.iter().any(|f| matches!(f, SseFrame::Done)));
            // A transport EOF and a [DONE] both end framing, never synthesize a
            // successful vendor finish from a dropped connection.
            let result = (|| -> Result<()> {
                for frame in frames? {
                    if let SseFrame::Event(event) = frame {
                        state.queue.extend(
                            state
                                .decoder
                                .as_mut()
                                .ok_or_else(|| Error::invalid("event after stream completion"))?
                                .push_json(&event.data)?,
                        );
                    }
                }
                if ended || eof {
                    state.queue.extend(
                        state
                            .decoder
                            .take()
                            .ok_or_else(|| Error::invalid("stream already completed"))?
                            .finish()?,
                    );
                    state.done = true;
                }
                Ok(())
            })();
            if let Err(error) = result {
                let usage = state
                    .decoder
                    .as_ref()
                    .map(StreamDecoder::observed_usage)
                    .unwrap_or_default();
                state.queue.push_back(Event::Error { error });
                state.queue.push_back(Event::Finish {
                    reason: FinishReason::new(FinishReasonKind::Interrupted, None),
                    usage,
                });
                state.done = true;
            }
        }
    });
    Ok(GenerationStream {
        inner: Box::pin(inner),
    })
}
