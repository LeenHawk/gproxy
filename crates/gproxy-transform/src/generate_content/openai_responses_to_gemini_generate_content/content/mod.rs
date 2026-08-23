use std::collections::BTreeMap;

use gproxy_protocol::{gemini, openai};

use crate::TransformError;

mod media;
mod messages;
mod native;
mod wire;

pub(in crate::generate_content) struct ContentConverter {
    function_names: BTreeMap<String, String>,
    native_ids: BTreeMap<String, String>,
}

impl ContentConverter {
    pub(in crate::generate_content) fn new() -> Self {
        Self {
            function_names: BTreeMap::new(),
            native_ids: BTreeMap::new(),
        }
    }

    pub(in crate::generate_content) fn input(
        &mut self,
        input: Option<openai::ResponseInput>,
    ) -> Result<Vec<gemini::Content>, TransformError> {
        match input {
            None => Ok(Vec::new()),
            Some(openai::ResponseInput::Text(text)) => Ok(vec![messages::text_content(
                gemini::ContentRoleKnown::User,
                text,
            )]),
            Some(openai::ResponseInput::Items(items)) => items
                .into_iter()
                .filter_map(|item| self.item(item).transpose())
                .collect(),
            Some(openai::ResponseInput::Unknown(raw)) => Err(TransformError::unsupported(
                "Responses input",
                raw.to_string(),
            )),
        }
    }

    pub(in crate::generate_content) fn item(
        &mut self,
        item: openai::ResponseItem,
    ) -> Result<Option<gemini::Content>, TransformError> {
        Ok(match item {
            openai::ResponseItem::Message(message) => Some(messages::message(message)?),
            openai::ResponseItem::Typed(item) => self.typed(*item)?,
            openai::ResponseItem::Unknown(raw) => Some(gemini::Content {
                parts: vec![gemini::Part {
                    data: Some(gemini::PartData::Raw(raw)),
                    ..Default::default()
                }],
                role: None,
                rest: Default::default(),
            }),
        })
    }

    fn typed(
        &mut self,
        item: openai::TypedResponseItem,
    ) -> Result<Option<gemini::Content>, TransformError> {
        match item {
            openai::TypedResponseItem::FunctionCall {
                call_id,
                name,
                arguments,
                mut rest,
                ..
            }
            | openai::TypedResponseItem::CustomToolCall {
                call_id,
                name,
                input: arguments,
                mut rest,
                ..
            } => {
                self.function_names.insert(call_id.clone(), name.clone());
                Ok(Some(native::function_call(
                    call_id, name, arguments, &mut rest,
                )?))
            }
            openai::TypedResponseItem::FunctionCallOutput {
                call_id,
                name,
                output,
                rest,
                ..
            } => {
                let name = name
                    .or_else(|| self.function_names.get(&call_id).cloned())
                    .ok_or_else(|| {
                        TransformError::shape(
                            "Responses function result",
                            "name missing and no matching call was seen",
                        )
                    })?;
                Ok(Some(native::function_result(call_id, name, output, rest)?))
            }
            openai::TypedResponseItem::CustomToolCallOutput {
                call_id,
                output,
                rest,
                ..
            } => {
                let name = self.function_names.get(&call_id).cloned().ok_or_else(|| {
                    TransformError::shape(
                        "Responses function result",
                        "name missing and no matching call was seen",
                    )
                })?;
                Ok(Some(native::function_result(call_id, name, output, rest)?))
            }
            openai::TypedResponseItem::Reasoning {
                id,
                summary,
                content,
                encrypted_content,
                rest,
                ..
            } => Ok(Some(native::reasoning(
                id,
                summary,
                content,
                encrypted_content,
                rest,
            ))),
            other => native::native_item(self, other).map(Some),
        }
    }
}
