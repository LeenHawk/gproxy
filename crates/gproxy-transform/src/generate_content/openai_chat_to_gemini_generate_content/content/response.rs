use gproxy_protocol::{gemini, openai};

use crate::TransformError;
use crate::generate_content::gemini_generate_content_to_openai_chat::tools::CODE_EXECUTION_NAME;

use super::parts::text_part;

pub(crate) fn candidate(message: openai::ChatMessage) -> Result<gemini::Content, TransformError> {
    let mut parts = Vec::new();
    if let Some(reasoning) = message.reasoning_content.filter(|value| !value.is_empty()) {
        parts.push(text_part(reasoning, true, Default::default()));
    }
    if let Some(text) = message.content.filter(|value| !value.is_empty()) {
        parts.push(text_part(text, false, Default::default()));
    }
    if let Some(refusal) = message.refusal.filter(|value| !value.is_empty()) {
        parts.push(text_part(refusal, false, Default::default()));
    }
    if let Some(call) = message.function_call {
        parts.push(lossy_function_call(
            None,
            call.name,
            &call.arguments,
            call.rest,
        ));
    }
    for call in message.tool_calls.into_iter().flatten() {
        parts.extend(tool_call(call)?);
    }
    let mut rest = message.rest;
    if let Some(annotations) = message.annotations {
        rest.insert(
            "openai_annotations".into(),
            serde_json::to_value(annotations)?,
        );
    }
    Ok(gemini::Content {
        parts,
        role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)),
        rest,
    })
}

fn tool_call(call: openai::ChatToolCall) -> Result<Vec<gemini::Part>, TransformError> {
    let (id, name, arguments, mut rest) = match call {
        openai::ChatToolCall::Function(call) => (
            call.id,
            call.function.name,
            call.function.arguments,
            merge(call.rest, call.function.rest),
        ),
        openai::ChatToolCall::Custom(call) => (
            call.id,
            call.custom.name,
            call.custom.input,
            merge(call.rest, call.custom.rest),
        ),
        openai::ChatToolCall::Unknown(raw) => {
            return Err(TransformError::unsupported(
                "Chat tool call",
                raw.to_string(),
            ));
        }
    };
    if name != CODE_EXECUTION_NAME {
        return Ok(vec![lossy_function_call(Some(id), name, &arguments, rest)]);
    }
    let mut code: gemini::ExecutableCode = serde_json::from_str(&arguments)?;
    code.id = Some(id.clone());
    let result = rest.remove("gemini_code_execution_result");
    let mut parts = vec![gemini::Part {
        data: Some(gemini::PartData::ExecutableCode {
            executable_code: code,
            rest: Default::default(),
        }),
        rest: rest.clone(),
        ..Default::default()
    }];
    if let Some(value) = result {
        let mut result: gemini::CodeExecutionResult = serde_json::from_value(value)?;
        result.id = Some(id);
        parts.push(gemini::Part {
            data: Some(gemini::PartData::CodeExecutionResult {
                code_execution_result: result,
                rest: Default::default(),
            }),
            rest: Default::default(),
            ..Default::default()
        });
    }
    Ok(parts)
}

pub(crate) fn lossy_function_call(
    id: Option<String>,
    name: String,
    arguments: &str,
    rest: gemini::ExtraFields,
) -> gemini::Part {
    gemini::Part {
        data: Some(gemini::PartData::FunctionCall {
            function_call: gemini::FunctionCall {
                id,
                name,
                args: serde_json::from_str(arguments).ok(),
                rest: Default::default(),
            },
            rest: Default::default(),
        }),
        rest,
        ..Default::default()
    }
}

fn merge(mut left: openai::Rest, right: openai::Rest) -> openai::Rest {
    left.extend(right);
    left
}
