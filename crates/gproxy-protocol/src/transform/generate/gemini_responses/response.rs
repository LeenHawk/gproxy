mod facts;
pub use super::usage::GeminiUsageFacts;
use crate::{
    transform::{
        Converted, Report, TransformError,
        identity::{IdentityFlow, IdentityRole, OutputItemKind, TargetIdPolicy},
    },
    wire::{
        DeclaredFields, gemini as g,
        openai::responses::{input as i, response as r},
    },
};
pub use facts::GeminiResponseContext;
pub fn gemini_to_responses_response(
    input: g::GenerateContentResponseBody,
    context: GeminiResponseContext,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
) -> Result<Converted<r::GenerateContentResponseBody>, TransformError> {
    if policy.dialect != crate::Dialect::OpenAi {
        return Err(TransformError::shape(
            "identity.policy",
            "Responses target required",
        ));
    }
    if context.created_at < 0 {
        return Err(TransformError::shape("created_at", "negative timestamp"));
    }
    let input = input.into_declared();
    let candidates = input
        .candidates
        .ok_or_else(|| TransformError::invalid_result("candidates", "no candidates"))?;
    if candidates.len() != 1 {
        return Err(TransformError::unsupported(
            "candidates",
            "Responses represents one candidate; fanout required",
        ));
    }
    let candidate = candidates.into_iter().next().unwrap();
    if candidate.index.is_some_and(|n| n != 0) {
        return Err(TransformError::invalid_result(
            "candidate.index",
            "single candidate index must be zero",
        ));
    }
    let reason = match candidate
        .finish_reason
        .ok_or_else(|| TransformError::missing_metadata("finish_reason"))?
    {
        g::FinishReason::Stop => None,
        g::FinishReason::MaxTokens => Some(r::ResponseIncompleteReason::MaxOutputTokens),
        g::FinishReason::Safety
        | g::FinishReason::Recitation
        | g::FinishReason::Language
        | g::FinishReason::Blocklist
        | g::FinishReason::ProhibitedContent
        | g::FinishReason::Spii
        | g::FinishReason::ImageSafety
        | g::FinishReason::ImageProhibitedContent
        | g::FinishReason::ImageRecitation => Some(r::ResponseIncompleteReason::ContentFilter),
        g::FinishReason::Unspecified
        | g::FinishReason::Other
        | g::FinishReason::MalformedFunctionCall
        | g::FinishReason::ImageOther
        | g::FinishReason::NoImage
        | g::FinishReason::UnexpectedToolCall
        | g::FinishReason::TooManyToolCalls
        | g::FinishReason::MissingThoughtSignature
        | g::FinishReason::MalformedResponse => {
            return Err(TransformError::invalid_result(
                "finish_reason",
                "non-successful Gemini generation",
            ));
        }
    };
    let incomplete = reason.is_some();
    let bindings =
        super::super::client_tools::Bindings::for_target(&context.request, crate::Dialect::Gemini)?;
    let mut ids = flow.clone();
    let mut report = Report::default();
    let usage = super::usage::to_responses(
        input
            .usage_metadata
            .ok_or_else(|| TransformError::missing_metadata("usage_metadata"))?,
        context.usage,
        &mut report,
    )?;
    let id = super::identity::id(
        &mut ids,
        policy,
        IdentityRole::Response,
        IdentityRole::Response,
        input.response_id,
        0,
    )?;
    let mut output = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (index, part) in candidate
        .content
        .unwrap_or_else(|| g::Content::builder().build())
        .parts
        .unwrap_or_default()
        .into_iter()
        .enumerate()
    {
        super::content::validate(&part)?;
        if part.file_data.is_some() || part.function_response.is_some() {
            return Err(TransformError::unsupported(
                "candidate.part",
                "Responses output media/server results need native adapter",
            ));
        }
        if part.thought_signature.is_some() {
            report.omitted(
                "thought_signature",
                "signature retained only in host scoped native replay state",
            );
        }
        if let Some(blob) = &part.inline_data {
            super::images::validate_part(&part)?;
            let id = super::identity::id(
                &mut ids,
                policy,
                IdentityRole::Message,
                IdentityRole::OutputItem(OutputItemKind::ImageGenerationCall),
                None,
                index as u64,
            )?;
            super::images::requested_format(blob, &context.request)?;
            let item = super::images::to_responses(blob.clone(), id, blob.data.len() as u64)?;
            output.push(r::ResponseOutputItem::ImageGenerationCall(item));
            continue;
        }
        if let Some(text) = part.text {
            if part.thought == Some(true) {
                let id = super::identity::id(
                    &mut ids,
                    policy,
                    IdentityRole::Message,
                    IdentityRole::OutputItem(OutputItemKind::Reasoning),
                    None,
                    index as u64,
                )?;
                let mut item =
                    i::ReasoningItem::builder(i::ReasoningItemType::ReasoningItem, id, Vec::new())
                        .build();
                item.content = Some(vec![
                    i::ReasoningContent::builder(i::ReasoningTextType::ReasoningText, text).build(),
                ]);
                item.status = Some(if incomplete {
                    i::ReasoningStatus::Incomplete
                } else {
                    i::ReasoningStatus::Completed
                });
                output.push(r::ResponseOutputItem::Reasoning(item));
            } else {
                let id = super::identity::id(
                    &mut ids,
                    policy,
                    IdentityRole::Message,
                    IdentityRole::OutputItem(OutputItemKind::Message),
                    None,
                    index as u64,
                )?;
                output.push(r::ResponseOutputItem::Message(
                    i::ResponseOutputMessage::builder(
                        id,
                        vec![i::OutputContent::Text(
                            i::ResponseOutputText::builder(
                                i::ResponseOutputTextType::ResponseOutputText,
                                text,
                                Vec::new(),
                                Vec::new(),
                            )
                            .build(),
                        )],
                        i::OutputMessageRole::Assistant,
                        if incomplete {
                            i::OutputMessageStatus::Incomplete
                        } else {
                            i::OutputMessageStatus::Completed
                        },
                        i::MessageType::Message,
                    )
                    .build(),
                ));
            }
        }
        if let Some(call) = part.function_call {
            if call.id.as_ref().is_some_and(|id| !seen.insert(id.clone())) {
                return Err(TransformError::invalid_result(
                    "call.id",
                    "duplicate call identity",
                ));
            }
            let call_id = super::identity::id(
                &mut ids,
                policy,
                IdentityRole::ToolCall,
                IdentityRole::ToolCall,
                call.id.clone(),
                index as u64,
            )?;
            let item_id = super::identity::id(
                &mut ids,
                policy,
                IdentityRole::ToolCall,
                IdentityRole::OutputItem(bindings.kind(&call.name)),
                call.id,
                index as u64,
            )?;
            let mut item = i::FunctionCall::builder(
                i::FunctionCallType::FunctionCall,
                serde_json::to_string(&call.args.unwrap_or_default())?,
                call_id,
                call.name,
            )
            .id(item_id)
            .build();
            item.status = Some(if incomplete {
                i::ItemStatus::Incomplete
            } else {
                i::ItemStatus::Completed
            });
            output.push(bindings.restore(item)?);
        }
    }
    for (present, field) in [
        (candidate.citation_metadata.is_some(), "citations"),
        (candidate.grounding_metadata.is_some(), "grounding"),
        (candidate.logprobs_result.is_some(), "logprobs"),
        (candidate.safety_ratings.is_some(), "safety_ratings"),
        (input.prompt_feedback.is_some(), "prompt_feedback"),
        (input.model_status.is_some(), "model_status"),
    ] {
        if present {
            report.omitted(
                field,
                "Gemini metadata needs target-specific detail mapping",
            );
        }
    }
    let model = input
        .model_version
        .or_else(|| context.request.model.clone())
        .ok_or_else(|| TransformError::missing_metadata("actual model"))?;
    let created = context.created_at;
    let mut out = context.into_response(id, created, model)?;
    out.output = output;
    out.usage = Some(Some(usage));
    out.status = Some(if incomplete {
        r::ResponseStatus::Incomplete
    } else {
        r::ResponseStatus::Completed
    });
    out.incomplete_details = reason.map(|reason| {
        r::ResponseIncompleteDetails::builder()
            .reason(reason)
            .build()
    });
    *flow = ids;
    Ok(Converted { value: out, report })
}
pub fn responses_to_gemini_response(
    input: r::GenerateContentResponseBody,
    context: super::identity::GeminiReplayContext,
) -> Result<Converted<g::GenerateContentResponseBody>, TransformError> {
    responses_to_gemini_response_with_modalities(input, context, None)
}
pub fn responses_to_gemini_response_with_modalities(
    input: r::GenerateContentResponseBody,
    mut context: super::identity::GeminiReplayContext,
    modalities: Option<&[g::Modality]>,
) -> Result<Converted<g::GenerateContentResponseBody>, TransformError> {
    let input = input.into_declared();
    if input.error.is_some() {
        return Err(TransformError::invalid_result("error", "upstream error"));
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
                "response is not successful terminal",
            ));
        }
        None => return Err(TransformError::missing_metadata("status")),
    };
    let mut reason = if completed {
        if input.incomplete_details.is_some() {
            return Err(TransformError::invalid_result(
                "incomplete_details",
                "completed response has incomplete details",
            ));
        }
        g::FinishReason::Stop
    } else {
        match input
            .incomplete_details
            .and_then(|v| v.reason)
            .ok_or_else(|| TransformError::missing_metadata("incomplete.reason"))?
        {
            r::ResponseIncompleteReason::MaxOutputTokens => g::FinishReason::MaxTokens,
            r::ResponseIncompleteReason::ContentFilter => g::FinishReason::Safety,
        }
    };
    let mut report = Report::default();
    let usage = super::usage::to_gemini(
        input
            .usage
            .flatten()
            .ok_or_else(|| TransformError::missing_metadata("usage"))?,
        &mut report,
    )?;
    let mut parts = Vec::new();
    let mut logs = Vec::new();
    let mut citations = Vec::new();
    let image_only = modalities.is_some_and(|values| {
        !values.is_empty() && values.iter().all(|v| *v == g::Modality::Image)
    });
    let refused = input.output.iter().any(|item| matches!(item,
        r::ResponseOutputItem::Message(message) if message.content.iter().any(|part| matches!(part, i::OutputContent::Refusal(_)))));
    if image_only && completed && refused {
        reason = g::FinishReason::Safety;
    }
    for item in input.output {
        if image_only && !matches!(item, r::ResponseOutputItem::ImageGenerationCall(_)) {
            report.omitted(
                "output.modalities",
                "non-image content excluded by the client's IMAGE-only response modalities",
            );
            continue;
        }
        match item {
            r::ResponseOutputItem::Message(message) => {
                if message.status == i::OutputMessageStatus::InProgress
                    || completed && message.status == i::OutputMessageStatus::Incomplete
                {
                    return Err(TransformError::invalid_result(
                        "output.message.status",
                        "nonterminal content",
                    ));
                }
                for part in message.content {
                    match part {
                        i::OutputContent::Text(text) => {
                            citations
                                .extend(super::logs::citations(text.annotations, &mut report)?);
                            logs.extend(text.logprobs);
                            parts.push(g::Part::builder().text(text.text).build());
                        }
                        i::OutputContent::Refusal(refusal) => {
                            parts.push(g::Part::builder().text(refusal.refusal).build());
                            if completed {
                                reason = g::FinishReason::Safety;
                            }
                        }
                    }
                }
            }
            r::ResponseOutputItem::FunctionCall(call) => {
                if call.status == Some(i::ItemStatus::InProgress)
                    || completed && call.status == Some(i::ItemStatus::Incomplete)
                {
                    return Err(TransformError::invalid_result(
                        "function.status",
                        "nonterminal call",
                    ));
                }
                parts.push(super::identity::function(call, &input.model, &mut context)?);
            }
            r::ResponseOutputItem::Reasoning(reasoning) => {
                if context.parts.contains_key(&reasoning.id) {
                    parts.push(super::identity::reasoning(
                        reasoning,
                        &input.model,
                        &mut context,
                    )?);
                } else {
                    if reasoning.encrypted_content.flatten().is_some() {
                        report.omitted(
                            "reasoning.encrypted_content",
                            "Responses ciphertext cannot be reinterpreted as Gemini signature",
                        );
                    }
                    let text = reasoning
                        .content
                        .map(|v| v.into_iter().map(|v| v.text).collect::<Vec<_>>().join(""))
                        .unwrap_or_else(|| {
                            reasoning
                                .summary
                                .into_iter()
                                .map(|v| v.text)
                                .collect::<Vec<_>>()
                                .join("")
                        });
                    if !text.is_empty() {
                        parts.push(g::Part::builder().thought(true).text(text).build());
                    }
                }
            }
            r::ResponseOutputItem::ImageGenerationCall(image) => {
                if !completed && image.status == i::ImageGenerationStatus::Failed {
                    report.omitted("image.incomplete", "failed image result has no generated bytes; native incomplete reason is retained");
                    continue;
                }
                let max = image.result.as_ref().map_or(0, |v| v.len() as u64);
                parts.push(super::images::restore(
                    image,
                    &input.model,
                    &mut context,
                    max,
                )?);
            }
            r::ResponseOutputItem::FunctionCallOutput(_)
            | r::ResponseOutputItem::FileSearchCall(_)
            | r::ResponseOutputItem::WebSearchCall(_)
            | r::ResponseOutputItem::ComputerCall(_)
            | r::ResponseOutputItem::ComputerCallOutput(_)
            | r::ResponseOutputItem::Program(_)
            | r::ResponseOutputItem::ProgramOutput(_)
            | r::ResponseOutputItem::ToolSearchCall(_)
            | r::ResponseOutputItem::ToolSearchOutput(_)
            | r::ResponseOutputItem::AdditionalTools(_)
            | r::ResponseOutputItem::Compaction(_)
            | r::ResponseOutputItem::CodeInterpreterCall(_)
            | r::ResponseOutputItem::LocalShellCall(_)
            | r::ResponseOutputItem::LocalShellCallOutput(_)
            | r::ResponseOutputItem::ShellCall(_)
            | r::ResponseOutputItem::ShellCallOutput(_)
            | r::ResponseOutputItem::ApplyPatchCall(_)
            | r::ResponseOutputItem::ApplyPatchCallOutput(_)
            | r::ResponseOutputItem::McpCall(_)
            | r::ResponseOutputItem::McpListTools(_)
            | r::ResponseOutputItem::McpApprovalRequest(_)
            | r::ResponseOutputItem::McpApprovalResponse(_)
            | r::ResponseOutputItem::CustomToolCall(_)
            | r::ResponseOutputItem::CustomToolCallOutput(_) => {
                return Err(TransformError::unsupported(
                    "output",
                    "hosted/custom execution requires Gemini adapter",
                ));
            }
        }
    }
    if image_only
        && completed
        && !refused
        && !parts
            .iter()
            .any(|part| part.inline_data.is_some() || part.file_data.is_some())
    {
        return Err(TransformError::invalid_result(
            "image.output",
            "IMAGE-only request completed without an actual generated image",
        ));
    }
    let mut candidate = g::Candidate::builder()
        .content(g::Content::builder().role("model").parts(parts).build())
        .finish_reason(reason)
        .index(0)
        .build();
    if !logs.is_empty() {
        candidate.logprobs_result = Some(super::logs::to_gemini(logs)?);
    }
    if !citations.is_empty() {
        candidate.citation_metadata = Some(
            g::CitationMetadata::builder()
                .citation_sources(citations)
                .build(),
        );
    }
    let out = g::GenerateContentResponseBody::builder()
        .candidates(vec![candidate])
        .usage_metadata(usage)
        .model_version(input.model)
        .response_id(input.id)
        .build();
    Ok(Converted { value: out, report })
}
