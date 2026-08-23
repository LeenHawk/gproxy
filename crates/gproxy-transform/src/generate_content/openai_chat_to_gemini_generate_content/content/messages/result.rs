use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::super::parts::text_content;
use super::State;

impl State {
    pub(super) fn tool_result(
        &mut self,
        message: openai::ChatToolMessageParam,
    ) -> Result<(), TransformError> {
        let output = text_content(message.content)?;
        let call = self.calls.get(&message.tool_call_id).ok_or_else(|| {
            TransformError::shape("Chat tool result", "tool_call_id has no preceding call")
        })?;
        let part = if call.code_execution {
            let mut result: gemini::CodeExecutionResult = serde_json::from_str(&output)?;
            result.id = Some(message.tool_call_id);
            gemini::Part {
                data: Some(gemini::PartData::CodeExecutionResult {
                    code_execution_result: result,
                    rest: Default::default(),
                }),
                rest: message.rest,
                ..Default::default()
            }
        } else {
            function_response(
                Some(message.tool_call_id),
                call.name.clone(),
                output,
                message.rest,
            )?
        };
        self.push(
            gemini::ContentRoleKnown::Function,
            vec![part],
            Default::default(),
        );
        Ok(())
    }

    pub(super) fn function_result(
        &mut self,
        message: openai::ChatFunctionMessageParam,
    ) -> Result<(), TransformError> {
        let part = function_response(None, message.name, message.content, message.rest)?;
        self.push(
            gemini::ContentRoleKnown::Function,
            vec![part],
            Default::default(),
        );
        Ok(())
    }
}

fn function_response(
    id: Option<String>,
    name: String,
    output: String,
    mut rest: gemini::ExtraFields,
) -> Result<gemini::Part, TransformError> {
    let parts = take(&mut rest, "gemini_function_response_parts")?;
    let will_continue = take(&mut rest, "gemini_function_response_will_continue")?;
    let scheduling = take(&mut rest, "gemini_function_response_scheduling")?;
    let response_rest = match take(&mut rest, "gemini_function_response_rest")? {
        Some(rest) => rest,
        None => gemini::ExtraFields::new(),
    };
    let response = match serde_json::from_str::<serde_json::Value>(&output) {
        Ok(serde_json::Value::Object(response)) => response,
        Ok(value) => {
            let mut response = gemini::JsonMap::new();
            response.insert("output".into(), value);
            response
        }
        Err(_) => {
            let mut response = gemini::JsonMap::new();
            response.insert("output".into(), output.into());
            response
        }
    };
    Ok(gemini::Part {
        data: Some(gemini::PartData::FunctionResponse {
            function_response: gemini::FunctionResponse {
                id,
                name,
                response,
                parts,
                will_continue,
                scheduling,
                rest: response_rest,
            },
            rest: Default::default(),
        }),
        rest,
        ..Default::default()
    })
}

fn take<T: serde::de::DeserializeOwned>(
    rest: &mut gemini::ExtraFields,
    key: &str,
) -> Result<Option<T>, TransformError> {
    rest.remove(key)
        .map(serde_json::from_value)
        .transpose()
        .map_err(Into::into)
}
