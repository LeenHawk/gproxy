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
                let mut event =
                    events::event(openai::ResponseStreamEventTypeKnown::ResponseAudioDelta);
                event.sequence_number = Some(self.next_sequence());
                event.delta = Some(inline_data.data.clone());
                event
                    .rest
                    .insert("mime_type".into(), inline_data.mime_type.clone().into());
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
        let id = events::item_id(&item, index);
        let mut added =
            events::event(openai::ResponseStreamEventTypeKnown::ResponseOutputItemAdded);
        added.sequence_number = Some(self.next_sequence());
        added.item = Some(Box::new(item.clone()));
        added.output_index = Some(index);
        let mut done = events::event(openai::ResponseStreamEventTypeKnown::ResponseOutputItemDone);
        done.sequence_number = Some(self.next_sequence());
        done.item = Some(Box::new(item.clone()));
        done.item_id = Some(id);
        done.output_index = Some(index);
        self.items.push((index, item));
        Ok(vec![events::emit(added)?, events::emit(done)?])
    }
}
