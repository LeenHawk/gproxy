use super::*;
use crate::dialects;
use crate::stream::{BlockId, Event};
pub(crate) struct StreamState {
    next_index: u32,
    pending_audio: Option<Box<(BlockId, Block)>>,
    has_tool_calls: bool,
    usage: Usage,
    reason: Option<gproxy_protocol::gemini::FinishReason>,
}
impl StreamState {
    pub(crate) fn new() -> Self {
        Self {
            next_index: 0,
            pending_audio: None,
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
            ..
        } = self;
        let mut out = Vec::new();

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
