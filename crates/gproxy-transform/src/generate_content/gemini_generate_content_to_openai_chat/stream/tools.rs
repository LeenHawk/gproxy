use std::collections::BTreeMap;

use gproxy_protocol::{gemini, openai};

use crate::TransformError;
use crate::generate_content::gemini_generate_content_to_openai_chat::tools::CODE_EXECUTION_NAME;
use crate::generate_content::openai_chat_to_gemini_generate_content::content;

pub(super) struct Pending {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
    rest: openai::Rest,
    emitted: bool,
}

pub(super) fn update(
    tools: &mut BTreeMap<(u32, u32), Pending>,
    choice: u32,
    call: openai::ChatToolCallDelta,
) -> Result<Vec<gemini::Part>, TransformError> {
    let pending = tools
        .entry((choice, call.index))
        .or_insert_with(|| Pending {
            id: None,
            name: None,
            arguments: String::new(),
            rest: Default::default(),
            emitted: false,
        });
    if let Some(id) = call.id {
        set_once(&mut pending.id, id, "tool id")?;
    }
    let (name, arguments, variant_rest) = match (call.function, call.custom) {
        (Some(function), None) => (function.name, function.arguments, function.rest),
        (None, Some(custom)) => (custom.name, custom.input, custom.rest),
        (Some(_), Some(_)) => {
            return Err(TransformError::shape(
                "Chat stream",
                "tool delta has function and custom payloads",
            ));
        }
        (None, None) => (None, None, Default::default()),
    };
    if let Some(name) = name {
        set_once(&mut pending.name, name, "tool name")?;
    }
    if let Some(arguments) = arguments {
        pending.arguments.push_str(&arguments);
    }
    pending.rest.extend(call.rest);
    pending.rest.extend(variant_rest);
    if !pending.emitted
        && pending.name.as_deref() != Some(CODE_EXECUTION_NAME)
        && let Some(name) = pending.name.clone()
    {
        pending.emitted = true;
        return Ok(vec![content::lossy_function_call(
            pending.id.clone(),
            name,
            &pending.arguments,
            pending.rest.clone(),
        )]);
    }
    Ok(Vec::new())
}

pub(super) fn update_legacy(
    tools: &mut BTreeMap<(u32, u32), Pending>,
    choice: u32,
    call: openai::FunctionCallDelta,
) -> Result<Vec<gemini::Part>, TransformError> {
    update(
        tools,
        choice,
        openai::ChatToolCallDelta {
            index: u32::MAX,
            id: Some(format!("function_call_{choice}")),
            type_: Some(openai::ChatToolCallType::Function),
            function: Some(call),
            custom: None,
            rest: Default::default(),
        },
    )
}

pub(super) fn finish_choice(
    tools: &mut BTreeMap<(u32, u32), Pending>,
    choice: u32,
) -> Result<Vec<gemini::Part>, TransformError> {
    let keys = tools
        .keys()
        .filter(|(candidate, _)| *candidate == choice)
        .copied()
        .collect::<Vec<_>>();
    let mut output = Vec::new();
    for key in keys {
        let pending = tools
            .remove(&key)
            .ok_or_else(|| TransformError::shape("Chat stream", "pending tool disappeared"))?;
        if pending.emitted {
            continue;
        }
        let id = pending
            .id
            .ok_or_else(|| TransformError::shape("Chat stream", "tool id is missing"))?;
        let name = pending
            .name
            .ok_or_else(|| TransformError::shape("Chat stream", "tool name is missing"))?;
        if name == CODE_EXECUTION_NAME {
            let mut rest = pending.rest;
            let result = rest.remove("gemini_code_execution_result");
            let mut code: gemini::ExecutableCode = serde_json::from_str(&pending.arguments)?;
            code.id = Some(id.clone());
            output.push(gemini::Part {
                data: Some(gemini::PartData::ExecutableCode {
                    executable_code: code,
                    rest: Default::default(),
                }),
                rest,
                ..Default::default()
            });
            if let Some(result) = result {
                let mut result: gemini::CodeExecutionResult = serde_json::from_value(result)?;
                result.id = Some(id);
                output.push(gemini::Part {
                    data: Some(gemini::PartData::CodeExecutionResult {
                        code_execution_result: result,
                        rest: Default::default(),
                    }),
                    ..Default::default()
                });
            }
        } else {
            output.push(content::lossy_function_call(
                Some(id),
                name,
                &pending.arguments,
                pending.rest,
            ));
        }
    }
    Ok(output)
}

fn set_once(
    slot: &mut Option<String>,
    value: String,
    field: &'static str,
) -> Result<(), TransformError> {
    if slot.as_ref().is_some_and(|current| current != &value) {
        return Err(TransformError::shape(
            "Chat stream",
            format!("{field} changed mid-stream"),
        ));
    }
    *slot = Some(value);
    Ok(())
}
