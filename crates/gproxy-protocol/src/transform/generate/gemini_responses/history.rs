use crate::{
    transform::{Report, TransformError},
    wire::{gemini as g, openai::responses::input as r},
};

pub(crate) fn to_gemini(
    input: Option<r::Input>,
    _model: &str,
    context: &mut super::identity::GeminiReplayContext,
    report: &mut Report,
) -> Result<(Vec<g::Content>, Vec<g::Part>), TransformError> {
    let input = match input {
        None => Vec::new(),
        Some(r::Input::Text(text)) => vec![r::InputItem::Easy(
            r::EasyInputMessage::builder(r::MessageContent::Text(text), r::MessageRole::User)
                .build(),
        )],
        Some(r::Input::Items(items)) => items,
    };
    let mut names = std::collections::BTreeMap::new();
    for item in &input {
        if let r::InputItem::FunctionCall(call) = item
            && names
                .insert(call.call_id.clone(), call.name.clone())
                .is_some()
        {
            return Err(TransformError::shape("function_call.id", "duplicate ID"));
        }
    }
    let mut contents = Vec::new();
    let mut system = Vec::new();
    let mut position = crate::transform::instructions::Position::default();
    for (index, item) in input.into_iter().enumerate() {
        let (role, parts) = match item {
            r::InputItem::Easy(message) => (message.role, parts(message.content)?),
            r::InputItem::Message(message) => (
                match message.role {
                    r::InputMessageRole::User => r::MessageRole::User,
                    r::InputMessageRole::System => r::MessageRole::System,
                    r::InputMessageRole::Developer => r::MessageRole::Developer,
                },
                message
                    .content
                    .into_iter()
                    .map(super::media::to_gemini)
                    .filter_map(|value| crate::transform::optional(value).transpose())
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            r::InputItem::OutputMessage(message) => {
                let mut parts = Vec::new();
                for part in message.content {
                    match part {
                        r::OutputContent::Text(part) => {
                            if !part.annotations.is_empty() || !part.logprobs.is_empty() {
                                report.omitted(
                                    "history.metadata",
                                    "Gemini request history has no annotation/logprob fields",
                                );
                            }
                            parts.push(g::Part::builder().text(part.text).build());
                        }
                        r::OutputContent::Refusal(part) => {
                            report.changed("refusal", "Gemini history represents refusal text");
                            parts.push(g::Part::builder().text(part.refusal).build());
                        }
                    }
                }
                (r::MessageRole::Assistant, parts)
            }
            r::InputItem::FunctionCall(call) => (
                r::MessageRole::Assistant,
                vec![super::identity::function(call, context)?],
            ),
            r::InputItem::FunctionCallOutput(output) => {
                let name = names
                    .get(&output.call_id)
                    .cloned()
                    .or_else(|| output.name.flatten())
                    .ok_or_else(|| TransformError::missing_metadata("function_response.name"))?;
                if output.namespace.flatten().is_some()
                    || output
                        .caller
                        .flatten()
                        .is_some_and(|v| matches!(v, r::Caller::Program(_)))
                {
                    continue;
                }
                (
                    r::MessageRole::User,
                    vec![super::media::output_to_gemini(
                        output.output,
                        name,
                        output.call_id,
                    )?],
                )
            }
            r::InputItem::ImageGenerationCall(image) => {
                let max = image.result.as_ref().map_or(0, |v| v.len() as u64);
                (
                    r::MessageRole::Assistant,
                    vec![super::images::restore(image, context, max)?],
                )
            }
            r::InputItem::Reasoning(reasoning) => (
                r::MessageRole::Assistant,
                vec![super::identity::reasoning(reasoning, context)?],
            ),
            r::InputItem::ItemReference(_) => {
                return Err(TransformError::missing_metadata(
                    "item reference resolved history",
                ));
            }
            r::InputItem::Compaction(_)
            | r::InputItem::ComputerCall(_)
            | r::InputItem::ComputerCallOutput(_)
            | r::InputItem::WebSearchCall(_)
            | r::InputItem::FileSearchCall(_)
            | r::InputItem::CodeInterpreterCall(_)
            | r::InputItem::CustomToolCall(_)
            | r::InputItem::CustomToolCallOutput(_)
            | r::InputItem::ToolSearchCall(_)
            | r::InputItem::ToolSearchOutput(_)
            | r::InputItem::AdditionalTools(_)
            | r::InputItem::LocalShellCall(_)
            | r::InputItem::LocalShellCallOutput(_)
            | r::InputItem::ShellCall(_)
            | r::InputItem::ShellCallOutput(_)
            | r::InputItem::ApplyPatchCall(_)
            | r::InputItem::ApplyPatchCallOutput(_)
            | r::InputItem::McpListTools(_)
            | r::InputItem::McpApprovalRequest(_)
            | r::InputItem::McpApprovalResponse(_)
            | r::InputItem::McpCall(_)
            | r::InputItem::CompactionTrigger(_)
            | r::InputItem::Program(_)
            | r::InputItem::ProgramOutput(_) => {
                continue;
            }
        };
        let leading = position.leading(matches!(
            role,
            r::MessageRole::System | r::MessageRole::Developer
        ));
        match role {
            r::MessageRole::System | r::MessageRole::Developer => {
                crate::transform::instructions::gemini(
                    parts,
                    leading,
                    &mut system,
                    &mut contents,
                    format!("input[{index}].role"),
                    report,
                )?;
            }
            r::MessageRole::User => {
                contents.push(g::Content::builder().role("user").parts(parts).build())
            }
            r::MessageRole::Assistant => {
                contents.push(g::Content::builder().role("model").parts(parts).build())
            }
        }
    }
    Ok((contents, system))
}

fn parts(value: r::MessageContent) -> Result<Vec<g::Part>, TransformError> {
    match value {
        r::MessageContent::Text(text) => Ok(vec![g::Part::builder().text(text).build()]),
        r::MessageContent::Parts(parts) => parts.into_iter().map(super::media::to_gemini).collect(),
    }
}
