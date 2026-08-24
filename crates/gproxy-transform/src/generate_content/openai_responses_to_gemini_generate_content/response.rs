use bytes::Bytes;
use gproxy_protocol::{gemini, openai};

use crate::TransformError;
use crate::generate_content::gemini_generate_content_to_openai_responses::{
    config, content::ContentConverter, usage,
};

pub(crate) fn transform(body: Bytes) -> Result<Bytes, TransformError> {
    let input: gemini::GenerateContentResponse = serde_json::from_slice(&body)?;
    if input.candidates.len() > 1
        || input
            .candidates
            .iter()
            .any(|candidate| candidate.index.is_some_and(|index| index != 0))
    {
        return Err(TransformError::unsupported(
            "Gemini generateContent response",
            "multiple or nonzero-index candidates",
        ));
    }
    let id = input.response_id.ok_or_else(|| {
        TransformError::shape("Gemini generateContent response", "responseId is missing")
    })?;
    let (status, incomplete_details) = response_status(&input.candidates);
    let service_tier = input
        .usage_metadata
        .as_ref()
        .and_then(|usage| config::gemini_service_tier(usage.service_tier.clone()));
    let mut converter = ContentConverter::new();
    let mut output = Vec::new();
    for candidate in input.candidates {
        if let Some(content) = candidate.content {
            output.extend(converter.response(content)?);
        }
    }
    let output_text = output
        .iter()
        .filter_map(|item| match item {
            openai::ResponseItem::Message(openai::ResponseMessageItem::Output(message)) => Some(
                message
                    .content
                    .iter()
                    .filter_map(|part| match part {
                        openai::ResponseMessageOutputContentPart::OutputText(part) => {
                            Some(part.text.as_str())
                        }
                        openai::ResponseMessageOutputContentPart::Refusal(part) => {
                            Some(part.refusal.as_str())
                        }
                        openai::ResponseMessageOutputContentPart::Unknown(_) => None,
                    })
                    .collect::<String>(),
            ),
            openai::ResponseItem::Message(
                openai::ResponseMessageItem::Input(_)
                | openai::ResponseMessageItem::EasyInput(_)
                | openai::ResponseMessageItem::Unknown(_),
            )
            | openai::ResponseItem::Typed(_)
            | openai::ResponseItem::Unknown(_) => None,
        })
        .collect::<String>();
    let output = openai::ResponseObject {
        id,
        created_at: None,
        background: None,
        completed_at: None,
        conversation: None,
        error: None,
        incomplete_details,
        instructions: None,
        max_output_tokens: None,
        max_tool_calls: None,
        metadata: None,
        model: input.model_version.map(Into::into),
        moderation: None,
        multi_agent: None,
        object: openai::ResponseObjectType::Response,
        output,
        output_text: (!output_text.is_empty()).then_some(output_text),
        parallel_tool_calls: None,
        prompt: None,
        prompt_cache_key: None,
        prompt_cache_options: None,
        prompt_cache_retention: None,
        previous_response_id: None,
        reasoning: None,
        safety_identifier: None,
        service_tier,
        status,
        store: None,
        temperature: None,
        text: None,
        tool_choice: None,
        tools: None,
        top_logprobs: None,
        top_p: None,
        truncation: None,
        usage: input.usage_metadata.map(usage::to_responses).transpose()?,
        user: None,
        rest: input.rest,
    };
    Ok(Bytes::from(serde_json::to_vec(&output)?))
}

pub(in crate::generate_content) fn response_status(
    candidates: &[gemini::Candidate],
) -> (
    Option<openai::ResponseStatus>,
    Option<openai::IncompleteDetails>,
) {
    let mut status = None;
    for reason in candidates
        .iter()
        .filter_map(|value| value.finish_reason.as_ref())
    {
        match reason {
            gemini::FinishReason::Known(gemini::FinishReasonKnown::MaxTokens) => {
                return incomplete(openai::IncompleteReason::MaxOutputTokens);
            }
            gemini::FinishReason::Known(
                gemini::FinishReasonKnown::Safety
                | gemini::FinishReasonKnown::Recitation
                | gemini::FinishReasonKnown::Blocklist
                | gemini::FinishReasonKnown::ProhibitedContent
                | gemini::FinishReasonKnown::Spii
                | gemini::FinishReasonKnown::ImageSafety
                | gemini::FinishReasonKnown::ImageProhibitedContent,
            ) => return incomplete(openai::IncompleteReason::ContentFilter),
            gemini::FinishReason::Known(gemini::FinishReasonKnown::Stop) => {
                status = Some(openai::ResponseStatus::Completed);
            }
            gemini::FinishReason::Known(gemini::FinishReasonKnown::FinishReasonUnspecified)
            | gemini::FinishReason::Unknown(_) => {}
            gemini::FinishReason::Known(
                gemini::FinishReasonKnown::Language | gemini::FinishReasonKnown::ImageRecitation,
            ) => return incomplete(openai::IncompleteReason::ContentFilter),
            gemini::FinishReason::Known(
                gemini::FinishReasonKnown::Other
                | gemini::FinishReasonKnown::MalformedFunctionCall
                | gemini::FinishReasonKnown::ImageOther
                | gemini::FinishReasonKnown::NoImage
                | gemini::FinishReasonKnown::UnexpectedToolCall
                | gemini::FinishReasonKnown::TooManyToolCalls
                | gemini::FinishReasonKnown::MissingThoughtSignature
                | gemini::FinishReasonKnown::MalformedResponse,
            ) => status = Some(openai::ResponseStatus::Failed),
            _ => {}
        }
    }
    (status, None)
}

fn incomplete(
    reason: openai::IncompleteReason,
) -> (
    Option<openai::ResponseStatus>,
    Option<openai::IncompleteDetails>,
) {
    (
        Some(openai::ResponseStatus::Incomplete),
        Some(openai::IncompleteDetails {
            reason: Some(reason),
            rest: Default::default(),
        }),
    )
}
