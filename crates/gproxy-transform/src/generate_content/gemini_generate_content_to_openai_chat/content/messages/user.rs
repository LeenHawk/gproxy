use gproxy_protocol::{gemini, openai};

use crate::TransformError;

use super::super::parts::{text_content, user_part};
use super::State;
use super::model::tool_message;

impl State {
    pub(super) fn user(
        &mut self,
        mut content: gemini::Content,
    ) -> Result<Vec<openai::ChatCompletionMessageParam>, TransformError> {
        // Gemini CLI session replay can repeat adjacent results with the same explicit id.
        content.parts.dedup_by(|current, previous| {
            matches!(current.data.as_ref(), Some(gemini::PartData::FunctionResponse {
                function_response, ..
            }) if function_response.id.is_some())
                && current == previous
        });
        let mut output = Vec::new();
        let mut ordinary = Vec::new();
        for part in content.parts {
            match part.data.as_ref() {
                Some(gemini::PartData::FunctionResponse { .. })
                | Some(gemini::PartData::CodeExecutionResult { .. }) => {
                    flush_user(&mut output, &mut ordinary);
                    output.push(self.result(part)?);
                }
                _ => {
                    if let Some(part) = user_part(part)? {
                        ordinary.push(part);
                    }
                }
            }
        }
        flush_user(&mut output, &mut ordinary);
        Ok(output)
    }

    fn result(
        &mut self,
        part: gemini::Part,
    ) -> Result<openai::ChatCompletionMessageParam, TransformError> {
        match part.data {
            Some(gemini::PartData::FunctionResponse {
                function_response, ..
            }) => {
                let id = self
                    .take_function_id(&function_response.name, function_response.id.as_deref())?;
                Ok(tool_message(
                    id,
                    serde_json::to_string(&function_response.response)?,
                ))
            }
            Some(gemini::PartData::CodeExecutionResult {
                code_execution_result,
                ..
            }) => {
                let id = match code_execution_result.id.clone() {
                    Some(id) => {
                        let position = self
                            .pending_code
                            .iter()
                            .position(|pending| pending == &id)
                            .ok_or_else(|| {
                                TransformError::shape(
                                    "Gemini code execution result",
                                    "id has no preceding executableCode",
                                )
                            })?;
                        self.pending_code.remove(position);
                        id
                    }
                    None => self.pending_code.pop_front().ok_or_else(|| {
                        TransformError::shape(
                            "Gemini code execution result",
                            "no preceding executableCode",
                        )
                    })?,
                };
                Ok(tool_message(
                    id,
                    serde_json::to_string(&code_execution_result)?,
                ))
            }
            _ => Err(TransformError::shape(
                "Gemini result",
                "result part is missing",
            )),
        }
    }

    fn take_function_id(
        &mut self,
        name: &str,
        explicit: Option<&str>,
    ) -> Result<String, TransformError> {
        let queue = self.calls.get_mut(name).ok_or_else(|| {
            TransformError::shape(
                "Gemini function response",
                "name has no preceding functionCall",
            )
        })?;
        let id = match explicit {
            Some(id) => {
                let position = queue
                    .iter()
                    .position(|pending| pending == id)
                    .ok_or_else(|| {
                        TransformError::shape(
                            "Gemini function response",
                            "id has no same-name preceding functionCall",
                        )
                    })?;
                queue.remove(position).ok_or_else(|| {
                    TransformError::shape("Gemini function response", "pending call disappeared")
                })?
            }
            None => queue.pop_front().ok_or_else(|| {
                TransformError::shape(
                    "Gemini function response",
                    "same-name pending call queue is empty",
                )
            })?,
        };
        if queue.is_empty() {
            self.calls.remove(name);
        }
        Ok(id)
    }
}

fn flush_user(
    output: &mut Vec<openai::ChatCompletionMessageParam>,
    parts: &mut Vec<openai::ChatContentPart>,
) {
    if !parts.is_empty() {
        output.push(openai::ChatCompletionMessageParam::User(crate::wire!(
            openai::ChatUserMessageParam {
                role: openai::ChatUserRole::User,
                content: text_content(std::mem::take(parts)),
                name: None,
                rest: Default::default(),
            }
        )));
    }
}
