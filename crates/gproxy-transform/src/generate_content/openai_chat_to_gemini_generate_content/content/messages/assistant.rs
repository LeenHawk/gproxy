use gproxy_protocol::{gemini, openai};

use crate::TransformError;
use crate::generate_content::gemini_generate_content_to_openai_chat::tools::CODE_EXECUTION_NAME;

use super::super::parts::{assistant_parts, function_call, text_part};
use super::{Call, State};

impl State {
    pub(super) fn assistant(
        &mut self,
        message: openai::ChatAssistantMessageParam,
        turn: usize,
    ) -> Result<(), TransformError> {
        let mut parts = message
            .content
            .map(assistant_parts)
            .transpose()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        if let Some(reasoning) = message.reasoning_content.filter(|value| !value.is_empty()) {
            parts.insert(0, text_part(reasoning, true));
        }
        if let Some(refusal) = message.refusal.filter(|value| !value.is_empty()) {
            parts.push(text_part(refusal, false));
        }
        if let Some(call) = message.function_call {
            let id = format!("function_call_{turn}");
            self.calls.insert(
                id.clone(),
                Call {
                    name: call.name.clone(),
                    code_execution: false,
                },
            );
            parts.push(function_call(Some(id), call.name, &call.arguments)?);
        }
        for call in message.tool_calls.into_iter().flatten() {
            parts.extend(self.tool_call(call)?);
        }
        self.push(gemini::ContentRoleKnown::Model, parts);
        Ok(())
    }

    fn tool_call(
        &mut self,
        call: openai::ChatToolCall,
    ) -> Result<Vec<gemini::Part>, TransformError> {
        let (id, name, arguments) = match call {
            openai::ChatToolCall::Function(call) => {
                (call.id, call.function.name, call.function.arguments)
            }
            openai::ChatToolCall::Custom(call) => (call.id, call.custom.name, call.custom.input),
            openai::ChatToolCall::Unknown(_) => return Ok(Vec::new()),
        };
        let code_execution = name == CODE_EXECUTION_NAME;
        self.calls.insert(
            id.clone(),
            Call {
                name: name.clone(),
                code_execution,
            },
        );
        if code_execution {
            let mut code: gemini::ExecutableCode = serde_json::from_str(&arguments)?;
            code.id = Some(id);
            return Ok(vec![gemini::Part {
                data: Some(gemini::PartData::ExecutableCode {
                    executable_code: code,
                    rest: Default::default(),
                }),
                rest: Default::default(),
                ..Default::default()
            }]);
        }
        Ok(vec![function_call(Some(id), name, &arguments)?])
    }
}
