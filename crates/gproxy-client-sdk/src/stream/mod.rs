//! Streams own response bodies. Dropping or cancelling consumes no background task.
use crate::{Error, Result, ir::*};
use futures_core::Stream;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, pin::Pin};
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct BlockId {
    pub item: u32,
    pub part: u32,
}

#[cfg(any(feature = "claude", feature = "openai", feature = "gemini"))]
impl BlockId {
    pub(crate) fn item(item: u32) -> Self {
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
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
mod decoder;
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
pub use decoder::StreamDecoder;
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
mod sse;
#[cfg(any(
    feature = "claude",
    feature = "openai",
    feature = "chat",
    feature = "gemini"
))]
pub(crate) use sse::decode;
