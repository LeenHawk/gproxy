use bytes::Bytes;
use gproxy_protocol::{claude, gemini};

use crate::TransformError;

use super::events;
use super::state::{OpenKind, State};

impl State {
    pub(super) fn part(&mut self, part: gemini::Part) -> Result<Vec<Bytes>, TransformError> {
        if part.thought == Some(false)
            || part.part_metadata.is_some()
            || part.media_resolution.is_some()
            || explicit_metadata(part.metadata.as_ref())
        {
            return Err(TransformError::unsupported(
                "Gemini stream part",
                "explicit part metadata",
            ));
        }
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
    ) -> Result<Vec<Bytes>, TransformError> {
        let mut output = Vec::new();
        if self.open.as_ref().is_some_and(|open| open.kind != kind) {
            output.extend(self.close_open()?);
        }
        let index = match &self.open {
            Some(open) => open.index,
            None => {
                let index = self.next_block(kind);
                let block = match kind {
                    OpenKind::Text => {
                        claude::ResponseContentBlock::Text(claude::ResponseTextBlock {
                            citations: None,
                            text: String::new(),
                            type_: claude::TextBlockType::Text,
                            rest: part.rest.clone(),
                        })
                    }
                    OpenKind::Thinking => {
                        claude::ResponseContentBlock::Thinking(claude::ThinkingBlock {
                            signature: None,
                            thinking: String::new(),
                            type_: claude::ThinkingBlockType::Thinking,
                            rest: part.rest.clone(),
                        })
                    }
                };
                output.push(events::encode(events::block_start(index, block))?);
                index
            }
        };
        let Some(gemini::PartData::Text { text, rest }) = part.data else {
            return Err(TransformError::shape(
                "Gemini stream",
                "text part changed during conversion",
            ));
        };
        if !text.is_empty() {
            let delta = match kind {
                OpenKind::Text => claude::KnownEventDelta::Text { text, rest },
                OpenKind::Thinking => claude::KnownEventDelta::Thinking {
                    estimated_tokens: None,
                    thinking: text,
                    rest,
                },
            };
            output.push(events::encode(events::block_delta(index, delta))?);
        }
        if let Some(signature) = part.thought_signature {
            output.push(events::encode(events::block_delta(
                index,
                claude::KnownEventDelta::Signature {
                    signature,
                    rest: Default::default(),
                },
            ))?);
        }
        Ok(output)
    }

    fn signature_only(&mut self, part: gemini::Part) -> Result<Vec<Bytes>, TransformError> {
        let open = self.open.as_ref().ok_or_else(|| {
            TransformError::shape("Gemini stream", "signature before thinking block")
        })?;
        if open.kind != OpenKind::Thinking {
            return Err(TransformError::shape(
                "Gemini stream",
                "signature on text block",
            ));
        }
        let signature = part
            .thought_signature
            .ok_or_else(|| TransformError::shape("Gemini stream", "signature is missing"))?;
        Ok(vec![events::encode(events::block_delta(
            open.index,
            claude::KnownEventDelta::Signature {
                signature,
                rest: part.rest,
            },
        ))?])
    }

    fn closed_part(&mut self, part: gemini::Part) -> Result<Vec<Bytes>, TransformError> {
        let mut output = self.close_open()?;
        let block = super::super::content::response_part(part, &mut self.correlation)?;
        self.has_tool |= matches!(
            &block,
            claude::ResponseContentBlock::ToolUse(_)
                | claude::ResponseContentBlock::ServerToolUse(_)
        );
        let index = self.next_index;
        self.next_index = self.next_index.saturating_add(1);
        output.push(events::encode(events::block_start(index, block))?);
        output.push(events::encode(events::block_stop(index))?);
        Ok(output)
    }
}

fn explicit_metadata(metadata: Option<&gemini::PartMetadata>) -> bool {
    match metadata {
        None => false,
        Some(gemini::PartMetadata::Raw(_)) => false,
        Some(_) => true,
    }
}
