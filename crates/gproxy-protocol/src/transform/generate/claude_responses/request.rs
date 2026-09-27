mod controls;
pub(crate) mod history;
pub(crate) mod mcp;
mod media;
pub(crate) mod messages;
mod schema;
pub(crate) mod tools;
use crate::{
    transform::{
        Converted, Report, TransformError,
        generate::signature::Carried,
        identity::{IdentityFlow, TargetIdPolicy},
    },
    wire::{
        DeclaredFields,
        claude::{content as cc, generate_content as c},
        openai::responses as r,
    },
};

/// The Claude upstream a Responses request is prepared for, when the caller
/// knows it; a mismatched model or a missing origin is refused.
#[derive(Default)]
pub struct ClaudeRequestContext {
    pub target: Option<crate::transform::identity::IdentityTarget>,
}

pub fn claude_to_responses_request(
    input: c::GenerateContentRequestBody,
    target_model: impl Into<String>,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
) -> Result<Converted<r::GenerateContentRequestBody>, TransformError> {
    let model = target_model.into();

    let mut input = input.into_declared();
    let mut ids = flow.clone();
    let mut report = Report::default();
    super::super::claude_controls::project(&mut input, &mut report);
    let mut out = r::GenerateContentRequestBody::builder().build();
    controls::to_responses(&input, &mut out, &mut report)?;
    out.model = Some(model.clone());
    out.tools = input
        .tools
        .map(|v| tools::to_responses(v, &mut report))
        .map(crate::transform::optional)
        .transpose()?
        .flatten();
    if let Some(choice) = input.tool_choice {
        let (choice, parallel) = tools::choice_to_responses(choice);
        out.tool_choice = Some(choice);
        out.parallel_tool_calls = parallel.map(Some);
    }
    if let Some(servers) = input.mcp_servers {
        for server in servers {
            if let Some(tool) = mcp::to_responses(server)? {
                out.tools.get_or_insert_with(Vec::new).push(tool);
            }
        }
    }
    out.instructions = input.system.map(|v| {
        Some(match v {
            crate::claude::count_tokens::SystemPrompt::Text(text) => text,
            crate::claude::count_tokens::SystemPrompt::Blocks(blocks) => blocks
                .into_iter()
                .map(|v| v.text)
                .collect::<Vec<_>>()
                .join("\n"),
        })
    });
    out.input = Some(r::Input::Items(messages::to_responses(
        input.messages,
        &mut ids,
        policy,
        &mut report,
    )?));
    crate::transform::instructions::responses(
        &mut out.input,
        &mut out.instructions,
        &model,
        &mut report,
    );
    *flow = ids;
    crate::transform::generate::openai_controls::target_responses(&mut out, &mut report);
    Ok(Converted { value: out, report })
}

pub fn responses_to_claude_request(
    input: r::GenerateContentRequestBody,
    target_model: impl Into<String>,
    context: ClaudeRequestContext,
) -> Result<Converted<c::GenerateContentRequestBody>, TransformError> {
    let model = target_model.into();

    if let Some(target) = &context.target
        && (target.model != model
            || target.dialect != crate::Dialect::Claude
            || target.origin.as_ref().is_none_or(|v| v.trim().is_empty()))
    {
        return Err(TransformError::shape(
            "reasoning.target",
            "native Claude replay requires matching model and explicit upstream origin",
        ));
    }
    let mut input = input.into_declared();
    let max = input
        .max_output_tokens
        .flatten()
        .ok_or_else(|| TransformError::missing_metadata("max_output_tokens"))?;
    let mut out = c::GenerateContentRequestBody::builder(max, Vec::new(), model).build();
    let mut report = Report::default();
    crate::transform::generate::openai_controls::project_request(&mut input, &mut report)?;
    super::super::client_tools::Bindings::for_target(&input, crate::Dialect::Claude)?
        .lower(&mut input, &mut report)?;
    controls::to_claude(&input, &mut out, &mut report)?;
    if let Some(input_tools) = input.tools {
        let mut native = Vec::new();
        let mut servers = Vec::new();
        for tool in input_tools {
            match tool {
                r::tools::Tool::Mcp(tool) => servers.push(mcp::to_claude(tool, &mut report)?),
                tool => native.push(tool),
            }
        }
        if !native.is_empty() {
            out.tools = Some(tools::to_claude(native)?);
        }
        if !servers.is_empty() {
            out.mcp_servers = Some(servers);
        }
    }
    let choice = input.tool_choice.or_else(|| {
        input
            .parallel_tool_calls
            .flatten()
            .map(|_| r::ToolChoice::Mode(r::ToolChoiceMode::Auto))
    });
    out.tool_choice = choice
        .map(|v| {
            crate::transform::optional(tools::choice_to_claude(
                v,
                input.parallel_tool_calls.flatten(),
            ))
        })
        .transpose()?
        .flatten();
    let (mut messages, mut system) = history::to_claude(input.input, &mut report)?;
    crate::transform::instructions::claude(&mut messages, &out.model, &mut report);
    if let Some(Some(text)) = input.instructions {
        system.insert(
            0,
            cc::TextBlock::builder(cc::TextBlockType::Tag, text).build(),
        );
    }
    out.messages.append(&mut messages);
    if !system.is_empty() {
        out.system = Some(crate::claude::count_tokens::SystemPrompt::Blocks(system));
    }
    crate::transform::generate::claude_controls::target(
        &out.model,
        &mut out.thinking,
        &mut out.tool_choice,
        &mut report,
    );
    Ok(Converted { value: out, report })
}

/// The thinking block a Responses reasoning item stands for. Only an item whose
/// `encrypted_content` carries a Claude signature (`claude:`, see `signature`)
/// has one: Anthropic accepts thinking only with the signature it issued, and
/// the client's text is the text that signature covers. Any other reasoning
/// (unsigned, OpenAI ciphertext, a Gemini signature) is left out.
pub(super) fn thinking(
    reasoning: r::ReasoningItem,
    report: &mut Report,
) -> Option<cc::ThinkingBlock> {
    let encrypted = reasoning.encrypted_content.flatten();
    let Some(Carried::Claude(signature)) = encrypted.as_deref().and_then(Carried::parse) else {
        report.omitted(
            "reasoning",
            "Claude thinking requires a Claude signature; this reasoning carries none",
        );
        return None;
    };
    if signature.is_empty() {
        report.omitted(
            "reasoning",
            "Claude thinking requires a non-empty signature",
        );
        return None;
    }
    let text = reasoning.content.map_or_else(
        || {
            reasoning
                .summary
                .into_iter()
                .map(|v| v.text)
                .collect::<Vec<_>>()
                .join("")
        },
        |content| {
            content
                .into_iter()
                .map(|v| v.text)
                .collect::<Vec<_>>()
                .join("")
        },
    );
    Some(cc::ThinkingBlock::builder(cc::ThinkingBlockType::Tag, signature.to_owned(), text).build())
}
