use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;

use super::super::State;

impl State {
    pub(in crate::generate_content::openai_chat_to_openai_responses::stream) fn add_item(
        &mut self,
        item: openai::ResponseItem,
        output_index: u32,
    ) -> Result<Vec<Bytes>, TransformError> {
        match item {
            openai::ResponseItem::Message(openai::ResponseMessageItem::Output(_)) => {
                self.started = true;
                Ok(vec![self.chunk(
                    openai::ChatDelta {
                        role: Some(openai::ChatDeltaRole::Assistant),
                        ..crate::generate_content::openai_chat_to_openai_responses::stream::wire::empty_delta()
                    },
                    None,
                    None,
                )?])
            }
            openai::ResponseItem::Typed(item) => self.complete_typed_item(*item, output_index),
            openai::ResponseItem::Message(openai::ResponseMessageItem::Input(_))
            | openai::ResponseItem::Message(openai::ResponseMessageItem::EasyInput(_))
            | openai::ResponseItem::Message(openai::ResponseMessageItem::Unknown(_))
            | openai::ResponseItem::Unknown(_) => Ok(Vec::new()),
        }
    }

    pub(in crate::generate_content::openai_chat_to_openai_responses::stream) fn done_item(
        &mut self,
        item: openai::ResponseItem,
        output_index: u32,
    ) -> Result<Vec<Bytes>, TransformError> {
        match item {
            openai::ResponseItem::Typed(item) => self.complete_typed_item(*item, output_index),
            openai::ResponseItem::Message(_) | openai::ResponseItem::Unknown(_) => Ok(Vec::new()),
        }
    }
}
