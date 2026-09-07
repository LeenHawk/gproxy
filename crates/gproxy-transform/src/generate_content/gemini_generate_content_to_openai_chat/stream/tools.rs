use std::collections::BTreeMap;

use gproxy_protocol::{gemini, openai};

use crate::TransformError;
use crate::generate_content::gemini_generate_content_to_openai_chat::tools::CODE_EXECUTION_NAME;
use crate::generate_content::openai_chat_to_gemini_generate_content::content;

pub(super) struct Pending {
    id: Option<String>,
    name: Option<String>,
    arguments: String,
}

pub(super) fn update(
    tools: &mut BTreeMap<(u32, u32), Pending>,
    choice: u32,
    call: openai::ChatToolCallDelta,
) -> Result<(), TransformError> {
    let pending = tools
        .entry((choice, call.index))
        .or_insert_with(|| Pending {
            id: None,
            name: None,
            arguments: String::new(),
        });
    if let Some(id) = call.id {
        set_once(&mut pending.id, id, "tool id")?;
    }
    let (name, arguments) = match (call.function, call.custom) {
        (Some(function), None) => (function.name, function.arguments),
        (None, Some(custom)) => (custom.name, custom.input),
        (Some(_), Some(_)) => {
            return Err(TransformError::shape(
                "Chat stream",
                "tool delta has function and custom payloads",
            ));
        }
        (None, None) => (None, None),
    };
    if let Some(name) = name {
        set_once(&mut pending.name, name, "tool name")?;
    }
    if let Some(arguments) = arguments {
        pending.arguments.push_str(&arguments);
    }
    Ok(())
}

pub(super) fn update_legacy(
    tools: &mut BTreeMap<(u32, u32), Pending>,
    choice: u32,
    call: openai::FunctionCallDelta,
) -> Result<(), TransformError> {
    update(
        tools,
        choice,
        crate::wire!(openai::ChatToolCallDelta {
            index: u32::MAX,
            id: Some(format!("function_call_{choice}")),
            type_: Some(openai::ChatToolCallType::Function),
            function: Some(call),
            custom: None,
            rest: Default::default(),
        }),
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
        let id = pending
            .id
            .ok_or_else(|| TransformError::shape("Chat stream", "tool id is missing"))?;
        let name = pending
            .name
            .ok_or_else(|| TransformError::shape("Chat stream", "tool name is missing"))?;
        if name == CODE_EXECUTION_NAME {
            let mut code: gemini::ExecutableCode = serde_json::from_str(&pending.arguments)?;
            code.id = Some(id.clone());
            output.push(crate::wire!(gemini::Part {
                data: Some(gemini::PartData::ExecutableCode {
                    executable_code: code,
                    rest: Default::default(),
                }),
                rest: Default::default(),
                ..Default::default()
            }));
        } else {
            output.push(content::lossy_function_call(
                Some(id),
                name,
                &pending.arguments,
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
