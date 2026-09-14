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
        identity::{IdentityFlow, TargetIdPolicy},
    },
    wire::{
        DeclaredFields,
        claude::{content as cc, generate_content as c},
        openai::responses as r,
    },
};
/// A native signed block and its scoped identity record recovered by the host.
pub struct RestoredClaudeThinking {
    pub state: crate::transform::identity::IdentityStateRecord,
    pub block: cc::ThinkingBlock,
}
#[derive(Default)]
pub struct ClaudeRequestContext {
    pub target: Option<crate::transform::identity::IdentityTarget>,
    pub restored_thinking: std::collections::BTreeMap<String, RestoredClaudeThinking>,
}
pub fn claude_to_responses_request(
    input: c::GenerateContentRequestBody,
    target_model: impl Into<String>,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
) -> Result<Converted<r::GenerateContentRequestBody>, TransformError> {
    if policy.dialect != crate::Dialect::OpenAi {
        return Err(TransformError::shape(
            "identity.policy",
            "Responses target required",
        ));
    }
    let model = target_model.into();
    if model.is_empty() {
        return Err(TransformError::missing_metadata("target_model"));
    }
    let input = input.into_declared();
    let mut ids = flow.clone();
    let mut report = Report::default();
    let mut out = r::GenerateContentRequestBody::builder().build();
    controls::to_responses(&input, &mut out, &mut report)?;
    out.model = Some(model.clone());
    out.tools = input
        .tools
        .map(|v| tools::to_responses(v, &mut report))
        .transpose()?;
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
    Ok(Converted { value: out, report })
}
pub fn responses_to_claude_request(
    input: r::GenerateContentRequestBody,
    target_model: impl Into<String>,
    context: ClaudeRequestContext,
) -> Result<Converted<c::GenerateContentRequestBody>, TransformError> {
    let model = target_model.into();
    if model.is_empty() {
        return Err(TransformError::missing_metadata("target_model"));
    }
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
        .filter(|v| *v > 0)
        .ok_or_else(|| TransformError::missing_metadata("max_output_tokens positive budget"))?;
    let mut out = c::GenerateContentRequestBody::builder(max, Vec::new(), model).build();
    let mut report = Report::default();
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
        .map(|v| tools::choice_to_claude(v, input.parallel_tool_calls.flatten()))
        .transpose()?;
    let (mut messages, mut system) = history::to_claude(input.input, context, &mut report)?;
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
    Ok(Converted { value: out, report })
}

pub(super) fn restore_reasoning(
    reasoning: r::ReasoningItem,
    target_model: &str,
    context: &mut ClaudeRequestContext,
) -> Result<cc::ThinkingBlock, TransformError> {
    let binding = context
        .target
        .as_ref()
        .ok_or_else(|| TransformError::missing_metadata("reasoning target"))?;
    if binding.model != target_model
        || binding.dialect != crate::Dialect::Claude
        || binding
            .origin
            .as_ref()
            .is_none_or(|origin| origin.trim().is_empty())
    {
        return Err(TransformError::shape(
            "reasoning.target",
            "model/dialect/origin binding does not match selected Claude target",
        ));
    }

    let restored = context
        .restored_thinking
        .remove(&reasoning.id)
        .ok_or_else(|| {
            TransformError::missing_metadata(format!(
                "reasoning[{}] native Claude signed block",
                reasoning.id
            ))
        })?;
    let expected = context
        .target
        .as_ref()
        .ok_or_else(|| TransformError::missing_metadata("reasoning target origin/model"))?;
    restored
        .state
        .validate_for(expected)
        .map_err(|error| TransformError::shape("reasoning.state", error.to_string()))?;
    use crate::transform::identity::{IdentityRole, OutputItemKind};
    if restored.state.role != IdentityRole::OutputItem(OutputItemKind::Reasoning)
        || restored.state.client_item_id.as_deref() != Some(reasoning.id.as_str())
    {
        return Err(TransformError::shape(
            "reasoning.state",
            "record must bind this reasoning item and field role",
        ));
    }
    let original = restored.block.into_declared();
    if restored
        .state
        .opaque_signature
        .as_ref()
        .is_none_or(|signature| {
            signature.value != original.signature
                || signature.field
                    != crate::transform::identity::OpaqueField::ClaudeThinkingSignature
        })
        || original.signature.is_empty()
    {
        return Err(TransformError::shape(
            "reasoning.signature",
            "native block signature does not match scoped record",
        ));
    }
    let presented = reasoning
        .content
        .map(|content| {
            content
                .into_iter()
                .map(|v| v.text)
                .collect::<Vec<_>>()
                .join("")
        })
        .unwrap_or_else(|| {
            reasoning
                .summary
                .into_iter()
                .map(|v| v.text)
                .collect::<Vec<_>>()
                .join("")
        });
    if presented != original.thinking || reasoning.encrypted_content.flatten().is_some() {
        return Err(TransformError::shape(
            "reasoning.content",
            "modified or foreign encrypted reasoning cannot bind native Claude signature",
        ));
    }
    Ok(original)
}
