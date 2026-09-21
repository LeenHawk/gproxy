use crate::{
    Dialect,
    transform::{
        Converted, Report, TransformError,
        identity::{IdentityFlow, IdentityRole, TargetIdPolicy},
    },
    wire::{
        DeclaredFields,
        openai::{
            chat::response as c,
            responses::{generate as g, response as r},
        },
    },
};
pub(crate) mod annotations;
mod content;
mod facts;
pub(crate) mod moderation;
pub(crate) mod usage;
pub use facts::ResponsesResponseContext;
pub use usage::ChatUsageSupplement;

/// Convert one completed Chat choice using facts retained from its original
/// Responses request. The dedicated response identity flow is reused on retries
/// and by streaming adapters; it is committed only after successful conversion.
pub fn chat_to_responses_response(
    input: c::GenerateContentResponseBody,
    context: ResponsesResponseContext,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
) -> Result<Converted<r::GenerateContentResponseBody>, TransformError> {
    if policy.dialect != Dialect::OpenAi {
        return Err(TransformError::shape(
            "identity.policy",
            "expected Responses target policy",
        ));
    }
    let input = input.into_declared();
    if input.choices.len() != 1 {
        return Err(TransformError::unsupported(
            "choices",
            "one Responses response requires exactly one Chat choice; fan-out belongs to the invocation adapter",
        ));
    }
    if input.created < 0 {
        return Err(TransformError::invalid_result(
            "created",
            "negative creation timestamp",
        ));
    }
    let mut ids = flow.clone();
    let bindings = super::client_tools::Bindings::new(&context.request)?;
    let mut report = Report::default();
    let choice = input.choices.into_iter().next().expect("one choice");
    if choice.index != 0 {
        return Err(TransformError::invalid_result(
            "choices.index",
            "one Chat choice must have index zero",
        ));
    }
    let reason = match choice.finish_reason {
        c::FinishReason::Length => Some(r::ResponseIncompleteReason::MaxOutputTokens),
        c::FinishReason::ContentFilter => Some(r::ResponseIncompleteReason::ContentFilter),
        c::FinishReason::Stop | c::FinishReason::ToolCalls | c::FinishReason::FunctionCall => None,
    };
    let incomplete = reason.is_some();
    let usage = input
        .usage
        .map(|value| usage::to_responses(value, context.usage, &mut report))
        .map(crate::transform::optional)
        .transpose()?
        .flatten();
    let id = content::identity(
        &mut ids,
        policy,
        Dialect::OpenAiChat,
        IdentityRole::Response,
        IdentityRole::Response,
        Some(input.id),
        0,
    )?;
    let output = content::to_responses(
        choice.message,
        choice.logprobs,
        incomplete,
        &mut ids,
        policy,
        &mut report,
        &bindings,
    )?;
    let mut target = context.into_response(id, input.created, input.model)?;
    target.output = output;
    target.usage = usage.map(Some);
    target.status = Some(if incomplete {
        r::ResponseStatus::Incomplete
    } else {
        r::ResponseStatus::Completed
    });
    target.incomplete_details = reason.map(|reason| r::ResponseIncompleteDetails {
        reason: Some(reason),
        rest: Default::default(),
    });
    target.service_tier = input.service_tier.map(|v| v.map(tier_to_responses));
    target.moderation = input
        .moderation
        .map(|v| {
            v.map(|v| moderation::to_responses(v, &mut report))
                .transpose()
        })
        .map(crate::transform::optional)
        .transpose()?
        .flatten();
    if input.system_fingerprint.is_some() {
        report.omitted("system_fingerprint", "Responses has no fingerprint field");
    }
    *flow = ids;
    Ok(Converted {
        value: target,
        report,
    })
}

/// Converts a terminal Responses object. Failed, pending, and cancelled
/// responses are never reported as successful Chat completions.
pub fn responses_to_chat_response(
    input: r::GenerateContentResponseBody,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
) -> Result<Converted<c::GenerateContentResponseBody>, TransformError> {
    if policy.dialect != Dialect::OpenAiChat {
        return Err(TransformError::shape(
            "identity.policy",
            "expected Chat target policy",
        ));
    }
    let input = input.into_declared();
    if let Some(error) = input.error {
        return Err(TransformError::invalid_result(
            "response.error",
            error.message,
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
                "status",
                "response is not a successful terminal result",
            ));
        }
        None => return Err(TransformError::missing_metadata("response.status")),
    };
    let terminal_reason = if completed {
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
                r::ResponseIncompleteReason::MaxOutputTokens => c::FinishReason::Length,
                r::ResponseIncompleteReason::ContentFilter => c::FinishReason::ContentFilter,
            },
        )
    };
    if input.created_at < 0 {
        return Err(TransformError::invalid_result(
            "created_at",
            "negative creation timestamp",
        ));
    }
    let mut ids = flow.clone();
    let mut report = Report::default();
    let result = content::to_chat(input.output, completed, &mut ids, policy, &mut report)?;
    let finish_reason = terminal_reason.unwrap_or(if result.has_tools {
        c::FinishReason::ToolCalls
    } else {
        c::FinishReason::Stop
    });
    let id = content::identity(
        &mut ids,
        policy,
        Dialect::OpenAi,
        IdentityRole::Response,
        IdentityRole::Response,
        Some(input.id),
        0,
    )?;
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
    ] {
        if present {
            report.omitted(
                field,
                "Chat does not echo this Responses request or lifecycle metadata",
            );
        }
    }
    let value = c::GenerateContentResponseBody {
        id,
        choices: vec![c::Choice {
            finish_reason,
            index: 0,
            logprobs: result.logprobs,
            message: result.message,
            rest: Default::default(),
        }],
        created: input.created_at,
        model: input.model,
        object: c::CompletionObject::ChatCompletion,
        service_tier: input.service_tier.map(|v| v.map(tier_to_chat)),
        system_fingerprint: None,
        usage: input
            .usage
            .flatten()
            .map(usage::to_chat)
            .map(crate::transform::optional)
            .transpose()?
            .flatten(),
        moderation: input
            .moderation
            .map(|v| v.map(moderation::to_chat).transpose())
            .map(crate::transform::optional)
            .transpose()?
            .flatten(),
        rest: Default::default(),
    };
    *flow = ids;
    Ok(Converted { value, report })
}

pub(crate) fn tier_to_responses(value: c::ResponseServiceTier) -> g::ServiceTier {
    match value {
        c::ResponseServiceTier::Auto => g::ServiceTier::Auto,
        c::ResponseServiceTier::Default => g::ServiceTier::Default,
        c::ResponseServiceTier::Flex => g::ServiceTier::Flex,
        c::ResponseServiceTier::Scale => g::ServiceTier::Scale,
        c::ResponseServiceTier::Priority => g::ServiceTier::Priority,
        c::ResponseServiceTier::Fast => g::ServiceTier::Fast,
    }
}

pub(crate) fn tier_to_chat(value: g::ServiceTier) -> c::ResponseServiceTier {
    match value {
        g::ServiceTier::Auto => c::ResponseServiceTier::Auto,
        g::ServiceTier::Default => c::ResponseServiceTier::Default,
        g::ServiceTier::Flex => c::ResponseServiceTier::Flex,
        g::ServiceTier::Scale => c::ResponseServiceTier::Scale,
        g::ServiceTier::Priority => c::ResponseServiceTier::Priority,
        g::ServiceTier::Fast => c::ResponseServiceTier::Fast,
    }
}
