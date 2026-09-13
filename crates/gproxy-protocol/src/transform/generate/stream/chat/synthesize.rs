use super::ChatStreamLimits;
use crate::{
    transform::{Converted, Report, TransformError},
    wire::{
        DeclaredFields,
        openai::chat::{content as c, response as r, stream as s},
    },
};
/// Synthesizes native JSON chunks. The framing encoder emits `[DONE]` after
/// these chunks; the source's actual creation time is retained.
pub fn synthesize_chat_stream(
    input: r::GenerateContentResponseBody,
    limits: ChatStreamLimits,
) -> Result<Converted<Vec<s::ChatCompletionChunk>>, TransformError> {
    let input = input.into_declared();
    if input.id.is_empty() || input.model.is_empty() || input.created < 0 {
        return Err(TransformError::invalid_result(
            "response",
            "valid identity/model/creation time required",
        ));
    }
    if input.choices.is_empty() || input.choices.len() > limits.max_choices {
        return Err(super::limit("choices"));
    }
    let mut report = Report::default();
    let mut out = Vec::new();
    let mut bytes = 0;
    let mut tool_count = 0;
    let mut ids = std::collections::BTreeSet::new();
    for (expected, choice) in input.choices.iter().enumerate() {
        if choice.index != expected as i64 {
            return Err(TransformError::invalid_result(
                "choice.index",
                "choices must be ordered and contiguous",
            ));
        }
        if choice
            .message
            .audio
            .as_ref()
            .and_then(Option::as_ref)
            .is_some()
        {
            return Err(TransformError::unsupported(
                "audio",
                "declared Chat chunk has no audio delta",
            ));
        }
        if choice
            .message
            .annotations
            .as_ref()
            .is_some_and(|v| !v.is_empty())
        {
            report.omitted(
                "annotations",
                "declared Chat chunk has no annotations field",
            );
        }
        if choice.message.function_call.is_some() && choice.message.tool_calls.is_some() {
            return Err(TransformError::invalid_result(
                "tool_calls",
                "legacy and modern calls conflict",
            ));
        }
        let mut delta = s::Delta::builder().build();
        delta.role = Some(Some(s::DeltaRole::Assistant));
        delta.content = choice.message.content.clone().map(Some);
        delta.refusal = choice.message.refusal.clone().map(Some);
        if let Some(call) = &choice.message.function_call {
            if call.name.is_empty() {
                return Err(TransformError::invalid_result(
                    "function_call.name",
                    "empty function name",
                ));
            }
            let mut function = s::DeltaFunctionCall::builder().build();
            function.name = Some(Some(call.name.clone()));
            function.arguments = Some(Some(call.arguments.clone()));
            delta.function_call = Some(Some(function));
            tool_count += 1;
        }
        if let Some(calls) = &choice.message.tool_calls {
            let mut tool_calls = Vec::new();
            for (index, call) in calls.iter().enumerate() {
                let c::MessageToolCall::Function(call) = call else {
                    return Err(TransformError::unsupported(
                        "tool_calls.custom",
                        "declared Chat chunk only carries function calls",
                    ));
                };
                if call.id.is_empty()
                    || call.function.name.is_empty()
                    || !ids.insert(call.id.clone())
                {
                    return Err(TransformError::invalid_result(
                        "tool_calls",
                        "empty or duplicate identity/function",
                    ));
                }
                let mut function = s::DeltaFunctionCall::builder().build();
                function.name = Some(Some(call.function.name.clone()));
                function.arguments = Some(Some(call.function.arguments.clone()));
                let mut tool = s::DeltaToolCall::builder(index as i64).build();
                tool.id = Some(Some(call.id.clone()));
                tool.type_ = Some(Some(s::DeltaToolCallType::Function));
                tool.function = Some(Some(function));
                tool_calls.push(tool);
            }
            tool_count += tool_calls.len();
            delta.tool_calls = Some(Some(tool_calls));
        }
        if tool_count > limits.max_tool_calls {
            return Err(super::limit("tool_calls"));
        }
        let has_tools = choice
            .message
            .tool_calls
            .as_ref()
            .is_some_and(|v| !v.is_empty());
        let legacy = choice.message.function_call.is_some();
        if (choice.finish_reason == r::FinishReason::ToolCalls && !has_tools)
            || (choice.finish_reason == r::FinishReason::FunctionCall && !legacy)
            || (choice.finish_reason == r::FinishReason::Stop && (has_tools || legacy))
        {
            return Err(TransformError::invalid_result(
                "finish_reason",
                "tool payload conflicts with terminal",
            ));
        }
        let mut event = s::StreamChoice::builder(choice.index, delta).build();
        event.logprobs = choice
            .logprobs
            .clone()
            .map(super::logs::synthesize)
            .transpose()?
            .map(Some);
        push(&mut out, &mut bytes, base(&input, vec![event]), limits)?;
        let mut finish =
            s::StreamChoice::builder(choice.index, s::Delta::builder().build()).build();
        finish.finish_reason = Some(Some(choice.finish_reason));
        push(&mut out, &mut bytes, base(&input, vec![finish]), limits)?;
    }
    if let Some(usage) = input.usage.clone() {
        let mut chunk = base(&input, Vec::new());
        chunk.usage = Some(Some(super::usage::synthesize(usage)?));
        push(&mut out, &mut bytes, chunk, limits)?;
    }
    Ok(Converted { value: out, report })
}
fn base(
    source: &r::GenerateContentResponseBody,
    choices: Vec<s::StreamChoice>,
) -> s::ChatCompletionChunk {
    let mut out = s::ChatCompletionChunk::builder(
        source.id.clone(),
        choices,
        source.created,
        source.model.clone(),
        s::ChunkObject::ChatCompletionChunk,
    )
    .build();
    out.service_tier = source.service_tier;
    out.system_fingerprint = source.system_fingerprint.clone().map(Some);
    out.moderation = source.moderation.clone();
    out
}
fn push(
    out: &mut Vec<s::ChatCompletionChunk>,
    bytes: &mut usize,
    value: s::ChatCompletionChunk,
    limits: ChatStreamLimits,
) -> Result<(), TransformError> {
    if out.len() >= limits.max_events {
        return Err(super::limit("events"));
    }
    let remaining = limits
        .max_bytes
        .checked_sub(*bytes)
        .ok_or_else(|| super::limit("bytes"))?;
    *bytes += super::encoded(&value, remaining)?;
    out.push(value);
    Ok(())
}
