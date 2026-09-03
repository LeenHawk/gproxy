use gproxy_protocol::{gemini, openai};

use crate::TransformError;
use crate::generate_content::gemini_generate_content_to_openai_chat::tools::CODE_EXECUTION_NAME;

use super::parts::text_part;

pub(crate) fn candidate(message: openai::ChatMessage) -> Result<gemini::Content, TransformError> {
    let mut parts = Vec::new();
    if let Some(reasoning) = message.reasoning_content.filter(|value| !value.is_empty()) {
        parts.push(text_part(reasoning, true));
    }
    if let Some(text) = message.content.filter(|value| !value.is_empty()) {
        parts.push(text_part(text, false));
    }
    if let Some(refusal) = message.refusal.filter(|value| !value.is_empty()) {
        parts.push(text_part(refusal, false));
    }
    if let Some(call) = message.function_call {
        parts.push(lossy_function_call(None, call.name, &call.arguments));
    }
    for call in message.tool_calls.into_iter().flatten() {
        parts.extend(tool_call(call)?);
    }
    Ok(crate::wire!(gemini::Content {
        parts,
        role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)),
        rest: Default::default(),
    }))
}

fn tool_call(call: openai::ChatToolCall) -> Result<Vec<gemini::Part>, TransformError> {
    let (id, name, arguments) = match call {
        openai::ChatToolCall::Function(call) => {
            (call.id, call.function.name, call.function.arguments)
        }
        openai::ChatToolCall::Custom(call) => (call.id, call.custom.name, call.custom.input),
        openai::ChatToolCall::Unknown(_) => return Ok(Vec::new()),
        #[cfg(not(feature = "exhaustive"))]
        _ => {
            return Err(crate::TransformError::unsupported(
                "protocol enum",
                "unrecognized external variant",
            ));
        }
    };
    if name != CODE_EXECUTION_NAME {
        return Ok(vec![lossy_function_call(Some(id), name, &arguments)]);
    }
    let mut code: gemini::ExecutableCode = serde_json::from_str(&arguments)?;
    code.id = Some(id.clone());
    let parts = vec![crate::wire!(gemini::Part {
        data: Some(gemini::PartData::ExecutableCode {
            executable_code: code,
            rest: Default::default(),
        }),
        rest: Default::default(),
        ..Default::default()
    })];
    Ok(parts)
}

pub(crate) fn lossy_function_call(
    id: Option<String>,
    name: String,
    arguments: &str,
) -> gemini::Part {
    crate::wire!(gemini::Part {
        data: Some(gemini::PartData::FunctionCall {
            function_call: gemini::FunctionCall {
                id,
                name,
                args: serde_json::from_str(arguments).ok(),
                rest: Default::default(),
            },
            rest: Default::default(),
        }),
        rest: Default::default(),
        ..Default::default()
    })
}
