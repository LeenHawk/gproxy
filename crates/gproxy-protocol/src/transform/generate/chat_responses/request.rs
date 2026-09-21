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
    let mut ids = flow.clone();
    let model = target_model.into();

    let mut report = Report::default();
    let mut output = responses::GenerateContentRequestBody::builder().build();
    controls::to_responses(&input, &mut output, &mut report)?;
    output.model = Some(model.clone());

    output.tools = if let Some(functions) = input.functions {
        Some(tools::legacy(functions)?)
    } else {
        input
            .tools
            .map(tools::to_responses)
            .map(crate::transform::optional)
            .transpose()?
            .flatten()
    };
    output.tool_choice = if let Some(choice) = input.function_call {
        Some(tools::legacy_choice(choice))
    } else {
        input
            .tool_choice
            .map(tools::choice_to_responses)
            .map(crate::transform::optional)
            .transpose()?
            .flatten()
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
    mut input: responses::GenerateContentRequestBody,
    target_model: impl Into<String>,
) -> Result<Converted<chat::GenerateContentRequestBody>, TransformError> {
    let model = target_model.into();

    let mut report = Report::default();
    let bindings = super::client_tools::Bindings::new(&input)?;
    bindings.lower(&mut input, &mut report)?;
    let mut output = chat::GenerateContentRequestBody::builder(Vec::new(), model).build();
    controls::to_chat(&input, &mut output, &mut report)?;
    output.tools = input
        .tools
        .map(tools::to_chat)
        .map(crate::transform::optional)
        .transpose()?
        .flatten();
    output.tool_choice = input
        .tool_choice
        .map(|v| crate::transform::optional(tools::choice_to_chat(v)))
        .transpose()?
        .flatten();
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

    Ok(Converted {
        value: output,
        report,
    })
}
