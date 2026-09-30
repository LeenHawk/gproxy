use crate::{
    transform::{Report, TransformError, generate::signature::Carried},
    wire::{gemini as g, openai::responses::input as r},
};

pub(crate) fn to_gemini(
    input: Option<r::Input>,
    _model: &str,
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
    // Unsigned thought text right before a signed thought: a thought run the
    // upstream split, whose signature needs the whole text; see `signature`.
    let mut thought_text = String::new();
    // A `gemini-next:` carrier's signature, for the item right after it.
    let mut next_signature: Option<String> = None;
    for (index, item) in input.into_iter().enumerate() {
        let mut thought = None;
        if let r::InputItem::Reasoning(reasoning) = &item {
            let encrypted = reasoning.encrypted_content.clone().flatten();
            match encrypted.as_deref().map(Carried::parse) {
                Some(Some(Carried::GeminiThought(native))) if !native.is_empty() => {
                    thought_text.push_str(&super::identity::reasoning_text(reasoning));
                    thought = Some(
                        g::Part::builder()
                            .thought(true)
                            .text(std::mem::take(&mut thought_text))
                            .thought_signature(native.to_owned())
                            .build(),
                    );
                    next_signature = None;
                }
                Some(Some(Carried::GeminiNext(native))) if !native.is_empty() => {
                    drop_thought(&mut thought_text, report);
                    next_signature = Some(native.to_owned());
                }
                None | Some(Some(Carried::GeminiThought(_))) => {
                    thought_text.push_str(&super::identity::reasoning_text(reasoning));
                }
                Some(_) => {
                    drop_thought(&mut thought_text, report);
                    report.omitted(
                        "reasoning.encrypted_content",
                        "reasoning signed by another upstream cannot become a Gemini signature",
                    );
                }
            }
            if thought.is_none() {
                continue;
            }
        } else {
            drop_thought(&mut thought_text, report);
        }
        let signature = next_signature.take();
        if signature.is_some()
            && !matches!(
                item,
                r::InputItem::FunctionCall(_)
                    | r::InputItem::ImageGenerationCall(_)
                    | r::InputItem::Reasoning(_)
            )
        {
            report.omitted(
                "reasoning.encrypted_content",
                "Gemini part signature is not followed by its part",
            );
        }
        let (role, mut parts) = match item {
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
                vec![super::identity::function(call)?],
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
                    vec![super::images::to_gemini(&image, max)?],
                )
            }
            r::InputItem::Reasoning(_) => {
                (r::MessageRole::Assistant, thought.into_iter().collect())
            }
            r::InputItem::ItemReference(_) => {
                return Err(TransformError::missing_metadata(
                    "item reference resolved history",
                ));
            }
            r::InputItem::MultiAgentCall(_)
            | r::InputItem::MultiAgentCallOutput(_)
            | r::InputItem::AgentMessage(_)
            | r::InputItem::ConfigurationUpdate(_)
            | r::InputItem::Compaction(_)
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
        if let (Some(signature), [part]) = (signature, parts.as_mut_slice())
            && (part.function_call.is_some() || part.inline_data.is_some())
        {
            part.thought_signature = Some(signature);
        }
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

/// Unsigned thought text not followed by its signature: Gemini reads nothing
/// from it, so it is left out.
fn drop_thought(text: &mut String, report: &mut Report) {
    if !std::mem::take(text).is_empty() {
        report.omitted(
            "reasoning",
            "unsigned reasoning has nothing the Gemini upstream can verify",
        );
    }
}

fn parts(value: r::MessageContent) -> Result<Vec<g::Part>, TransformError> {
    match value {
        r::MessageContent::Text(text) => Ok(vec![g::Part::builder().text(text).build()]),
        r::MessageContent::Parts(parts) => parts.into_iter().map(super::media::to_gemini).collect(),
    }
}
