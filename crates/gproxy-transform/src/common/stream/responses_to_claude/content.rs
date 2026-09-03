use gproxy_protocol::{claude, openai};

use crate::TransformError;

use super::super::openai_to_claude::{Scalar, State};

impl State {
    pub(super) fn response_content_part_added(
        &mut self,
        event: openai::ResponseContentPartEvent,
    ) -> Result<Vec<claude::StreamEvent>, TransformError> {
        let mut output = self.ensure_start()?;
        match event.part {
            openai::ResponseContentPart::OutputText(part) => {
                let index = self.allocate();
                let text = part.text;
                output.extend(self.block_start(
                    index,
                    claude::ResponseContentBlock::Text(crate::wire!(claude::ResponseTextBlock {
                        citations: None,
                        text: String::new(),
                        type_: claude::TextBlockType::Text,
                        rest: Default::default(),
                    })),
                )?);
                self.response_indices
                    .insert((event.item_id, Some(event.content_index)), index);
                if !text.is_empty() {
                    output.push(self.delta(
                        index,
                        claude::KnownEventDelta::Text {
                            text,
                            rest: Default::default(),
                        },
                    )?);
                }
            }
            openai::ResponseContentPart::Refusal(part) => {
                let index = self.allocate();
                let refusal = part.refusal;
                output.extend(self.block_start(
                    index,
                    claude::ResponseContentBlock::Text(crate::wire!(claude::ResponseTextBlock {
                        citations: None,
                        text: String::new(),
                        type_: claude::TextBlockType::Text,
                        rest: Default::default(),
                    })),
                )?);
                self.response_indices
                    .insert((event.item_id, Some(event.content_index)), index);
                if !refusal.is_empty() {
                    output.push(self.delta(
                        index,
                        claude::KnownEventDelta::Text {
                            text: refusal,
                            rest: Default::default(),
                        },
                    )?);
                }
            }
            openai::ResponseContentPart::ReasoningText(part) => {
                let index = self.response_index(Some(&event.item_id), None)?;
                self.response_indices
                    .insert((event.item_id, Some(event.content_index)), index);
                if !part.text.is_empty() {
                    output.push(self.delta(
                        index,
                        claude::KnownEventDelta::Thinking {
                            estimated_tokens: None,
                            thinking: part.text,
                            rest: Default::default(),
                        },
                    )?);
                }
            }
            openai::ResponseContentPart::Unknown(raw) => {
                return Err(TransformError::unsupported(
                    "Responses content part",
                    raw.to_string(),
                ));
            }
            #[cfg(not(feature = "exhaustive"))]
            _ => {
                return Err(crate::TransformError::unsupported(
                    "protocol enum",
                    "unrecognized external variant",
                ));
            }
        }
        Ok(output)
    }

    pub(super) fn response_output_text_delta(
        &mut self,
        event: openai::ResponseOutputTextDeltaEvent,
    ) -> Result<Vec<claude::StreamEvent>, TransformError> {
        let mut output = self.ensure_start()?;
        output.extend(self.response_scalar(
            event.item_id,
            event.content_index,
            event.delta,
            Scalar::Text,
        )?);
        Ok(output)
    }

    pub(super) fn response_content_delta(
        &mut self,
        event: openai::ResponseContentDeltaEvent,
        kind: Scalar,
    ) -> Result<Vec<claude::StreamEvent>, TransformError> {
        let mut output = self.ensure_start()?;
        output.extend(self.response_scalar(
            event.item_id,
            Some(event.content_index),
            event.delta,
            kind,
        )?);
        Ok(output)
    }

    pub(super) fn response_summary_delta(
        &mut self,
        event: openai::ResponseReasoningSummaryTextDeltaEvent,
    ) -> Result<Vec<claude::StreamEvent>, TransformError> {
        let mut output = self.ensure_start()?;
        output.extend(self.response_scalar(event.item_id, None, event.delta, Scalar::Thinking)?);
        Ok(output)
    }

    pub(super) fn response_tool_delta(
        &mut self,
        event: openai::ResponseItemStringDeltaEvent,
    ) -> Result<Vec<claude::StreamEvent>, TransformError> {
        let mut output = self.ensure_start()?;
        let index = self.response_index(Some(&event.item_id), None)?;
        self.response_tool_inputs
            .get_mut(&index)
            .ok_or_else(|| TransformError::shape("Responses stream", "tool input state missing"))?
            .push_str(&event.delta);
        output.push(self.input_delta(index, event.delta)?);
        Ok(output)
    }

    pub(super) fn response_content_done(
        &mut self,
        item_id: &str,
        content_index: Option<u32>,
    ) -> Result<Vec<claude::StreamEvent>, TransformError> {
        let output = self.ensure_start()?;
        self.response_index(Some(item_id), content_index)?;
        Ok(output)
    }

    pub(super) fn response_tool_done(
        &mut self,
        item_id: Option<&str>,
        output_index: u32,
        full: String,
    ) -> Result<Vec<claude::StreamEvent>, TransformError> {
        let mut output = self.ensure_start()?;
        let index = self.response_index_for_output(item_id, output_index, None)?;
        output.extend(self.response_tool_full(index, full)?);
        Ok(output)
    }

    pub(super) fn response_tool_full(
        &mut self,
        index: u64,
        full: String,
    ) -> Result<Vec<claude::StreamEvent>, TransformError> {
        let current = self
            .response_tool_inputs
            .get_mut(&index)
            .ok_or_else(|| TransformError::shape("Responses stream", "tool input state missing"))?;
        let delta = full
            .strip_prefix(current.as_str())
            .ok_or_else(|| {
                TransformError::shape(
                    "Responses stream",
                    "tool input done value does not extend prior deltas",
                )
            })?
            .to_owned();
        *current = full;
        if delta.is_empty() {
            Ok(Vec::new())
        } else {
            Ok(vec![self.input_delta(index, delta)?])
        }
    }
}
