use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;

use super::super::{State, ToolKind, ToolStart};
use crate::generate_content::openai_chat_to_openai_responses::stream::native::source_id;

pub(super) struct CallContext {
    pub(super) call_id: String,
    pub(super) id: Option<String>,
    pub(super) output_index: u32,
}

impl State {
    pub(super) fn complete_function_call(
        &mut self,
        arguments: String,
        name: String,
        namespace: Option<String>,
        status: Option<openai::ResponseItemLifecycleStatus>,
        context: CallContext,
    ) -> Result<Vec<Bytes>, TransformError> {
        let CallContext {
            call_id,
            id,
            output_index,
        } = context;
        let source_id = source_id(id.as_deref(), output_index);
        let _ = (namespace, status);
        let mut output = self.start_tool(ToolStart {
            source_id: source_id.clone(),
            call_id,
            output_index,
            name,
            kind: ToolKind::Function,
        })?;
        output.extend(self.finish_tool(&source_id, output_index, ToolKind::Function, arguments)?);
        Ok(output)
    }

    pub(super) fn complete_custom_call(
        &mut self,
        input: String,
        name: String,
        namespace: Option<String>,
        context: CallContext,
    ) -> Result<Vec<Bytes>, TransformError> {
        let CallContext {
            call_id,
            id,
            output_index,
        } = context;
        let source_id = source_id(id.as_deref(), output_index);
        let _ = namespace;
        let mut output = self.start_tool(ToolStart {
            source_id: source_id.clone(),
            call_id,
            output_index,
            name,
            kind: ToolKind::Custom,
        })?;
        output.extend(self.finish_tool(&source_id, output_index, ToolKind::Custom, input)?);
        Ok(output)
    }

    pub(super) fn complete_shell_call(
        &mut self,
        action: openai::ShellAction,
        environment: Option<openai::ShellEnvironment>,
        status: Option<openai::ResponseItemLifecycleStatus>,
        created_by: Option<String>,
        context: CallContext,
    ) -> Result<Vec<Bytes>, TransformError> {
        let CallContext {
            call_id,
            id,
            output_index,
        } = context;
        let source_id = source_id(id.as_deref(), output_index);
        let _ = (environment, status, created_by);
        self.complete_function_item(
            ToolStart {
                source_id,
                call_id,
                output_index,
                name: "shell".into(),
                kind: ToolKind::Function,
            },
            serde_json::to_string(&action)?,
        )
    }

    pub(super) fn complete_patch_call(
        &mut self,
        operation: openai::ApplyPatchOperation,
        status: openai::ResponseApplyPatchCallStatus,
        created_by: Option<String>,
        context: CallContext,
    ) -> Result<Vec<Bytes>, TransformError> {
        let CallContext {
            call_id,
            id,
            output_index,
        } = context;
        let source_id = source_id(id.as_deref(), output_index);
        let _ = (status, created_by);
        self.complete_function_item(
            ToolStart {
                source_id,
                call_id,
                output_index,
                name: "apply_patch".into(),
                kind: ToolKind::Function,
            },
            serde_json::to_string(&operation)?,
        )
    }
}
