use super::parse::{optional_count_chat, parse_message_inner};
use super::*;
/// Native Chat chunks do not contain a complete audio object. Accumulate their
/// observed fields without inventing an audio ID or an expiration timestamp.
pub(super) fn append_nullable(target: &mut Option<Option<String>>, delta: Option<Option<String>>) {
    match delta {
        Some(Some(text)) => target
            .get_or_insert_with(|| Some(String::new()))
            .get_or_insert_with(String::new)
            .push_str(&text),
        Some(None) if target.is_none() => *target = Some(None),
        _ => {}
    }
}

pub(crate) struct StreamState {
    pub(super) message: r::ResponseMessage,
    pub(super) calls: BTreeMap<u32, c::ChatToolCall>,
    pub(super) audio_seen: bool,
    pub(super) audio: Vec<u8>,
    pub(super) transcript: String,
    pub(super) audio_id: Option<String>,
    pub(super) expires: Option<i64>,
    pub(super) audio_rest: Rest,
    pub(super) format: Option<AudioFormat>,
    pub(super) reason: Option<r::FinishReason>,
    pub(super) usage: Usage,
}

impl StreamState {
    pub(crate) fn new(format: Option<AudioFormat>) -> Self {
        Self {
            message: r::ResponseMessage::builder(None, None, r::ResponseRole::Assistant).build(),
            calls: BTreeMap::new(),
            audio_seen: false,
            audio: Vec::new(),
            transcript: String::new(),
            audio_id: None,
            expires: None,
            audio_rest: Rest::new(),
            format,
            reason: None,
            usage: Usage::default(),
        }
    }
    pub(crate) fn push(&mut self, data: &str) -> Result<Vec<crate::stream::Event>> {
        use crate::stream::Event;
        use gproxy_protocol::openai::chat::stream as s;
        // Error records are a separate envelope, not normal completion chunks.
        // OpenRouter can include an optional trailing choice with a raw reason.
        #[derive(Deserialize)]
        struct ErrorChoice {
            finish_reason: Option<String>,
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        #[allow(clippy::large_enum_variant)]
        enum Record {
            Error {
                #[serde(rename = "error")]
                _error: s::StreamError,
                #[serde(default)]
                choices: Vec<ErrorChoice>,
                id: Option<String>,
                model: Option<String>,
                created: Option<i64>,
            },
            Chunk(s::ChatCompletionChunk),
        }
        let chunk = match serde_json::from_str::<Record>(data).map_err(Error::invalid)? {
            Record::Error {
                _error: _,
                choices,
                id,
                model,
                created,
            } => {
                return Ok(vec![
                    Event::ResponseMetadata {
                        metadata: ResponseMetadata {
                            id,
                            model,
                            timestamp: created,
                        },
                    },
                    Event::Error {
                        error: Error::from_event(data.into()),
                    },
                    Event::Finish {
                        reason: FinishReason::new(
                            FinishReasonKind::Error,
                            choices
                                .into_iter()
                                .next()
                                .and_then(|choice| choice.finish_reason),
                        ),
                        usage: self.usage.clone(),
                    },
                ]);
            }
            Record::Chunk(chunk) => chunk,
        };
        let mut out = vec![Event::ResponseMetadata {
            metadata: ResponseMetadata {
                id: Some(chunk.id.clone()),
                model: Some(chunk.model.clone()),
                timestamp: Some(chunk.created),
            },
        }];
        if chunk.choices.len() > 1 {
            out.push(Event::Warning {
                warning: Warning::new(
                    WarningKind::Compatibility,
                    "response.choices",
                    "Only the first choice is returned",
                ),
            });
        }
        if let Some(choice) = chunk.choices.into_iter().next() {
            self.apply_choice(choice, &mut out)?;
        }
        if let Some(Some(value)) = chunk.usage {
            self.usage = Usage {
                input_tokens: Some(count(value.prompt_tokens)?),
                output_tokens: Some(count(value.completion_tokens)?),
                reasoning_tokens: optional_count_chat(
                    value
                        .completion_tokens_details
                        .as_ref()
                        .and_then(Option::as_ref)
                        .and_then(|v| v.reasoning_tokens.flatten()),
                )?,
                cache_read_tokens: optional_count_chat(
                    value
                        .prompt_tokens_details
                        .as_ref()
                        .and_then(Option::as_ref)
                        .and_then(|v| v.cached_tokens.flatten()),
                )?,
                cache_write_tokens: optional_count_chat(
                    value
                        .prompt_tokens_details
                        .as_ref()
                        .and_then(Option::as_ref)
                        .and_then(|v| v.cache_write_tokens.flatten()),
                )?,
            };
        }
        Ok(out)
    }
    pub(crate) fn usage(&self) -> Usage {
        self.usage.clone()
    }
    pub(crate) fn finish(mut self) -> Result<Vec<crate::stream::Event>> {
        use crate::stream::{BlockId, Event};
        let reason = self
            .reason
            .map(finish_reason)
            .unwrap_or_else(|| FinishReason::new(FinishReasonKind::Interrupted, None));
        self.message.tool_calls = (!self.calls.is_empty()).then(|| {
            self.calls
                .into_values()
                .map(c::MessageToolCall::Function)
                .collect()
        });
        let audio = if self.audio_seen {
            let value = Audio {
                source: Asset::Inline { bytes: self.audio },
                format: self
                    .format
                    .clone()
                    .ok_or_else(|| Error::invalid("audio stream requires its requested format"))?,
                transcript: Some(self.transcript),
            };
            let native = AudioMetadata {
                id: self.audio_id,
                expires_at: self.expires,
                rest: self.audio_rest,
            };
            Some(block(
                Content::Audio(value),
                Metadata {
                    audio: Some(native),
                    ..Default::default()
                },
            )?)
        } else {
            None
        };
        let blocks = parse_message_inner(self.message, self.format, audio)?;
        let mut events = Vec::new();
        for (item, block) in blocks {
            let id = BlockId { item, part: 0 };
            if matches!(block.content, Content::ToolCall(_)) {
                events.push(Event::ToolCallEnd { id });
            }
            if let Content::Text(text) = &block.content {
                for citation in &text.citations {
                    events.push(Event::Citation {
                        id,
                        citation: citation.clone(),
                    });
                }
            }
            events.push(Event::BlockEnd { id, block });
        }
        events.push(Event::Finish {
            reason,
            usage: self.usage,
        });
        Ok(events)
    }
}

mod choice;
