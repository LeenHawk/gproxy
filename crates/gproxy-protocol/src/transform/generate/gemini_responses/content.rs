use crate::{
    transform::{
        Report, TransformError,
        identity::{IdentityFlow, IdentityRole, OutputItemKind, TargetIdPolicy},
    },
    wire::{gemini as g, openai::responses::input as r},
};
pub(crate) fn validate(part: &g::Part) -> Result<(), TransformError> {
    if part.executable_code.is_some()
        || part.code_execution_result.is_some()
        || part.tool_call.is_some()
        || part.tool_response.is_some()
        || part.video_metadata.is_some()
        || part.part_metadata.is_some()
        || part.media_resolution.is_some()
    {
        return Err(TransformError::unsupported(
            "part",
            "native execution/annotated media requires adapter",
        ));
    }
    Ok(())
}
pub(crate) fn to_responses(
    input: Vec<g::Content>,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
    report: &mut Report,
) -> Result<Vec<r::InputItem>, TransformError> {
    let mut out = Vec::new();
    let mut names: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    let mut calls = std::collections::BTreeMap::new();
    let mut index = 0u64;
    for content in input {
        let role = match content.role.as_deref() {
            None | Some("user") => r::MessageRole::User,
            Some("model") => r::MessageRole::Assistant,
            Some("system") => r::MessageRole::System,
            Some(_) => return Err(TransformError::shape("role", "unknown Gemini role")),
        };
        for part in content.parts.unwrap_or_default() {
            validate(&part)?;
            if part.thought == Some(true) {
                let id = super::identity::id(
                    flow,
                    policy,
                    IdentityRole::Message,
                    IdentityRole::OutputItem(OutputItemKind::Reasoning),
                    None,
                    index,
                )?;
                index += 1;
                let mut item =
                    r::ReasoningItem::builder(r::ReasoningItemType::ReasoningItem, id, Vec::new())
                        .build();
                item.content = part.text.map(|text| {
                    vec![
                        r::ReasoningContent::builder(r::ReasoningTextType::ReasoningText, text)
                            .build(),
                    ]
                });
                out.push(r::InputItem::Reasoning(item));
                report.omitted(
                    "thought_signature",
                    "Gemini signature must remain scoped in host replay state",
                );
                continue;
            }
            if part.thought_signature.is_some() {
                report.omitted(
                    "thought_signature",
                    "Gemini signature must remain scoped in host replay state",
                );
            }
            if let Some(text) = part.text {
                out.push(r::InputItem::Easy(
                    r::EasyInputMessage::builder(r::MessageContent::Text(text), role).build(),
                ));
            }
            for media in [
                part.inline_data.map(super::media::inline),
                part.file_data.map(super::media::file),
            ]
            .into_iter()
            .flatten()
            {
                if role != r::MessageRole::User {
                    return Err(TransformError::unsupported(
                        "media.role",
                        "Responses history media requires user role",
                    ));
                }
                out.push(r::InputItem::Easy(
                    r::EasyInputMessage::builder(r::MessageContent::Parts(vec![media?]), role)
                        .build(),
                ));
            }
            if let Some(call) = part.function_call {
                if role != r::MessageRole::Assistant {
                    return Err(TransformError::shape(
                        "function_call.role",
                        "expected model role",
                    ));
                }
                if call.id.as_ref().is_some_and(|id| calls.contains_key(id)) {
                    return Err(TransformError::shape(
                        "function_call.id",
                        "duplicate source call",
                    ));
                }
                let id = super::identity::id(
                    flow,
                    policy,
                    IdentityRole::ToolCall,
                    IdentityRole::ToolCall,
                    call.id.clone(),
                    index,
                )?;
                let item_id = super::identity::id(
                    flow,
                    policy,
                    IdentityRole::ToolCall,
                    IdentityRole::OutputItem(OutputItemKind::FunctionCall),
                    call.id.clone(),
                    index,
                )?;
                index += 1;
                if let Some(source) = call.id {
                    calls.insert(source, id.clone());
                }
                names.entry(call.name.clone()).or_default().push(id.clone());
                out.push(r::InputItem::FunctionCall(
                    r::FunctionCall::builder(
                        r::FunctionCallType::FunctionCall,
                        serde_json::to_string(&call.args.unwrap_or_default())?,
                        id,
                        call.name,
                    )
                    .id(item_id)
                    .build(),
                ));
            }
            if let Some(result) = part.function_response {
                if result.scheduling.is_some() || result.will_continue == Some(true) {
                    return Err(TransformError::unsupported(
                        "function_response",
                        "multimedia/async result needs output adapter",
                    ));
                }
                let id = if let Some(id) = result.id.as_ref() {
                    calls.get(id).cloned().ok_or_else(|| {
                        TransformError::missing_metadata("function_response call id")
                    })?
                } else {
                    let ids = names.get_mut(&result.name).ok_or_else(|| {
                        TransformError::missing_metadata("function_response call name")
                    })?;
                    if ids.len() != 1 {
                        return Err(TransformError::missing_metadata(
                            "ambiguous same-name function response",
                        ));
                    }
                    ids.remove(0)
                };
                let output = super::media::output(result)?;
                out.push(r::InputItem::FunctionCallOutput(
                    r::FunctionCallOutput::builder(
                        r::FunctionCallOutputType::FunctionCallOutput,
                        id,
                        output,
                    )
                    .build(),
                ));
            }
        }
    }
    Ok(out)
}
