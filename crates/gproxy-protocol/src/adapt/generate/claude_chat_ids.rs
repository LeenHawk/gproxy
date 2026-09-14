//! The buffered Claude/Chat pure pair preserves IDs. Apply the selected target
//! policy here and retain its exact call/result association in the invocation flow.
use super::GenerationIdentity;
use crate::{
    Dialect,
    transform::{
        TransformError,
        identity::{IdentityFlow, IdentityRole, SourceIdentity, TargetIdPolicy},
    },
    wire::{
        claude::{content as cc, generate_content as c},
        openai::chat as h,
    },
};
use std::collections::BTreeMap;
pub(super) fn allocate(
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
    dialect: Dialect,
    role: IdentityRole,
    id: &str,
    index: u64,
) -> Result<String, TransformError> {
    flow.resolve_or_allocate(
        role,
        SourceIdentity::new(dialect, Some(id.into()), index),
        policy,
    )
    .map(|h| h.emitted_id)
    .map_err(|e| TransformError::shape("identity", e.to_string()))
}
pub(super) fn result_id(
    id: &mut String,
    aliases: &BTreeMap<String, String>,
    policy: &TargetIdPolicy,
) -> Result<(), TransformError> {
    if let Some(mapped) = aliases.get(id) {
        *id = mapped.clone();
    } else if !policy.accepts_source(id) {
        return Err(TransformError::missing_metadata(
            "tool result requires corresponding declared call or target-valid restored ID",
        ));
    }
    Ok(())
}
pub(super) fn chat_request(
    value: &mut h::GenerateContentRequestBody,
    identities: &mut GenerationIdentity,
    source: Dialect,
) -> Result<(), TransformError> {
    let mut flow = identities.request.clone();
    let policy = &identities.request_policy;
    let mut aliases = BTreeMap::new();
    let mut index = 0;
    for message in &mut value.messages {
        if let h::ChatMessage::Assistant(message) = message {
            for call in message.tool_calls.iter_mut().flatten() {
                let id = match call {
                    h::MessageToolCall::Function(call) => &mut call.id,
                    h::MessageToolCall::Custom(call) => &mut call.id,
                };
                let mapped =
                    allocate(&mut flow, policy, source, IdentityRole::ToolCall, id, index)?;
                index += 1;
                aliases.insert(id.clone(), mapped.clone());
                *id = mapped;
            }
        }
    }
    for message in &mut value.messages {
        if let h::ChatMessage::Tool(message) = message {
            result_id(&mut message.tool_call_id, &aliases, policy)?;
        }
    }
    identities.request = flow;
    Ok(())
}
pub(super) fn claude_request(
    value: &mut c::GenerateContentRequestBody,
    identities: &mut GenerationIdentity,
    source: Dialect,
) -> Result<(), TransformError> {
    let mut flow = identities.request.clone();
    let policy = &identities.request_policy;
    let mut aliases = BTreeMap::new();
    let mut index = 0;
    for message in &mut value.messages {
        if let cc::MessageContent::Blocks(blocks) = &mut message.content {
            for block in blocks {
                if let cc::ContentBlock::ToolUse(block) = block {
                    let mapped = allocate(
                        &mut flow,
                        policy,
                        source,
                        IdentityRole::ToolCall,
                        &block.id,
                        index,
                    )?;
                    index += 1;
                    aliases.insert(block.id.clone(), mapped.clone());
                    block.id = mapped;
                }
            }
        }
    }
    for message in &mut value.messages {
        if let cc::MessageContent::Blocks(blocks) = &mut message.content {
            for block in blocks {
                if let cc::ContentBlock::ToolResult(block) = block {
                    result_id(&mut block.tool_use_id, &aliases, policy)?;
                }
            }
        }
    }
    identities.request = flow;
    Ok(())
}
pub(super) fn chat_response(
    value: &mut h::GenerateContentResponseBody,
    identities: &mut GenerationIdentity,
) -> Result<(), TransformError> {
    let mut flow = identities.response.clone();
    let policy = &identities.response_policy;
    value.id = allocate(
        &mut flow,
        policy,
        Dialect::Claude,
        IdentityRole::Response,
        &value.id,
        0,
    )?;
    for (index, call) in value
        .choices
        .iter_mut()
        .flat_map(|c| c.message.tool_calls.iter_mut().flatten())
        .enumerate()
    {
        let id = match call {
            h::MessageToolCall::Function(call) => &mut call.id,
            h::MessageToolCall::Custom(call) => &mut call.id,
        };
        *id = allocate(
            &mut flow,
            policy,
            Dialect::Claude,
            IdentityRole::ToolCall,
            id,
            index as u64,
        )?;
    }
    identities.response = flow;
    Ok(())
}
pub(super) fn claude_response(
    value: &mut c::GenerateContentResponseBody,
    identities: &mut GenerationIdentity,
) -> Result<(), TransformError> {
    let mut flow = identities.response.clone();
    let policy = &identities.response_policy;
    value.id = allocate(
        &mut flow,
        policy,
        Dialect::OpenAiChat,
        IdentityRole::Response,
        &value.id,
        0,
    )?;
    let mut index = 0;
    for block in &mut value.content {
        if let c::ResponseContentBlock::ToolUse(block) = block {
            block.id = allocate(
                &mut flow,
                policy,
                Dialect::OpenAiChat,
                IdentityRole::ToolCall,
                &block.id,
                index,
            )?;
            index += 1;
        }
    }
    identities.response = flow;
    Ok(())
}
