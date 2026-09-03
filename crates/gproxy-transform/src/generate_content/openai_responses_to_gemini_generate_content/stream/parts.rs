use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::{State, events};

impl State {
    pub(super) fn part(
        &mut self,
        candidate_index: i32,
        part: gemini::Part,
    ) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
        let thought = part.thought == Some(true);
        let signature = part.thought_signature.clone();
        match part.data.as_ref() {
            Some(gemini::PartData::Text { text, .. }) => {
                return self.text_delta(candidate_index, text.clone(), thought, signature);
            }
            None if signature.is_some() => {
                return self.text_delta(candidate_index, String::new(), true, signature);
            }
            Some(gemini::PartData::InlineData { inline_data, .. })
                if inline_data.mime_type.starts_with("audio/") =>
            {
                self.audio = true;
                let event = openai::KnownResponseStreamEvent::ResponseAudioDelta(crate::wire!(
                    openai::ResponseAudioDeltaEvent {
                        delta: inline_data.data.clone(),
                        sequence_number: Some(self.next_sequence()),
                        rest: Default::default(),
                    }
                ));
                return Ok(vec![events::emit(event)?]);
            }
            _ => {}
        }
        let content = crate::wire!(gemini::Content {
            parts: vec![part],
            role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)),
            rest: Default::default(),
        });
        let mut output = Vec::new();
        for item in self.content.response(content)? {
            output.extend(self.complete_item(item)?);
        }
        Ok(output)
    }

    fn complete_item(
        &mut self,
        item: openai::ResponseItem,
    ) -> Result<Vec<openai::ResponseStreamEvent>, TransformError> {
        let index = self.allocate();
        let added = openai::KnownResponseStreamEvent::ResponseOutputItemAdded(crate::wire!(
            openai::ResponseOutputItemEvent {
                item: Box::new(item.clone()),
                output_index: index,
                sequence_number: Some(self.next_sequence()),
                rest: Default::default(),
            }
        ));
        let done = openai::KnownResponseStreamEvent::ResponseOutputItemDone(crate::wire!(
            openai::ResponseOutputItemEvent {
                item: Box::new(item.clone()),
                output_index: index,
                sequence_number: Some(self.next_sequence()),
                rest: Default::default(),
            }
        ));
        self.items.push((index, item));
        Ok(vec![events::emit(added)?, events::emit(done)?])
    }
}
