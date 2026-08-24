use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;

use super::super::State;

impl State {
    pub(in crate::generate_content::openai_chat_to_openai_responses::stream) fn complete_item(
        &mut self,
        item: openai::ResponseItem,
        output_index: u32,
        event_rest: openai::Rest,
    ) -> Result<Vec<Bytes>, TransformError> {
        match item {
            openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => {
                let mut output = Vec::new();
                let mut rest = message.rest;
                rest.insert("status".into(), serde_json::to_value(message.status)?);
                if let Some(phase) = message.phase {
                    rest.insert("phase".into(), serde_json::to_value(phase)?);
                }
                for part in message.content {
                    output.extend(self.complete_message_part(part, event_rest.clone())?);
                }
                if !rest.is_empty() {
                    output.push(self.preserve(rest, Default::default())?);
                } else if output.is_empty() && !event_rest.is_empty() {
                    output.push(self.preserve(Default::default(), event_rest)?);
                }
                Ok(output)
            }
            openai::ResponseItem::Typed(item) => {
                self.complete_typed_item(*item, output_index, event_rest)
            }
            openai::ResponseItem::Message(openai::ResponseMessageItem::Input(_)) => Err(
                TransformError::unsupported("Responses output item", "input message"),
            ),
            openai::ResponseItem::Message(openai::ResponseMessageItem::EasyInput(_)) => Err(
                TransformError::unsupported("Responses output item", "easy input message"),
            ),
            openai::ResponseItem::Message(openai::ResponseMessageItem::Unknown(value)) => Err(
                TransformError::unsupported("Responses output item", value.to_string()),
            ),
            openai::ResponseItem::Unknown(value) => Err(TransformError::unsupported(
                "Responses output item",
                value.to_string(),
            )),
        }
    }
}
