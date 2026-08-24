use gproxy_protocol::gemini;

pub(super) fn chunk(
    content: Option<gemini::Content>,
    finish_reason: Option<gemini::FinishReason>,
    usage_metadata: Option<gemini::UsageMetadata>,
    response_id: Option<String>,
    model_version: Option<String>,
) -> gemini::GenerateContentResponse {
    let candidate = (content.is_some() || finish_reason.is_some()).then(|| gemini::Candidate {
        content,
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
    });
    gemini::GenerateContentResponse {
        candidates: candidate.into_iter().collect(),
        prompt_feedback: None,
        usage_metadata,
        model_version,
        response_id,
        model_status: None,
        rest: Default::default(),
    }
}

pub(super) fn text(text: String, thought: bool) -> gemini::Content {
    gemini::Content {
        parts: vec![gemini::Part {
            thought: thought.then_some(true),
            data: Some(gemini::PartData::Text {
                text,
                rest: Default::default(),
            }),
            ..Default::default()
        }],
        role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)),
        rest: Default::default(),
    }
}
