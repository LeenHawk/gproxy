use crate::{
    transform::{
        Converted, Report, TransformError,
        identity::{IdentityFlow, IdentityRole, SourceIdentity, TargetIdPolicy},
    },
    wire::{DeclaredFields, gemini as g, openai::chat as c},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct GeminiChatResponseSupplement {
    pub created_unix_seconds: Option<i64>,
}

pub fn gemini_to_openai_response(
    input: g::GenerateContentResponseBody,
    target_model: impl Into<String>,
    supplement: &GeminiChatResponseSupplement,
    flow: &mut IdentityFlow,
    policy: &TargetIdPolicy,
) -> Result<Converted<c::GenerateContentResponseBody>, TransformError> {
    if policy.dialect != crate::Dialect::OpenAiChat {
        return Err(TransformError::shape(
            "identity.policy",
            "expected Chat target policy",
        ));
    }
    let input = input.into_declared();
    let created = supplement
        .created_unix_seconds
        .ok_or_else(|| TransformError::missing_metadata("response.created"))?;
    if created < 0 {
        return Err(TransformError::invalid_result(
            "created",
            "negative timestamp",
        ));
    }
    let model = input.model_version.unwrap_or_else(|| target_model.into());
    if model.is_empty() {
        return Err(TransformError::missing_metadata("response.model"));
    }
    let mut ids = flow.clone();
    let mut report = Report::default();
    let usage = input
        .usage_metadata
        .as_ref()
        .map(|usage| super::usage::to_chat(usage, &mut report))
        .map(crate::transform::optional)
        .transpose()?
        .flatten();
    let candidates = input
        .candidates
        .filter(|candidates| !candidates.is_empty())
        .ok_or_else(|| {
            TransformError::invalid_result(
                "candidates",
                "no generated candidates; prompt block requires error handling",
            )
        })?;
    let mut bindings = super::content::Calls::default();
    let mut choices = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (ordinal, candidate) in candidates.into_iter().enumerate() {
        let index = candidate.index.unwrap_or(ordinal as i64);
        if index < 0 || !seen.insert(index) {
            return Err(TransformError::invalid_result(
                "candidates.index",
                "negative or duplicate candidate index",
            ));
        }
        let mut content = candidate
            .content
            .unwrap_or_else(|| g::Content::builder().role("model").build());
        if content.role.is_none() {
            content.role = Some("model".into());
        }
        let mapped = super::content::gemini_content_to_chat(
            content,
            &mut report,
            &mut ids,
            policy,
            &mut bindings,
        )?;
        let mut text = String::new();
        let mut has_text = false;
        let mut tool_calls = Vec::new();
        for message in mapped {
            match message {
                c::ChatMessage::Assistant(message) => {
                    if let Some(Some(content)) = message.content {
                        match content {
                            c::AssistantContent::Text(value) => {
                                text.push_str(&value);
                                has_text = true;
                            }
                            c::AssistantContent::Parts(parts) => {
                                for part in parts {
                                    match part {
                                        c::AssistantContentPart::Text(part) => {
                                            text.push_str(&part.text);
                                            has_text = true;
                                        }
                                        c::AssistantContentPart::Refusal(_) => {
                                            return Err(TransformError::invalid_result(
                                                "content",
                                                "unexpected internal refusal",
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    tool_calls.extend(message.tool_calls.unwrap_or_default());
                }
                c::ChatMessage::User(_)
                | c::ChatMessage::System(_)
                | c::ChatMessage::Developer(_)
                | c::ChatMessage::Tool(_)
                | c::ChatMessage::Function(_) => {
                    return Err(TransformError::invalid_result(
                        "candidate.content.role",
                        "candidate must have model role",
                    ));
                }
            }
        }
        let finish = match candidate
            .finish_reason
            .ok_or_else(|| TransformError::missing_metadata("candidate.finish_reason"))?
        {
            g::FinishReason::Stop => {
                if tool_calls.is_empty() {
                    c::FinishReason::Stop
                } else {
                    c::FinishReason::ToolCalls
                }
            }
            g::FinishReason::MaxTokens => c::FinishReason::Length,
            g::FinishReason::Safety
            | g::FinishReason::Recitation
            | g::FinishReason::Language
            | g::FinishReason::Blocklist
            | g::FinishReason::ProhibitedContent
            | g::FinishReason::Spii
            | g::FinishReason::ImageSafety
            | g::FinishReason::ImageProhibitedContent
            | g::FinishReason::ImageRecitation => c::FinishReason::ContentFilter,
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
                    "candidate.finish_reason",
                    "Gemini generation did not finish with a representable successful reason",
                ));
            }
        };
        for (present, field) in [
            (candidate.safety_ratings.is_some(), "safety_ratings"),
            (candidate.citation_metadata.is_some(), "citation_metadata"),
            (
                candidate.grounding_attributions.is_some(),
                "grounding_attributions",
            ),
            (candidate.grounding_metadata.is_some(), "grounding_metadata"),
            (candidate.avg_logprobs.is_some(), "avg_logprobs"),
            (
                candidate.url_context_metadata.is_some(),
                "url_context_metadata",
            ),
            (candidate.token_count.is_some(), "token_count"),
            (candidate.finish_message.is_some(), "finish_message"),
        ] {
            if present {
                report.omitted(
                    format!("candidate.{field}"),
                    "Gemini detail requires a dedicated target metadata adapter",
                );
            }
        }
        let mut message =
            c::ResponseMessage::builder(has_text.then_some(text), None, c::ResponseRole::Assistant)
                .build();
        message.tool_calls = (!tool_calls.is_empty()).then_some(tool_calls);
        let logs = candidate
            .logprobs_result
            .map(|logs| super::logs::to_chat(logs, &mut report))
            .map(crate::transform::optional)
            .transpose()?
            .flatten();
        choices.push(c::Choice::builder(finish, index, logs, message).build());
    }
    let id = ids
        .resolve_or_allocate(
            IdentityRole::Response,
            SourceIdentity::new(crate::Dialect::Gemini, input.response_id, 0),
            policy,
        )
        .map_err(|error| TransformError::shape("response.identity", error.to_string()))?
        .emitted_id;
    let mut out = c::GenerateContentResponseBody::builder(
        id,
        choices,
        created,
        model,
        c::CompletionObject::ChatCompletion,
    )
    .build();
    out.usage = usage;
    if input.prompt_feedback.is_some() {
        report.omitted("prompt_feedback", "Chat has no prompt-feedback metadata");
    }
    if input.model_status.is_some() {
        report.omitted("model_status", "Chat has no model-status field");
    }
    *flow = ids;
    Ok(Converted { value: out, report })
}

pub fn openai_to_gemini_response(
    input: &c::GenerateContentResponseBody,
) -> Result<Converted<g::GenerateContentResponseBody>, TransformError> {
    let mut report = Report::default();
    let usage = input
        .usage
        .as_ref()
        .map(|usage| super::usage::to_gemini(usage, &mut report))
        .map(crate::transform::optional)
        .transpose()?
        .flatten();
    let mut candidates = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    if input.choices.is_empty() {
        return Err(TransformError::invalid_result("choices", "no choices"));
    }
    for choice in &input.choices {
        if choice.index < 0 || !seen.insert(choice.index) {
            return Err(TransformError::invalid_result(
                "choices.index",
                "negative or duplicate candidate index",
            ));
        }
        if choice
            .message
            .audio
            .as_ref()
            .and_then(Option::as_ref)
            .is_some()
        {
            return Err(TransformError::missing_metadata(
                "audio/legacy response requires adapter facts",
            ));
        }
        let mut parts = Vec::new();
        if let Some(text) = &choice.message.content {
            parts.push(g::Part::builder().text(text.clone()).build());
        }
        if let Some(refusal) = &choice.message.refusal {
            parts.push(g::Part::builder().text(refusal.clone()).build());
            report.changed(
                "refusal",
                "Gemini refusal represented by text and safety finish reason",
            );
        }
        if let Some(call) = &choice.message.function_call {
            if choice
                .message
                .tool_calls
                .as_ref()
                .is_some_and(|calls| !calls.is_empty())
            {
                return Err(TransformError::invalid_result(
                    "function_call",
                    "legacy/current calls conflict",
                ));
            }
            let args: serde_json::Map<String, serde_json::Value> =
                serde_json::from_str(&call.arguments).map_err(|error| {
                    TransformError::invalid_result("function_call.arguments", error.to_string())
                })?;
            parts.push(
                g::Part::builder()
                    .function_call(
                        g::FunctionCall::builder(call.name.clone())
                            .args(args)
                            .build(),
                    )
                    .build(),
            );
        }
        for call in choice.message.tool_calls.iter().flatten() {
            match call {
                c::MessageToolCall::Function(call) => {
                    if call.id.is_empty() {
                        return Err(TransformError::invalid_result(
                            "tool_call.id",
                            "empty tool identity",
                        ));
                    }
                    let args: serde_json::Map<String, serde_json::Value> =
                        serde_json::from_str(&call.function.arguments).map_err(|error| {
                            TransformError::invalid_result(
                                "tool.arguments",
                                format!("object JSON required: {error}"),
                            )
                        })?;
                    parts.push(
                        g::Part::builder()
                            .function_call(
                                g::FunctionCall::builder(call.function.name.clone())
                                    .id(call.id.clone())
                                    .args(args)
                                    .build(),
                            )
                            .build(),
                    );
                }
                c::MessageToolCall::Custom(_) => {
                    continue;
                }
            }
        }
        let mut candidate = g::Candidate::builder().build();
        candidate.content = Some(g::Content::builder().role("model").parts(parts).build());
        candidate.index = Some(choice.index);
        candidate.finish_reason = Some(match choice.finish_reason {
            c::FinishReason::Stop | c::FinishReason::ToolCalls | c::FinishReason::FunctionCall => {
                g::FinishReason::Stop
            }
            c::FinishReason::Length => g::FinishReason::MaxTokens,
            c::FinishReason::ContentFilter => g::FinishReason::Safety,
        });
        candidate.logprobs_result = choice
            .logprobs
            .as_ref()
            .map(|logs| super::logs::to_gemini(logs, &mut report))
            .map(crate::transform::optional)
            .transpose()?
            .flatten();
        if let Some(annotations) = &choice.message.annotations {
            let mut sources = Vec::new();
            for annotation in annotations {
                let citation = &annotation.url_citation;
                if citation.start_index < 0 || citation.end_index < citation.start_index {
                    return Err(TransformError::invalid_result(
                        "annotations",
                        "invalid citation range",
                    ));
                }
                sources.push(
                    g::CitationSource::builder()
                        .start_index(citation.start_index)
                        .end_index(citation.end_index)
                        .uri(citation.url.clone())
                        .build(),
                );
            }
            candidate.citation_metadata = Some(
                g::CitationMetadata::builder()
                    .citation_sources(sources)
                    .build(),
            );
            report.omitted(
                "annotations.title",
                "Gemini citation sources have no title field",
            );
        }
        candidates.push(candidate);
    }
    let mut out = g::GenerateContentResponseBody::builder().build();
    out.candidates = Some(candidates);
    out.usage_metadata = usage;
    out.response_id = Some(input.id.clone());
    out.model_version = Some(input.model.clone());
    Ok(Converted { value: out, report })
}
