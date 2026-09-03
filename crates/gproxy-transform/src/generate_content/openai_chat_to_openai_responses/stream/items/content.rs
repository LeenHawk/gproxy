use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;

use super::super::State;
use super::suffix;
use crate::generate_content::openai_chat_to_openai_responses::stream::wire::empty_delta;

impl State {
    pub(in crate::generate_content::openai_chat_to_openai_responses::stream) fn complete_part(
        &mut self,
        part: openai::ResponseContentPart,
        _item_id: String,
        _output_index: u32,
        _content_index: u32,
    ) -> Result<Vec<Bytes>, TransformError> {
        match part {
            openai::ResponseContentPart::OutputText(part) => self.finish_text(part.text),
            openai::ResponseContentPart::Refusal(part) => self.finish_refusal(part.refusal),
            openai::ResponseContentPart::ReasoningText(part) => self.finish_reasoning(part.text),
            openai::ResponseContentPart::Unknown(_) => Ok(Vec::new()),
        }
    }

    pub(in crate::generate_content::openai_chat_to_openai_responses::stream) fn finish_text(
        &mut self,
        full: String,
    ) -> Result<Vec<Bytes>, TransformError> {
        let delta = suffix(&self.text, &full, "output text")?;
        self.text = full;
        self.content_chunk(delta, ContentKind::Text)
    }

    pub(in crate::generate_content::openai_chat_to_openai_responses::stream) fn finish_reasoning(
        &mut self,
        full: String,
    ) -> Result<Vec<Bytes>, TransformError> {
        let delta = suffix(&self.reasoning, &full, "reasoning text")?;
        self.reasoning = full;
        self.content_chunk(delta, ContentKind::Reasoning)
    }

    pub(in crate::generate_content::openai_chat_to_openai_responses::stream) fn finish_refusal(
        &mut self,
        full: String,
    ) -> Result<Vec<Bytes>, TransformError> {
        let delta = suffix(&self.refusal, &full, "refusal")?;
        self.refusal = full;
        self.content_chunk(delta, ContentKind::Refusal)
    }

    fn content_chunk(
        &self,
        delta: String,
        kind: ContentKind,
    ) -> Result<Vec<Bytes>, TransformError> {
        if delta.is_empty() {
            return Ok(Vec::new());
        }
        let mut value = empty_delta();
        match kind {
            ContentKind::Text => value.content = Some(delta),
            ContentKind::Reasoning => value.reasoning_content = Some(delta),
            ContentKind::Refusal => value.refusal = Some(delta),
        }
        Ok(vec![self.chunk(value, None, None)?])
    }
}

enum ContentKind {
    Text,
    Reasoning,
    Refusal,
}
