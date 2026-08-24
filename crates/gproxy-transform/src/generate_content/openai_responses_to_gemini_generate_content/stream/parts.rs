use bytes::Bytes;
use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::{State, events};

impl State {
    pub(super) fn part(&mut self, part: gemini::Part) -> Result<Vec<Bytes>, TransformError> {
        let thought = part.thought == Some(true);
        let signature = part.thought_signature.clone();
        match part.data.as_ref() {
            Some(gemini::PartData::Text { text, .. }) => {
                return self.text_delta(text.clone(), thought, signature, part.rest);
            }
            None if signature.is_some() => {
                return self.text_delta(String::new(), true, signature, part.rest);
            }
            Some(gemini::PartData::InlineData { inline_data, .. })
                if inline_data.mime_type.starts_with("audio/") =>
            {
                self.audio = true;
                let mut rest = part.rest;
                rest.insert("mime_type".into(), inline_data.mime_type.clone().into());
                let event = openai::KnownResponseStreamEvent::ResponseAudioDelta(
                    openai::ResponseAudioDeltaEvent {
                        delta: inline_data.data.clone(),
                        sequence_number: Some(self.next_sequence()),
                        rest,
                    },
                );
                return Ok(vec![events::emit(event)?]);
            }
            _ => {}
        }
        let content = gemini::Content {
            parts: vec![part],
            role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)),
            rest: Default::default(),
        };
        let mut output = Vec::new();
        for item in self.content.response(content)? {
            output.extend(self.complete_item(item)?);
        }
        Ok(output)
    }

    fn complete_item(&mut self, item: openai::ResponseItem) -> Result<Vec<Bytes>, TransformError> {
        let index = self.allocate();
        let added = openai::KnownResponseStreamEvent::ResponseOutputItemAdded(
            openai::ResponseOutputItemEvent {
                item: Box::new(item.clone()),
                output_index: index,
                sequence_number: Some(self.next_sequence()),
                rest: Default::default(),
            },
        );
        let done = openai::KnownResponseStreamEvent::ResponseOutputItemDone(
            openai::ResponseOutputItemEvent {
                item: Box::new(item.clone()),
                output_index: index,
                sequence_number: Some(self.next_sequence()),
                rest: Default::default(),
            },
        );
        self.items.push((index, item));
        Ok(vec![events::emit(added)?, events::emit(done)?])
    }
}
