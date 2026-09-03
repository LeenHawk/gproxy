use gproxy_protocol::{claude, gemini};

pub(super) fn metadata(
    id: String,
    model: String,
    usage: Option<gemini::UsageMetadata>,
) -> gemini::GenerateContentResponse {
    crate::wire!(gemini::GenerateContentResponse {
        candidates: Vec::new(),
        prompt_feedback: None,
        usage_metadata: usage,
        model_version: Some(model),
        response_id: Some(id),
        model_status: None,
        rest: Default::default(),
    })
}

pub(super) fn candidate(
    part: Option<gemini::Part>,
    finish_reason: Option<gemini::FinishReason>,
    usage: Option<gemini::UsageMetadata>,
) -> gemini::GenerateContentResponse {
    let candidates = if part.is_some() || finish_reason.is_some() {
        vec![crate::wire!(gemini::Candidate {
            content: part.map(|part| super::super::content::model_content(vec![part])),
            finish_reason,
            safety_ratings: Vec::new(),
            citation_metadata: None,
            token_count: None,
            grounding_metadata: None,
            avg_logprobs: None,
            logprobs_result: None,
            url_context_metadata: None,
            index: Some(0),
            finish_message: None,
            rest: Default::default(),
        })]
    } else {
        Vec::new()
    };
    crate::wire!(gemini::GenerateContentResponse {
        candidates,
        prompt_feedback: None,
        usage_metadata: usage,
        model_version: None,
        response_id: None,
        model_status: None,
        rest: Default::default(),
    })
}

pub(super) fn text(text: String, thought: bool) -> gemini::Part {
    crate::wire!(gemini::Part {
        thought: thought.then_some(true),
        thought_signature: None,
        part_metadata: None,
        media_resolution: None,
        data: Some(gemini::PartData::Text {
            text,
            rest: Default::default(),
        }),
        metadata: None,
        rest: Default::default(),
    })
}

pub(super) fn signature(signature: String) -> gemini::Part {
    crate::wire!(gemini::Part {
        thought: Some(true),
        thought_signature: Some(signature),
        part_metadata: None,
        media_resolution: None,
        data: None,
        metadata: None,
        rest: Default::default(),
    })
}

pub(super) fn message_delta(
    delta: claude::MessageDelta,
    usage: Option<claude::Usage>,
) -> Result<gemini::GenerateContentResponse, crate::TransformError> {
    let usage = usage.map(super::super::usage::convert).transpose()?;
    Ok(candidate(
        None,
        delta
            .stop_reason
            .map(super::super::response::stop_reason)
            .transpose()?,
        usage,
    ))
}
