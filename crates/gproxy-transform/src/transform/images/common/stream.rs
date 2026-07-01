use crate::protocol::{gemini, openai};

use super::content::inline_image_part;
use super::response::gemini_candidate_to_openai_images;
use super::scalar::{i32_to_u32, u32_to_i32};
use super::usage::{gemini_usage_to_openai, openai_usage_to_gemini};

pub(in crate::transform::images) fn openai_generation_stream_to_gemini(
    input: openai::ImageGenerationStreamEvent,
) -> Option<gemini::GenerateContentResponse> {
    match input {
        openai::ImageGenerationStreamEvent::Known(
            openai::KnownImageGenerationStreamEvent::PartialImage {
                b64_json,
                partial_image_index,
                ..
            },
        ) => Some(gemini_image_stream_response(
            b64_json,
            partial_image_index,
            None,
            None,
        )),
        openai::ImageGenerationStreamEvent::Known(
            openai::KnownImageGenerationStreamEvent::Completed {
                b64_json, usage, ..
            },
        ) => Some(gemini_image_stream_response(
            b64_json,
            0,
            Some(gemini::FinishReasonKnown::Stop),
            usage.map(openai_usage_to_gemini),
        )),
        openai::ImageGenerationStreamEvent::Unknown(_) => None,
    }
}

pub(in crate::transform::images) fn openai_edit_stream_to_gemini(
    input: openai::ImageEditStreamEvent,
) -> Option<gemini::GenerateContentResponse> {
    match input {
        openai::ImageEditStreamEvent::Known(openai::KnownImageEditStreamEvent::PartialImage {
            b64_json,
            partial_image_index,
            ..
        }) => Some(gemini_image_stream_response(
            b64_json,
            partial_image_index,
            None,
            None,
        )),
        openai::ImageEditStreamEvent::Known(openai::KnownImageEditStreamEvent::Completed {
            b64_json,
            usage,
            ..
        }) => Some(gemini_image_stream_response(
            b64_json,
            0,
            Some(gemini::FinishReasonKnown::Stop),
            usage.map(openai_usage_to_gemini),
        )),
        openai::ImageEditStreamEvent::Unknown(_) => None,
    }
}

fn gemini_image_stream_response(
    b64_json: String,
    index: u32,
    finish_reason: Option<gemini::FinishReasonKnown>,
    usage_metadata: Option<gemini::UsageMetadata>,
) -> gemini::GenerateContentResponse {
    gemini::GenerateContentResponse {
        candidates: vec![gemini::Candidate {
            content: Some(gemini::Content {
                parts: vec![inline_image_part(b64_json, "image/png".to_owned())],
                role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)),
                extra: Default::default(),
            }),
            finish_reason: finish_reason.map(gemini::FinishReason::Known),
            safety_ratings: Vec::new(),
            citation_metadata: None,
            token_count: None,
            grounding_metadata: None,
            avg_logprobs: None,
            logprobs_result: None,
            url_context_metadata: None,
            index: Some(u32_to_i32(index)),
            finish_message: None,
            extra: Default::default(),
        }],
        prompt_feedback: None,
        usage_metadata,
        model_version: None,
        response_id: None,
        model_status: None,
        extra: Default::default(),
    }
}

pub(in crate::transform::images) fn gemini_to_openai_generation_stream(
    input: gemini::GenerateContentResponse,
) -> Option<openai::ImageGenerationStreamEvent> {
    let event = gemini_stream_parts(input)?;
    if event.completed {
        Some(openai::ImageGenerationStreamEvent::Known(
            openai::KnownImageGenerationStreamEvent::Completed {
                b64_json: event.b64_json,
                usage: event.usage,
                extra: Default::default(),
            },
        ))
    } else {
        Some(openai::ImageGenerationStreamEvent::Known(
            openai::KnownImageGenerationStreamEvent::PartialImage {
                b64_json: event.b64_json,
                partial_image_index: event.index,
                extra: Default::default(),
            },
        ))
    }
}

pub(in crate::transform::images) fn gemini_to_openai_edit_stream(
    input: gemini::GenerateContentResponse,
) -> Option<openai::ImageEditStreamEvent> {
    let event = gemini_stream_parts(input)?;
    if event.completed {
        Some(openai::ImageEditStreamEvent::Known(
            openai::KnownImageEditStreamEvent::Completed {
                b64_json: event.b64_json,
                usage: event.usage,
                extra: Default::default(),
            },
        ))
    } else {
        Some(openai::ImageEditStreamEvent::Known(
            openai::KnownImageEditStreamEvent::PartialImage {
                b64_json: event.b64_json,
                partial_image_index: event.index,
                extra: Default::default(),
            },
        ))
    }
}

struct OpenAiStreamImage {
    b64_json: String,
    index: u32,
    completed: bool,
    usage: Option<openai::ImageUsage>,
}

fn gemini_stream_parts(input: gemini::GenerateContentResponse) -> Option<OpenAiStreamImage> {
    let usage = input.usage_metadata.map(gemini_usage_to_openai);
    let mut completed = usage.is_some();

    for candidate in input.candidates {
        completed |= candidate.finish_reason.is_some();
        let index = candidate.index.map(i32_to_u32).unwrap_or_default();
        for image in gemini_candidate_to_openai_images(candidate) {
            if let Some(b64_json) = image.b64_json {
                return Some(OpenAiStreamImage {
                    b64_json,
                    index,
                    completed,
                    usage,
                });
            }
        }
    }

    None
}
