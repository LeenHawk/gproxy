mod controls;
mod history;
mod media;
mod messages;
mod shared_controls;
mod tools;
use crate::{
    transform::{Converted, Report, TransformError},
    wire::openai::{chat, responses},
};

/// Exact declared call kind recovered from a previous response, never inferred from an ID prefix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ToolCallKind {
    Function,
    Custom,
}
pub fn chat_to_responses_request(
    input: chat::GenerateContentRequestBody,
    target_model: impl Into<String>,
    flow: &mut crate::transform::identity::IdentityFlow,
    policy: &crate::transform::identity::TargetIdPolicy,
) -> Result<Converted<responses::GenerateContentRequestBody>, TransformError> {
    chat_to_responses_request_with_calls(input, target_model, &Default::default(), flow, policy)
}

pub fn chat_to_responses_request_with_calls(
    input: chat::GenerateContentRequestBody,
    target_model: impl Into<String>,
    prior_calls: &std::collections::BTreeMap<String, ToolCallKind>,
    flow: &mut crate::transform::identity::IdentityFlow,
    policy: &crate::transform::identity::TargetIdPolicy,
) -> Result<Converted<responses::GenerateContentRequestBody>, TransformError> {
    if input.stream.flatten() == Some(true) {
        super::stream_tools::check(&input)?;
    }
    if policy.dialect != crate::Dialect::OpenAi {
        return Err(TransformError::shape(
            "identity.policy",
            "expected Responses target policy",
        ));
    }
    let mut ids = flow.clone();
    let model = target_model.into();
    if model.is_empty() {
        return Err(TransformError::missing_metadata("target_model"));
    }
    let mut report = Report::default();
    let mut output = responses::GenerateContentRequestBody::builder().build();
    controls::to_responses(&input, &mut output, &mut report)?;
    output.model = Some(model.clone());
    if input.tools.is_some() && input.functions.is_some() {
        return Err(TransformError::shape(
            "tools",
            "legacy/current declarations conflict",
        ));
    }
    if input.tool_choice.is_some() && input.function_call.is_some() {
        return Err(TransformError::shape(
            "tool_choice",
            "legacy/current choices conflict",
        ));
    }
    output.tools = if let Some(functions) = input.functions {
        Some(tools::legacy(functions)?)
    } else {
        input.tools.map(tools::to_responses).transpose()?
    };
    output.tool_choice = if let Some(choice) = input.function_call {
        Some(tools::legacy_choice(choice))
    } else {
        input
            .tool_choice
            .map(tools::choice_to_responses)
            .transpose()?
    };
    output.input = Some(responses::input::Input::Items(messages::to_responses(
        input.messages,
        prior_calls,
        &mut report,
        &mut ids,
        policy,
    )?));
    output.metadata = input.metadata;
    output.prompt_cache_key = input.prompt_cache_key;
    output.safety_identifier = input.safety_identifier;
    output.store = input.store;
    output.stream = input.stream;
    output.user = input.user;
    crate::transform::instructions::responses(
        &mut output.input,
        &mut output.instructions,
        &model,
        &mut report,
    );
    *flow = ids;
    Ok(Converted {
        value: output,
        report,
    })
}

pub fn responses_to_chat_request(
    input: responses::GenerateContentRequestBody,
    target_model: impl Into<String>,
) -> Result<Converted<chat::GenerateContentRequestBody>, TransformError> {
    let model = target_model.into();
    if model.is_empty() {
        return Err(TransformError::missing_metadata("target_model"));
    }
    let mut report = Report::default();
    let mut output = chat::GenerateContentRequestBody::builder(Vec::new(), model).build();
    controls::to_chat(&input, &mut output, &mut report)?;
    output.tools = input.tools.map(tools::to_chat).transpose()?;
    output.tool_choice = input.tool_choice.map(tools::choice_to_chat).transpose()?;
    if let Some(Some(text)) = input.instructions {
        output.messages.push(chat::ChatMessage::System(
            chat::SystemMessage::builder(chat::SystemRole::System, chat::TextContent::Text(text))
                .build(),
        ));
    }
    output
        .messages
        .extend(history::to_chat(input.input, &mut report)?);
    output.metadata = input.metadata;
    output.prompt_cache_key = input.prompt_cache_key;
    output.safety_identifier = input.safety_identifier;
    output.store = input.store;
    output.stream = input.stream;
    output.user = input.user;
    crate::transform::instructions::chat(&mut output.messages, &output.model, &mut report);
    if output.stream.flatten() == Some(true) {
        super::stream_tools::check(&output)?;
    }
    Ok(Converted {
        value: output,
        report,
    })
}
