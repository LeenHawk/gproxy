use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;

use super::super::State;
use super::preserve_option;
use super::typed_tools::CallContext;
use crate::generate_content::openai_chat_to_openai_responses::stream::wire::response_item_name;

impl State {
    pub(super) fn complete_typed_item(
        &mut self,
        item: openai::TypedResponseItem,
        output_index: u32,
        event_rest: openai::Rest,
    ) -> Result<Vec<Bytes>, TransformError> {
        match item {
            openai::TypedResponseItem::FunctionCall {
                arguments,
                call_id,
                name,
                id,
                caller,
                namespace,
                status,
                rest,
            } => self.complete_function_call(
                arguments,
                name,
                namespace,
                status,
                CallContext {
                    call_id,
                    id,
                    caller,
                    rest,
                    output_index,
                    event_rest,
                },
            ),
            openai::TypedResponseItem::CustomToolCall {
                call_id,
                input,
                name,
                id,
                caller,
                namespace,
                rest,
            } => self.complete_custom_call(
                input,
                name,
                namespace,
                CallContext {
                    call_id,
                    id,
                    caller,
                    rest,
                    output_index,
                    event_rest,
                },
            ),
            openai::TypedResponseItem::ShellCall {
                action,
                call_id,
                id,
                caller,
                environment,
                status,
                created_by,
                rest,
            } => self.complete_shell_call(
                action,
                environment,
                status,
                created_by,
                CallContext {
                    call_id,
                    id,
                    caller,
                    rest,
                    output_index,
                    event_rest,
                },
            ),
            openai::TypedResponseItem::ApplyPatchCall {
                call_id,
                operation,
                status,
                id,
                caller,
                created_by,
                rest,
            } => self.complete_patch_call(
                operation,
                status,
                created_by,
                CallContext {
                    call_id,
                    id,
                    caller,
                    rest,
                    output_index,
                    event_rest,
                },
            ),
            openai::TypedResponseItem::Reasoning {
                summary,
                content,
                encrypted_content,
                status,
                mut rest,
                ..
            } => {
                if encrypted_content.is_some() {
                    return Err(TransformError::unsupported(
                        "Responses stream",
                        "encrypted reasoning content",
                    ));
                }
                preserve_option(&mut rest, "status", status)?;
                let mut output = Vec::new();
                for part in summary {
                    output.extend(self.finish_reasoning(
                        part.text,
                        part.rest,
                        event_rest.clone(),
                    )?);
                }
                for part in content.into_iter().flatten() {
                    output.extend(self.finish_reasoning(
                        part.text,
                        part.rest,
                        event_rest.clone(),
                    )?);
                }
                if !rest.is_empty() {
                    output.push(self.preserve(rest, Default::default())?);
                } else if output.is_empty() && !event_rest.is_empty() {
                    output.push(self.preserve(Default::default(), event_rest)?);
                }
                Ok(output)
            }
            unsupported @ (openai::TypedResponseItem::FileSearchCall { .. }
            | openai::TypedResponseItem::ComputerCall { .. }
            | openai::TypedResponseItem::ComputerCallOutput { .. }
            | openai::TypedResponseItem::WebSearchCall { .. }
            | openai::TypedResponseItem::FunctionCallOutput { .. }
            | openai::TypedResponseItem::ToolSearchCall { .. }
            | openai::TypedResponseItem::ToolSearchOutput { .. }
            | openai::TypedResponseItem::AdditionalTools { .. }
            | openai::TypedResponseItem::Compaction { .. }
            | openai::TypedResponseItem::ImageGenerationCall { .. }
            | openai::TypedResponseItem::CodeInterpreterCall { .. }
            | openai::TypedResponseItem::LocalShellCall { .. }
            | openai::TypedResponseItem::LocalShellCallOutput { .. }
            | openai::TypedResponseItem::ShellCallOutput { .. }
            | openai::TypedResponseItem::ApplyPatchCallOutput { .. }
            | openai::TypedResponseItem::McpListTools { .. }
            | openai::TypedResponseItem::McpApprovalRequest { .. }
            | openai::TypedResponseItem::McpApprovalResponse { .. }
            | openai::TypedResponseItem::McpCall { .. }
            | openai::TypedResponseItem::CustomToolCallOutput { .. }
            | openai::TypedResponseItem::Program { .. }
            | openai::TypedResponseItem::ProgramOutput { .. }
            | openai::TypedResponseItem::MultiAgentCall { .. }
            | openai::TypedResponseItem::MultiAgentCallOutput { .. }
            | openai::TypedResponseItem::AgentMessage { .. }
            | openai::TypedResponseItem::CompactionTrigger { .. }
            | openai::TypedResponseItem::ItemReference { .. }) => Err(TransformError::unsupported(
                "Responses output item",
                response_item_name(&unsupported),
            )),
        }
    }
}
