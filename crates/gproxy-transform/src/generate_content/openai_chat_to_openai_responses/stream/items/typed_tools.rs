use bytes::Bytes;
use gproxy_protocol::openai;

use crate::TransformError;

use super::super::{State, ToolKind, ToolStart};
use super::preserve_option;
use crate::generate_content::openai_chat_to_openai_responses::stream::native::source_id;

pub(super) struct CallContext {
    pub(super) call_id: String,
    pub(super) id: Option<String>,
    pub(super) caller: Option<openai::ResponseCaller>,
    pub(super) rest: openai::Rest,
    pub(super) output_index: u32,
    pub(super) event_rest: openai::Rest,
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
            caller,
            mut rest,
            output_index,
            event_rest,
        } = context;
        let source_id = source_id(id.as_deref(), output_index);
        preserve_option(&mut rest, "responses_item_id", id)?;
        preserve_option(&mut rest, "caller", caller)?;
        preserve_option(&mut rest, "namespace", namespace)?;
        preserve_option(&mut rest, "status", status)?;
        let mut output = self.start_tool(ToolStart {
            source_id: source_id.clone(),
            call_id,
            output_index,
            name,
            kind: ToolKind::Function,
            rest,
            event_rest,
        })?;
        output.extend(self.finish_tool(
            &source_id,
            output_index,
            ToolKind::Function,
            arguments,
            Default::default(),
        )?);
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
            caller,
            mut rest,
            output_index,
            event_rest,
        } = context;
        let source_id = source_id(id.as_deref(), output_index);
        preserve_option(&mut rest, "responses_item_id", id)?;
        preserve_option(&mut rest, "caller", caller)?;
        preserve_option(&mut rest, "namespace", namespace)?;
        let mut output = self.start_tool(ToolStart {
            source_id: source_id.clone(),
            call_id,
            output_index,
            name,
            kind: ToolKind::Custom,
            rest,
            event_rest,
        })?;
        output.extend(self.finish_tool(
            &source_id,
            output_index,
            ToolKind::Custom,
            input,
            Default::default(),
        )?);
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
            caller,
            mut rest,
            output_index,
            event_rest,
        } = context;
        let source_id = source_id(id.as_deref(), output_index);
        preserve_option(&mut rest, "responses_item_id", id)?;
        preserve_option(&mut rest, "caller", caller)?;
        preserve_option(&mut rest, "environment", environment)?;
        preserve_option(&mut rest, "status", status)?;
        preserve_option(&mut rest, "created_by", created_by)?;
        self.complete_function_item(
            ToolStart {
                source_id,
                call_id,
                output_index,
                name: "shell".into(),
                kind: ToolKind::Function,
                rest,
                event_rest,
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
            caller,
            mut rest,
            output_index,
            event_rest,
        } = context;
        let source_id = source_id(id.as_deref(), output_index);
        preserve_option(&mut rest, "responses_item_id", id)?;
        preserve_option(&mut rest, "caller", caller)?;
        preserve_option(&mut rest, "status", Some(status))?;
        preserve_option(&mut rest, "created_by", created_by)?;
        self.complete_function_item(
            ToolStart {
                source_id,
                call_id,
                output_index,
                name: "apply_patch".into(),
                kind: ToolKind::Function,
                rest,
                event_rest,
            },
            serde_json::to_string(&operation)?,
        )
    }
}
