use crate::{
    Dialect,
    transform::{
        Converted, Report, TransformError,
        identity::{IdentityFlow, IdentityRole, TargetIdPolicy},
    },
    wire::{DeclaredFields, claude::generate_content as c, openai::responses as r},
};
mod content;
mod facts;
mod mcp;
mod usage;
pub use facts::ClaudeResponseContext;
pub use usage::ResponsesUsageFacts;

/// Return mapping for a Responses request executed through Claude. Original
/// request controls and actual creation/usage facts are retained by invocation.
pub fn claude_to_responses_response(
    input: c::GenerateContentResponseBody,
    context: ClaudeResponseContext,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
) -> Result<Converted<r::GenerateContentResponseBody>, TransformError> {
    if policy.dialect != Dialect::OpenAi {
        return Err(TransformError::shape(
            "identity.policy",
            "expected Responses policy",
        ));
    }
    let input = input.into_declared();
    if context.created_at < 0 {
        return Err(TransformError::shape(
            "created_at",
            "negative creation time",
        ));
    }
    let incomplete = match input.stop_reason {
        c::StopReason::MaxTokens => true,
        c::StopReason::EndTurn
        | c::StopReason::ToolUse
        | c::StopReason::StopSequence
        | c::StopReason::Refusal => false,
        c::StopReason::ModelContextWindowExceeded => {
            return Err(TransformError::unsupported(
                "stop_reason",
                "context window exhaustion is not a Responses max_output_tokens stop",
            ));
        }
        c::StopReason::PauseTurn | c::StopReason::Compaction => {
            return Err(TransformError::unsupported(
                "stop_reason",
                "paused execution or replacement history requires invocation continuation",
            ));
        }
    };
    let mut ids = flow.clone();
    let mut report = Report::default();
    let response_id = content::id(
        &mut ids,
        policy,
        Dialect::Claude,
        IdentityRole::Response,
        IdentityRole::Response,
        Some(input.id),
        0,
    )?;
    let output = content::to_responses(
        input.content,
        incomplete,
        input.stop_reason == c::StopReason::Refusal,
        &mut ids,
        policy,
        &mut report,
    )?;
    if input.stop_reason == c::StopReason::ToolUse
        && !output
            .iter()
            .any(|item| matches!(item, r::ResponseOutputItem::FunctionCall(_)))
    {
        return Err(TransformError::invalid_result(
            "stop_reason",
            "tool_use stop has no function call",
        ));
    }
    let tier = input
        .usage
        .service_tier
        .flatten()
        .and_then(|value| match value {
            c::ResponseServiceTier::Standard => Some(r::ServiceTier::Default),
            c::ResponseServiceTier::Priority => Some(r::ServiceTier::Priority),
            c::ResponseServiceTier::Batch => {
                report.omitted("usage.service_tier", "Responses has no batch tier");
                None
            }
        });
    let usage = usage::to_responses(input.usage, context.usage, &mut report)?;
    let created_at = context.created_at;
    let mut target = context.into_response(response_id, created_at, input.model)?;
    target.output = output;
    target.usage = Some(usage);
    target.service_tier = tier.map(Some);
    target.status = Some(if incomplete {
        r::ResponseStatus::Incomplete
    } else {
        r::ResponseStatus::Completed
    });
    target.incomplete_details = incomplete.then(|| r::ResponseIncompleteDetails {
        reason: Some(r::ResponseIncompleteReason::MaxOutputTokens),
        rest: Default::default(),
    });
    for (present, field) in [
        (input.container.is_some(), "container"),
        (input.context_management.is_some(), "context_management"),
        (input.diagnostics.is_some(), "diagnostics"),
        (input.stop_details.is_some(), "stop_details"),
        (input.stop_sequence.is_some(), "stop_sequence"),
    ] {
        if present {
            report.omitted(
                field,
                "Responses has no equivalent Claude response metadata field",
            );
        }
    }
    *flow = ids;
    Ok(Converted {
        value: target,
        report,
    })
}

/// Convert a successful terminal Responses result; pending/failed/cancelled
/// objects remain failures instead of becoming Claude end_turn messages.
pub fn responses_to_claude_response(
    input: r::GenerateContentResponseBody,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
) -> Result<Converted<c::GenerateContentResponseBody>, TransformError> {
    convert_to_claude(input, None, flow, policy)
}

/// Restore signed Claude thinking only when the caller supplies its validated
/// original-origin state and native block. The context is consumed on use.
pub fn responses_to_claude_response_with_context(
    input: r::GenerateContentResponseBody,
    context: super::request::ClaudeRequestContext,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
) -> Result<Converted<c::GenerateContentResponseBody>, TransformError> {
    convert_to_claude(input, Some(context), flow, policy)
}
fn convert_to_claude(
    input: r::GenerateContentResponseBody,
    mut context: Option<super::request::ClaudeRequestContext>,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
) -> Result<Converted<c::GenerateContentResponseBody>, TransformError> {
    if policy.dialect != Dialect::Claude {
        return Err(TransformError::shape(
            "identity.policy",
            "expected Claude policy",
        ));
    }
    let input = input.into_declared();
    if input.error.is_some() {
        return Err(TransformError::invalid_result(
            "response.error",
            "upstream response contains an error",
        ));
    }
    let completed = match input.status {
        Some(r::ResponseStatus::Completed) => true,
        Some(r::ResponseStatus::Incomplete) => false,
        Some(
            r::ResponseStatus::Failed
            | r::ResponseStatus::Cancelled
            | r::ResponseStatus::Queued
            | r::ResponseStatus::InProgress,
        ) => {
            return Err(TransformError::invalid_result(
                "response.status",
                "not a successful terminal response",
            ));
        }
        None => return Err(TransformError::missing_metadata("response.status")),
    };
    let reason = if completed {
        if input.incomplete_details.is_some() {
            return Err(TransformError::invalid_result(
                "incomplete_details",
                "completed response has incomplete details",
            ));
        }
        None
    } else {
        Some(
            match input
                .incomplete_details
                .and_then(|v| v.reason)
                .ok_or_else(|| TransformError::missing_metadata("incomplete_details.reason"))?
            {
                r::ResponseIncompleteReason::MaxOutputTokens => c::StopReason::MaxTokens,
                r::ResponseIncompleteReason::ContentFilter => c::StopReason::Refusal,
            },
        )
    };
    let mut ids = flow.clone();
    let mut report = Report::default();
    let content = content::to_claude(
        input.output,
        completed,
        &mut ids,
        policy,
        &mut report,
        content::Restoration {
            model: &input.model,
            context: context.as_mut(),
        },
    )?;
    let id = content::id(
        &mut ids,
        policy,
        Dialect::OpenAi,
        IdentityRole::Response,
        IdentityRole::Response,
        Some(input.id),
        0,
    )?;
    let mut usage = usage::to_claude(
        input
            .usage
            .ok_or_else(|| TransformError::missing_metadata("response.usage"))?,
    )?;
    usage.service_tier = input.service_tier.flatten().and_then(|tier| match tier {
        r::ServiceTier::Default => Some(Some(c::ResponseServiceTier::Standard)),
        r::ServiceTier::Priority => Some(Some(c::ResponseServiceTier::Priority)),
        r::ServiceTier::Auto
        | r::ServiceTier::Flex
        | r::ServiceTier::Scale
        | r::ServiceTier::Fast => {
            report.omitted(
                "service_tier",
                "Claude has no equivalent effective service tier",
            );
            None
        }
    });
    let stop_reason = reason.unwrap_or(if content.tools {
        c::StopReason::ToolUse
    } else if content.refusal {
        c::StopReason::Refusal
    } else {
        c::StopReason::EndTurn
    });
    for (present, field) in [
        (input.instructions.is_some(), "instructions"),
        (input.metadata.is_some(), "metadata"),
        (!input.tools.is_empty(), "tools"),
        (input.temperature.is_some(), "temperature"),
        (input.top_p.is_some(), "top_p"),
        (input.conversation.is_some(), "conversation"),
        (input.completed_at.is_some(), "completed_at"),
        (input.max_output_tokens.is_some(), "max_output_tokens"),
        (input.max_tool_calls.is_some(), "max_tool_calls"),
        (input.previous_response_id.is_some(), "previous_response_id"),
        (input.prompt.is_some(), "prompt"),
        (input.prompt_cache_key.is_some(), "prompt_cache_key"),
        (input.prompt_cache_options.is_some(), "prompt_cache_options"),
        (
            input.prompt_cache_retention.is_some(),
            "prompt_cache_retention",
        ),
        (input.reasoning.is_some(), "reasoning"),
        (input.safety_identifier.is_some(), "safety_identifier"),
        (input.text.is_some(), "text"),
        (input.top_logprobs.is_some(), "top_logprobs"),
        (input.truncation.is_some(), "truncation"),
        (input.user.is_some(), "user"),
        (input.moderation.is_some(), "moderation"),
        (input.background.is_some(), "background"),
    ] {
        if present {
            report.omitted(
                field,
                "Claude does not echo this Responses request or lifecycle metadata",
            );
        }
    }
    let value = c::GenerateContentResponseBody {
        type_: c::GenerateContentResponseBodyType::Tag,
        id,
        container: None,
        content: content.blocks,
        context_management: None,
        diagnostics: None,
        model: input.model,
        role: c::ResponseRole::Assistant,
        stop_details: None,
        stop_reason,
        stop_sequence: None,
        usage,
        rest: Default::default(),
    };
    *flow = ids;
    Ok(Converted { value, report })
}
