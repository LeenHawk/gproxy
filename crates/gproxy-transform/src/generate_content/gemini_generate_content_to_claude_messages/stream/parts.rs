use gproxy_protocol::{claude, gemini};

use crate::TransformError;

use super::events;
use super::state::{OpenKind, State};

impl State {
    pub(super) fn part(
        &mut self,
        part: gemini::Part,
    ) -> Result<Vec<claude::StreamEvent>, TransformError> {
        let kind = if part.thought == Some(true) {
            OpenKind::Thinking
        } else {
            OpenKind::Text
        };
        match part.data.as_ref() {
            Some(gemini::PartData::Text { .. }) => self.text_part(part, kind),
            None if part.thought_signature.is_some() => self.signature_only(part),
            _ => self.closed_part(part),
        }
    }

    fn text_part(
        &mut self,
        part: gemini::Part,
        kind: OpenKind,
    ) -> Result<Vec<claude::StreamEvent>, TransformError> {
        let mut output = Vec::new();
        if self.open.as_ref().is_some_and(|open| open.kind != kind) {
            output.extend(self.close_open()?);
        }
        let index = match &self.open {
            Some(open) => open.index,
            None => {
                let index = self.next_block(kind);
                let block = match kind {
                    OpenKind::Text => claude::ResponseContentBlock::Text(crate::wire!(
                        claude::ResponseTextBlock {
                            citations: None,
                            text: String::new(),
                            type_: claude::TextBlockType::Text,
                            rest: Default::default(),
                        }
                    )),
                    OpenKind::Thinking => claude::ResponseContentBlock::Thinking(crate::wire!(
                        claude::ThinkingBlock {
                            signature: None,
                            thinking: String::new(),
                            type_: claude::ThinkingBlockType::Thinking,
                            rest: Default::default(),
                        }
                    )),
                };
                output.push(events::wrap(events::block_start(index, block)));
                index
            }
        };
        let Some(gemini::PartData::Text { text, .. }) = part.data else {
            return Err(TransformError::shape(
                "Gemini stream",
                "text part changed during conversion",
            ));
        };
        if !text.is_empty() {
            let delta = match kind {
                OpenKind::Text => claude::KnownEventDelta::Text {
                    text,
                    rest: Default::default(),
                },
                OpenKind::Thinking => claude::KnownEventDelta::Thinking {
                    estimated_tokens: None,
                    thinking: text,
                    rest: Default::default(),
                },
            };
            output.push(events::wrap(events::block_delta(index, delta)));
        }
        if kind == OpenKind::Thinking
            && let Some(signature) = part.thought_signature
        {
            output.push(events::wrap(events::block_delta(
                index,
                claude::KnownEventDelta::Signature {
                    signature,
                    rest: Default::default(),
                },
            )));
        }
        Ok(output)
    }

    fn signature_only(
        &mut self,
        part: gemini::Part,
    ) -> Result<Vec<claude::StreamEvent>, TransformError> {
        let signature = part
            .thought_signature
            .ok_or_else(|| TransformError::shape("Gemini stream", "signature is missing"))?;
        let Some(open) = self.open.as_ref() else {
            self.pending_signature = Some(signature);
            return Ok(Vec::new());
        };
        if open.kind != OpenKind::Thinking {
            self.pending_signature = Some(signature);
            return Ok(Vec::new());
        }
        Ok(vec![events::wrap(events::block_delta(
            open.index,
            claude::KnownEventDelta::Signature {
                signature,
                rest: Default::default(),
            },
        ))])
    }

    fn closed_part(
        &mut self,
        part: gemini::Part,
    ) -> Result<Vec<claude::StreamEvent>, TransformError> {
        let mut output = self.close_open()?;
        if matches!(part.data, Some(gemini::PartData::FunctionCall { .. }))
            && let Some(signature) = part
                .thought_signature
                .clone()
                .or(self.pending_signature.take())
        {
            let index = self.next_index;
            self.next_index = self.next_index.saturating_add(1);
            let block = claude::ResponseContentBlock::RedactedThinking(crate::wire!(
                claude::RedactedThinkingBlock {
                    data: signature,
                    type_: claude::RedactedThinkingBlockType::RedactedThinking,
                    rest: Default::default(),
                }
            ));
            output.push(events::wrap(events::block_start(index, block)));
            output.push(events::wrap(events::block_stop(index)));
        } else {
            self.pending_signature = None;
        }
        let Some(block) = super::super::content::response_part(part, &mut self.correlation)? else {
            return Ok(output);
        };
        let index = self.next_index;
        self.next_index = self.next_index.saturating_add(1);
        if let claude::ResponseContentBlock::ToolUse(mut tool) = block {
            self.has_tool = true;
            let input = std::mem::take(&mut tool.input);
            output.push(events::wrap(events::block_start(
                index,
                claude::ResponseContentBlock::ToolUse(tool),
            )));
            output.push(events::wrap(events::block_delta(
                index,
                claude::KnownEventDelta::InputJson {
                    partial_json: serde_json::to_string(&input)?,
                    rest: Default::default(),
                },
            )));
        } else {
            self.has_tool |= matches!(&block, claude::ResponseContentBlock::ServerToolUse(_));
            output.push(events::wrap(events::block_start(index, block)));
        }
        output.push(events::wrap(events::block_stop(index)));
        Ok(output)
    }
}
