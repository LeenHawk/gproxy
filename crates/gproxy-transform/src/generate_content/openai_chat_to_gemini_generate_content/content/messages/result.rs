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
            crate::wire!(gemini::Part {
                data: Some(gemini::PartData::CodeExecutionResult {
                    code_execution_result: result,
                    rest: Default::default(),
                }),
                rest: Default::default(),
                ..Default::default()
            })
        } else {
            function_response(Some(message.tool_call_id), call.name.clone(), Some(output))?
        };
        self.push(gemini::ContentRoleKnown::Function, vec![part]);
        Ok(())
    }

    pub(super) fn function_result(
        &mut self,
        message: openai::ChatFunctionMessageParam,
    ) -> Result<(), TransformError> {
        let part = function_response(None, message.name, message.content)?;
        self.push(gemini::ContentRoleKnown::Function, vec![part]);
        Ok(())
    }
}

fn function_response(
    id: Option<String>,
    name: String,
    output: Option<String>,
) -> Result<gemini::Part, TransformError> {
    let response = match output {
        None => {
            let mut response = gemini::JsonMap::new();
            response.insert("output".into(), serde_json::Value::Null);
            response
        }
        Some(output) => match serde_json::from_str::<serde_json::Value>(&output) {
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
        },
    };
    Ok(crate::wire!(gemini::Part {
        data: Some(gemini::PartData::FunctionResponse {
            function_response: gemini::FunctionResponse {
                id,
                name,
                response,
                parts: None,
                will_continue: None,
                scheduling: None,
                rest: Default::default(),
            },
            rest: Default::default(),
        }),
        rest: Default::default(),
        ..Default::default()
    }))
}
