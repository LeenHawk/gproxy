use super::*;
use crate::dialects;
use crate::stream::{BlockId, Event};
pub(crate) struct StreamState {
    next_index: u32,
    pending_audio: Option<Box<(BlockId, Block)>>,
    /// Text/reasoning arrives split across chunks; it stays open so the
    /// aggregated block matches the non-streaming single-part shape.
    pending_text: Option<Box<(BlockId, Block)>>,
    has_tool_calls: bool,
    usage: Usage,
    reason: Option<gproxy_protocol::gemini::FinishReason>,
}
impl StreamState {
    pub(crate) fn new() -> Self {
        Self {
            next_index: 0,
            pending_audio: None,
            pending_text: None,
            has_tool_calls: false,
            usage: Usage::default(),
            reason: None,
        }
    }
    pub(crate) fn usage(&self) -> Usage {
        self.usage.clone()
    }
    pub(crate) fn push(&mut self, data: &str) -> Result<Vec<Event>> {
        let Self {
            next_index,
            pending_audio,
            pending_text,
            has_tool_calls,
            usage,
            reason,
        } = self;

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
            for source in crate::dialects::shared::sources::gemini_candidate(&candidate)? {
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
                if let Some(pending) = pending_text.as_mut()
                    && let Some(delta) = append(&mut pending.1, &block)
                {
                    out.push(delta.into_event(pending.0));
                    *next_index += 1;
                    continue;
                }
                if let Some(pending) = pending_text.take() {
                    let (id, block) = *pending;
                    out.push(Event::BlockEnd { id, block });
                }
                match &block.content {
                    Content::Text(Text { text, .. }) => out.push(Event::TextDelta {
                        id,
                        delta: text.clone(),
                    }),
                    Content::Reasoning(Reasoning { text }) => out.push(Event::ReasoningDelta {
                        id,
                        delta: text.clone(),
                    }),
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
                } else if matches!(block.content, Content::Text(_) | Content::Reasoning(_)) {
                    *pending_text = Some(Box::new((id, block)));
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
                usage.output_tokens = dialects::optional_count(value.candidates_token_count)?;
            }
            if value.thoughts_token_count.is_some() {
                usage.reasoning_tokens = dialects::optional_count(value.thoughts_token_count)?;
            }
            if value.cached_content_token_count.is_some() {
                usage.cache_read_tokens =
                    dialects::optional_count(value.cached_content_token_count)?;
            }
        }
        Ok(out)
    }
    pub(crate) fn finish(self) -> Result<Vec<Event>> {
        let Self {
            reason,
            has_tool_calls,
            usage,
            pending_audio,
            pending_text,
            ..
        } = self;
        let mut out = Vec::new();

        if let Some(pending) = pending_text {
            let (id, block) = *pending;
            out.push(Event::BlockEnd { id, block });
        }
        if let Some(pending) = pending_audio {
            let (id, block) = *pending;
            out.push(Event::BlockEnd { id, block });
        }
        let mut finish = dialects::gemini::finish_reason(reason, has_tool_calls);
        if reason.is_none() || reason == Some(gproxy_protocol::gemini::FinishReason::Unspecified) {
            finish.unified = FinishReasonKind::Interrupted;
        }
        out.push(Event::Finish {
            reason: finish,
            usage,
        });

        Ok(out)
    }
}

enum Delta {
    Text(String),
    Reasoning(String),
}
impl Delta {
    fn into_event(self, id: BlockId) -> Event {
        match self {
            Self::Text(delta) => Event::TextDelta { id, delta },
            Self::Reasoning(delta) => Event::ReasoningDelta { id, delta },
        }
    }
}

/// Appends `next` to the open block when it continues it: both plain text (or
/// both reasoning), same cache breakpoint, and part metadata identical apart from
/// a thought signature the open block does not have yet. The signature usually
/// arrives on the last fragment, so the later metadata replaces the open block's.
/// Returns None, leaving the open block unchanged, when `next` starts a new block.
fn append(open: &mut Block, next: &Block) -> Option<Delta> {
    let unsigned = |block: &Block| {
        let mut meta = block.provider_metadata.clone();
        let signed = meta
            .get_mut(&Vendor::Gemini)
            .and_then(|m| m.get_mut("native"))
            .and_then(|n| n.as_object_mut())
            .and_then(|n| n.remove("thoughtSignature"))
            .is_some();
        (meta, signed)
    };
    let (open_meta, open_signed) = unsigned(open);
    if open_signed || open.cache != next.cache || open_meta != unsigned(next).0 {
        return None;
    }
    let delta = match (&mut open.content, &next.content) {
        (Content::Text(open_text), Content::Text(text)) if open_text.refusal == text.refusal => {
            open_text.text.push_str(&text.text);
            open_text.citations.extend(text.citations.iter().cloned());
            Delta::Text(text.text.clone())
        }
        (Content::Reasoning(open_text), Content::Reasoning(text)) => {
            open_text.text.push_str(&text.text);
            Delta::Reasoning(text.text.clone())
        }
        _ => return None,
    };
    open.provider_metadata = next.provider_metadata.clone();
    Some(delta)
}
