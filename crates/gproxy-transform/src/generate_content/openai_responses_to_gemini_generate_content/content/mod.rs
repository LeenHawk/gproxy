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
    pending_signature: Option<String>,
}

impl ContentConverter {
    pub(in crate::generate_content) fn new() -> Self {
        Self {
            function_names: BTreeMap::new(),
            native_ids: BTreeMap::new(),
            pending_signature: None,
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
            openai::ResponseItem::Message(message) => messages::message(message)?,
            openai::ResponseItem::Typed(item) => self.typed(*item)?,
            openai::ResponseItem::Unknown(_) => None,
            #[cfg(not(feature = "exhaustive"))]
            _ => {
                return Err(crate::TransformError::unsupported(
                    "protocol enum",
                    "unrecognized external variant",
                ));
            }
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
                ..
            }
            | openai::TypedResponseItem::CustomToolCall {
                call_id,
                name,
                input: arguments,
                ..
            } => {
                let signature = self.pending_signature.take();
                self.function_names.insert(call_id.clone(), name.clone());
                Ok(Some(native::function_call(
                    call_id, name, arguments, signature,
                )?))
            }
            openai::TypedResponseItem::FunctionCallOutput {
                call_id,
                name,
                output,
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
                Ok(Some(native::function_result(call_id, name, output)?))
            }
            openai::TypedResponseItem::CustomToolCallOutput {
                call_id, output, ..
            } => {
                let name = self.function_names.get(&call_id).cloned().ok_or_else(|| {
                    TransformError::shape(
                        "Responses function result",
                        "name missing and no matching call was seen",
                    )
                })?;
                Ok(Some(native::function_result(call_id, name, output)?))
            }
            openai::TypedResponseItem::Reasoning {
                id: _,
                summary,
                content,
                encrypted_content,
                ..
            } => {
                let empty = summary.is_empty()
                    && content
                        .as_ref()
                        .is_none_or(|parts| parts.iter().all(|part| part.text.is_empty()));
                if empty && encrypted_content.is_some() {
                    self.pending_signature = encrypted_content;
                    Ok(None)
                } else {
                    Ok(Some(native::reasoning(summary, content, encrypted_content)))
                }
            }
            other @ (openai::TypedResponseItem::FileSearchCall { .. }
            | openai::TypedResponseItem::ComputerCall { .. }
            | openai::TypedResponseItem::ComputerCallOutput { .. }
            | openai::TypedResponseItem::WebSearchCall { .. }
            | openai::TypedResponseItem::ToolSearchCall { .. }
            | openai::TypedResponseItem::ToolSearchOutput { .. }
            | openai::TypedResponseItem::AdditionalTools { .. }
            | openai::TypedResponseItem::Compaction { .. }
            | openai::TypedResponseItem::ImageGenerationCall { .. }
            | openai::TypedResponseItem::CodeInterpreterCall { .. }
            | openai::TypedResponseItem::LocalShellCall { .. }
            | openai::TypedResponseItem::LocalShellCallOutput { .. }
            | openai::TypedResponseItem::ShellCall { .. }
            | openai::TypedResponseItem::ShellCallOutput { .. }
            | openai::TypedResponseItem::ApplyPatchCall { .. }
            | openai::TypedResponseItem::ApplyPatchCallOutput { .. }
            | openai::TypedResponseItem::McpListTools { .. }
            | openai::TypedResponseItem::McpApprovalRequest { .. }
            | openai::TypedResponseItem::McpApprovalResponse { .. }
            | openai::TypedResponseItem::McpCall { .. }
            | openai::TypedResponseItem::Program { .. }
            | openai::TypedResponseItem::ProgramOutput { .. }
            | openai::TypedResponseItem::MultiAgentCall { .. }
            | openai::TypedResponseItem::MultiAgentCallOutput { .. }
            | openai::TypedResponseItem::AgentMessage { .. }
            | openai::TypedResponseItem::ConfigurationUpdate { .. }
            | openai::TypedResponseItem::CompactionTrigger { .. }
            | openai::TypedResponseItem::ItemReference { .. }) => native::native_item(self, other),
            #[cfg(not(feature = "exhaustive"))]
            _ => {
                return Err(crate::TransformError::unsupported(
                    "protocol enum",
                    "unrecognized external variant",
                ));
            }
        }
    }
}
