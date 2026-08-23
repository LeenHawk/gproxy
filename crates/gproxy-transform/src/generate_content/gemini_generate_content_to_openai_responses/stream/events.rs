use gproxy_protocol::{gemini, openai};

use crate::TransformError;

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

pub(super) fn ignored_or_unsupported(
    event: openai::ResponseStreamEventTypeKnown,
) -> Result<Vec<bytes::Bytes>, TransformError> {
    use openai::ResponseStreamEventTypeKnown as E;
    match event {
        E::ResponseContentPartAdded
        | E::ResponseContentPartDone
        | E::ResponseOutputTextDone
        | E::ResponseOutputTextAnnotationAdded
        | E::ResponseRefusalDone
        | E::ResponseReasoningSummaryPartAdded
        | E::ResponseReasoningSummaryPartDone
        | E::ResponseReasoningSummaryTextDone
        | E::ResponseReasoningTextDone => Ok(Vec::new()),
        E::ResponseAudioDelta
        | E::ResponseAudioDone
        | E::ResponseAudioTranscriptDelta
        | E::ResponseAudioTranscriptDone
        | E::ResponseImageGenerationCallCompleted
        | E::ResponseImageGenerationCallGenerating
        | E::ResponseImageGenerationCallInProgress
        | E::ResponseImageGenerationCallPartialImage
        | E::ResponseFileSearchCallInProgress
        | E::ResponseFileSearchCallSearching
        | E::ResponseFileSearchCallCompleted
        | E::ResponseWebSearchCallInProgress
        | E::ResponseWebSearchCallSearching
        | E::ResponseWebSearchCallCompleted
        | E::ResponseCodeInterpreterCallInProgress
        | E::ResponseCodeInterpreterCallInterpreting
        | E::ResponseCodeInterpreterCallCompleted
        | E::ResponseCodeInterpreterCallCodeDelta
        | E::ResponseCodeInterpreterCallCodeDone
        | E::ResponseMcpCallArgumentsDelta
        | E::ResponseMcpCallArgumentsDone
        | E::ResponseMcpCallInProgress
        | E::ResponseMcpCallCompleted
        | E::ResponseMcpCallFailed
        | E::ResponseMcpListToolsInProgress
        | E::ResponseMcpListToolsCompleted
        | E::ResponseMcpListToolsFailed => Err(TransformError::unsupported(
            "Responses stream",
            event.as_str(),
        )),
        E::ResponseCreated
        | E::ResponseInProgress
        | E::ResponseCompleted
        | E::ResponseFailed
        | E::ResponseIncomplete
        | E::ResponseQueued
        | E::ResponseOutputItemAdded
        | E::ResponseOutputItemDone
        | E::ResponseOutputTextDelta
        | E::ResponseFunctionCallArgumentsDelta
        | E::ResponseFunctionCallArgumentsDone
        | E::ResponseCustomToolCallInputDelta
        | E::ResponseCustomToolCallInputDone
        | E::ResponseRefusalDelta
        | E::ResponseReasoningSummaryTextDelta
        | E::ResponseReasoningTextDelta
        | E::Error => Err(TransformError::shape(
            "Responses stream",
            format!("{} reached the wrong dispatch branch", event.as_str()),
        )),
    }
}
