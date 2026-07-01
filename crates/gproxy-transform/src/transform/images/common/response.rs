use crate::protocol::{gemini, openai};

use super::content::{file_image_part, inline_image_part, push_text_content, text_part};
use super::mime::{infer_mime_type_from_uri, is_image_mime, openai_output_format_mime};
use super::scalar::usize_to_i32;
use super::usage::{gemini_usage_to_openai, openai_usage_to_gemini};

pub(in crate::transform::images) fn openai_images_response_to_gemini(
    input: openai::ImagesResponse,
) -> gemini::GenerateContentResponse {
    let output_format = input.output_format;
    let usage = input.usage;
    let candidates = input
        .data
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .filter_map(|(index, image)| {
            openai_image_to_candidate(image, index, output_format.as_ref())
        })
        .collect();

    gemini::GenerateContentResponse {
        candidates,
        prompt_feedback: None,
        usage_metadata: usage.map(openai_usage_to_gemini),
        model_version: None,
        response_id: None,
        model_status: None,
        extra: Default::default(),
    }
}

fn openai_image_to_candidate(
    image: openai::Image,
    index: usize,
    output_format: Option<&openai::ImageOutputFormat>,
) -> Option<gemini::Candidate> {
    let mut parts = Vec::new();
    let output_mime_type = output_format.map(openai_output_format_mime);

    if let Some(text) = image.revised_prompt.filter(|text| !text.is_empty()) {
        parts.push(text_part(text));
    }
    if let Some(data) = image.b64_json {
        let mime_type = output_mime_type.unwrap_or("image/png").to_owned();
        parts.push(inline_image_part(data, mime_type));
    }
    if let Some(url) = image.url {
        let mime_type = output_mime_type
            .map(str::to_owned)
            .or_else(|| infer_mime_type_from_uri(Some(&url)));
        parts.push(file_image_part(url, mime_type));
    }

    if parts.is_empty() {
        return None;
    }

    Some(gemini::Candidate {
        content: Some(gemini::Content {
            parts,
            role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::Model)),
            extra: Default::default(),
        }),
        finish_reason: Some(gemini::FinishReason::Known(gemini::FinishReasonKnown::Stop)),
        safety_ratings: Vec::new(),
        citation_metadata: None,
        token_count: None,
        grounding_metadata: None,
        avg_logprobs: None,
        logprobs_result: None,
        url_context_metadata: None,
        index: Some(usize_to_i32(index)),
        finish_message: None,
        extra: Default::default(),
    })
}

pub(in crate::transform::images) fn gemini_response_to_openai_images(
    input: gemini::GenerateContentResponse,
) -> openai::ImagesResponse {
    let data = gemini_candidates_to_openai_images(input.candidates);
    let usage = input.usage_metadata.map(gemini_usage_to_openai);

    openai::ImagesResponse {
        created: 0,
        background: None,
        data: (!data.is_empty()).then_some(data),
        output_format: None,
        quality: None,
        size: None,
        usage,
        extra: Default::default(),
    }
}

pub(super) fn gemini_candidates_to_openai_images(
    candidates: Vec<gemini::Candidate>,
) -> Vec<openai::Image> {
    candidates
        .into_iter()
        .flat_map(gemini_candidate_to_openai_images)
        .collect()
}

pub(super) fn gemini_candidate_to_openai_images(
    candidate: gemini::Candidate,
) -> Vec<openai::Image> {
    let Some(content) = candidate.content else {
        return Vec::new();
    };
    let revised_prompt = content_revised_prompt(&content);

    content
        .parts
        .into_iter()
        .filter_map(|part| part_to_openai_image(part, revised_prompt.clone()))
        .collect()
}

fn content_revised_prompt(content: &gemini::Content) -> Option<String> {
    let mut values = Vec::new();
    push_text_content(&mut values, content);

    if values.is_empty() {
        None
    } else {
        Some(values.join("\n"))
    }
}

fn part_to_openai_image(
    part: gemini::Part,
    revised_prompt: Option<String>,
) -> Option<openai::Image> {
    match part.data? {
        gemini::PartData::InlineData { inline_data } if is_image_mime(&inline_data.mime_type) => {
            Some(openai::Image {
                b64_json: Some(inline_data.data),
                revised_prompt,
                url: None,
                extra: Default::default(),
            })
        }
        gemini::PartData::FileData { file_data }
            if file_data
                .mime_type
                .as_deref()
                .map(is_image_mime)
                .unwrap_or(true) =>
        {
            Some(openai::Image {
                b64_json: None,
                revised_prompt,
                url: Some(file_data.file_uri),
                extra: Default::default(),
            })
        }
        _ => None,
    }
}
