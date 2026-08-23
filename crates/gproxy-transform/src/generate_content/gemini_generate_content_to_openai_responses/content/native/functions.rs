use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::super::{ContentConverter, wire};
use super::correlated;
use crate::generate_content::gemini_generate_content_to_openai_responses::ids;

impl ContentConverter {
    pub(in crate::generate_content) fn function_call(
        &mut self,
        call: gemini::FunctionCall,
        signature: Option<String>,
        mut rest: openai::Rest,
    ) -> Result<openai::ResponseItem, TransformError> {
        let call_id = self.allocate_call(call.id);
        self.calls_by_name
            .entry(call.name.clone())
            .or_default()
            .push_back(call_id.clone());
        rest.extend(call.rest);
        if let Some(signature) = signature {
            rest.insert("thought_signature".into(), signature.into());
        }
        Ok(openai::ResponseItem::Typed(Box::new(
            openai::TypedResponseItem::FunctionCall {
                arguments: wire::arguments(call.args)?,
                call_id: call_id.clone(),
                name: call.name,
                id: Some(ids::item_id("fc", &call_id)),
                caller: None,
                namespace: None,
                status: Some(openai::ResponseItemLifecycleStatus::Completed),
                rest,
            },
        )))
    }

    pub(in crate::generate_content) fn function_response(
        &mut self,
        result: gemini::FunctionResponse,
        mut rest: openai::Rest,
    ) -> Result<openai::ResponseItem, TransformError> {
        if result.will_continue.is_some() || result.scheduling.is_some() {
            return Err(TransformError::unsupported(
                "Gemini functionResponse",
                "willContinue or scheduling",
            ));
        }
        let pending = self.calls_by_name.get_mut(&result.name);
        let call_id = correlated(result.id, pending).ok_or_else(|| {
            TransformError::shape(
                "Gemini functionResponse",
                "id missing and no matching functionCall was seen",
            )
        })?;
        rest.extend(result.rest);
        Ok(openai::ResponseItem::Typed(Box::new(
            openai::TypedResponseItem::FunctionCallOutput {
                call_id,
                output: wire::function_output(result.response, result.parts)?,
                id: None,
                caller: None,
                name: Some(result.name),
                namespace: None,
                status: Some(openai::ResponseItemLifecycleStatus::Completed),
                created_by: None,
                rest,
            },
        )))
    }

    pub(in crate::generate_content) fn tool_call(
        &mut self,
        call: gemini::ToolCall,
        mut rest: openai::Rest,
    ) -> Result<openai::ResponseItem, TransformError> {
        let call_id = self.allocate_call(call.id);
        let name = wire::server_tool_name(&call.tool_type)?;
        self.calls_by_name
            .entry(name.clone())
            .or_default()
            .push_back(call_id.clone());
        rest.extend(call.rest);
        Ok(openai::ResponseItem::Typed(Box::new(
            openai::TypedResponseItem::FunctionCall {
                arguments: wire::arguments(call.args)?,
                call_id: call_id.clone(),
                name,
                id: Some(ids::item_id("fc", &call_id)),
                caller: None,
                namespace: None,
                status: Some(openai::ResponseItemLifecycleStatus::Completed),
                rest,
            },
        )))
    }

    pub(in crate::generate_content) fn tool_response(
        &mut self,
        result: gemini::ToolResponse,
        mut rest: openai::Rest,
    ) -> Result<openai::ResponseItem, TransformError> {
        let name = wire::server_tool_name(&result.tool_type)?;
        let pending = self.calls_by_name.get_mut(&name);
        let call_id = correlated(result.id, pending).ok_or_else(|| {
            TransformError::shape(
                "Gemini toolResponse",
                "id missing and no matching toolCall was seen",
            )
        })?;
        rest.extend(result.rest);
        let output = result
            .response
            .map(wire::output)
            .transpose()?
            .ok_or_else(|| TransformError::shape("Gemini toolResponse", "response is missing"))?;
        Ok(openai::ResponseItem::Typed(Box::new(
            openai::TypedResponseItem::FunctionCallOutput {
                call_id,
                output,
                id: None,
                caller: None,
                name: Some(name),
                namespace: None,
                status: Some(openai::ResponseItemLifecycleStatus::Completed),
                created_by: None,
                rest,
            },
        )))
    }
}
