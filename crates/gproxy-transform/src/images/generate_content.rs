use bytes::Bytes;
use gproxy_protocol::openai::images as openai_images;
use gproxy_protocol::{gemini, openai};

use crate::TransformError;

pub(crate) fn openai_request(
    body: Bytes,
    model: &str,
    edit: bool,
) -> Result<Bytes, TransformError> {
    if !edit {
        let input: openai_images::CreateImageRequest = serde_json::from_slice(&body)?;
        return super::encode(&openai_create_request_typed(input, model));
    }
    let input: openai_images::EditImageRequest = serde_json::from_slice(&body)?;
    super::encode(&openai_edit_request_typed(input, model))
}

pub(crate) fn openai_create_request_typed(
    input: openai_images::CreateImageRequest,
    model: &str,
) -> gemini::GenerateContentRequest {
    build_gemini_request(
        input.prompt,
        Vec::new(),
        input.n,
        input.output_format,
        input.size.as_ref().and_then(wire_string),
        model,
    )
}

pub(crate) fn openai_edit_request_typed(
    input: openai_images::EditImageRequest,
    model: &str,
) -> gemini::GenerateContentRequest {
    build_gemini_request(
        input.prompt,
        input.images,
        input.n,
        input.output_format,
        input.size.as_ref().and_then(wire_string),
        model,
    )
}

pub(crate) fn gemini_request(
    body: Bytes,
    model: &str,
    edit: bool,
) -> Result<Bytes, TransformError> {
    let input: gemini::GenerateContentRequest = serde_json::from_slice(&body)?;
    match gemini_request_typed(input, model, edit) {
        OpenAiImageRequest::Create(request) => super::encode(&request),
        OpenAiImageRequest::Edit(request) => super::encode(&request),
    }
}

enum OpenAiImageRequest {
    Create(openai_images::CreateImageRequest),
    Edit(openai_images::EditImageRequest),
}

fn gemini_request_typed(
    input: gemini::GenerateContentRequest,
    model: &str,
    edit: bool,
) -> OpenAiImageRequest {
    let mut prompt = String::new();
    let mut images = Vec::new();
    for content in input.contents {
        for part in content.parts {
            match part.data {
                Some(gemini::PartData::Text { text, .. }) => prompt.push_str(&text),
                Some(gemini::PartData::InlineData { inline_data, .. }) => {
                    images.push(crate::wire!(openai_images::ImageReference {
                        file_id: None,
                        image_url: Some(format!(
                            "data:{};base64,{}",
                            inline_data.mime_type, inline_data.data
                        )),
                        rest: Default::default(),
                    }));
                }
                Some(gemini::PartData::FileData { file_data, .. }) => {
                    images.push(crate::wire!(openai_images::ImageReference {
                        file_id: None,
                        image_url: Some(file_data.file_uri),
                        rest: Default::default(),
                    }));
                }
                Some(_) | None => {}
            }
        }
    }
    if edit {
        return OpenAiImageRequest::Edit(crate::wire!(openai_images::EditImageRequest {
            images,
            prompt,
            background: None,
            input_fidelity: None,
            mask: None,
            model: Some(model.into()),
            moderation: None,
            n: candidate_count(input.generation_config.as_ref()),
            output_compression: None,
            output_format: None,
            partial_images: None,
            quality: None,
            size: None,
            stream: None,
            user: None,
            rest: Default::default(),
        }));
    }
    OpenAiImageRequest::Create(crate::wire!(openai_images::CreateImageRequest {
        prompt,
        background: None,
        model: Some(model.into()),
        moderation: None,
        n: candidate_count(input.generation_config.as_ref()),
        output_compression: None,
        output_format: None,
        partial_images: None,
        quality: None,
        response_format: None,
        size: None,
        stream: None,
        style: None,
        user: None,
        rest: Default::default(),
    }))
}

pub(crate) fn gemini_create_request_typed(
    input: gemini::GenerateContentRequest,
    model: &str,
) -> openai_images::CreateImageRequest {
    let OpenAiImageRequest::Create(request) = gemini_request_typed(input, model, false) else {
        unreachable!("create mode returns a create request")
    };
    request
}

pub(crate) fn gemini_edit_request_typed(
    input: gemini::GenerateContentRequest,
    model: &str,
) -> openai_images::EditImageRequest {
    let OpenAiImageRequest::Edit(request) = gemini_request_typed(input, model, true) else {
        unreachable!("edit mode returns an edit request")
    };
    request
}

pub(crate) fn gemini_response_to_openai(body: Bytes) -> Result<Bytes, TransformError> {
    let input: gemini::GenerateContentResponse = serde_json::from_slice(&body)?;
    super::encode(&gemini_response_to_openai_typed(input))
}

pub(crate) fn gemini_response_to_openai_typed(
    input: gemini::GenerateContentResponse,
) -> openai_images::ImagesResponse {
    let mut data = Vec::new();
    let mut text = Vec::new();
    for candidate in input.candidates {
        if let Some(content) = candidate.content {
            for part in content.parts {
                match part.data {
                    Some(gemini::PartData::InlineData { inline_data, .. }) => {
                        data.push(crate::wire!(openai_images::Image {
                            b64_json: Some(inline_data.data),
                            revised_prompt: None,
                            url: None,
                            rest: Default::default(),
                        }));
                    }
                    Some(gemini::PartData::FileData { file_data, .. }) => {
                        data.push(crate::wire!(openai_images::Image {
                            b64_json: None,
                            revised_prompt: None,
                            url: Some(file_data.file_uri),
                            rest: Default::default(),
                        }));
                    }
                    Some(gemini::PartData::Text { text: value, .. }) => text.push(value),
                    Some(_) | None => {}
                }
            }
        }
    }
    if let Some(prompt) = (!text.is_empty()).then(|| text.join("\n")) {
        for image in &mut data {
            image.revised_prompt = Some(prompt.clone());
        }
    }
    let usage = input.usage_metadata.map(|usage| {
        let input_tokens = nonnegative(usage.prompt_token_count);
        let output_tokens = nonnegative(usage.candidates_token_count);
        crate::wire!(openai_images::ImageUsage {
            input_tokens,
            input_tokens_details: openai_images::ImageTokenDetails {
                image_tokens: 0,
                text_tokens: input_tokens,
                rest: Default::default(),
            },
            output_tokens,
            total_tokens: nonnegative(usage.total_token_count),
            output_tokens_details: None,
            rest: Default::default(),
        })
    });
    crate::wire!(openai_images::ImagesResponse {
        created: 0,
        background: None,
        data: Some(data),
        output_format: None,
        quality: None,
        size: None,
        usage,
        rest: Default::default(),
    })
}

pub(crate) fn openai_response_to_gemini(body: Bytes) -> Result<Bytes, TransformError> {
    let input: openai_images::ImagesResponse = serde_json::from_slice(&body)?;
    super::encode(&openai_response_to_gemini_typed(input))
}

pub(crate) fn openai_response_to_gemini_typed(
    input: openai_images::ImagesResponse,
) -> gemini::GenerateContentResponse {
    let mut parts = Vec::new();
    for image in input.data.unwrap_or_default() {
        if let Some(data) = image.b64_json {
            parts.push(crate::wire!(gemini::Part {
                data: Some(gemini::PartData::InlineData {
                    inline_data: gemini::Blob {
                        mime_type: mime(input.output_format.as_ref()),
                        data,
                        rest: Default::default(),
                    },
                    rest: Default::default(),
                }),
                ..Default::default()
            }));
        } else if let Some(file_uri) = image.url {
            parts.push(crate::wire!(gemini::Part {
                data: Some(gemini::PartData::FileData {
                    file_data: gemini::FileData {
                        mime_type: Some(mime(input.output_format.as_ref())),
                        file_uri,
                        rest: Default::default(),
                    },
                    rest: Default::default(),
                }),
                ..Default::default()
            }));
        }
    }
    let candidate = (!parts.is_empty()).then(|| {
        crate::wire!(gemini::Candidate {
            content: Some(gemini::Content {
                parts,
                ..Default::default()
            }),
            ..Default::default()
        })
    });
    crate::wire!(gemini::GenerateContentResponse {
        candidates: candidate.into_iter().collect(),
        response_id: Some(input.created.to_string()),
        rest: Default::default(),
        ..Default::default()
    })
}

fn build_gemini_request(
    prompt: String,
    images: Vec<openai_images::ImageReference>,
    n: Option<u32>,
    output_format: Option<openai::ImageOutputFormat>,
    size: Option<String>,
    model: &str,
) -> gemini::GenerateContentRequest {
    let mut parts = vec![crate::wire!(gemini::Part {
        data: Some(gemini::PartData::Text {
            text: prompt,
            rest: Default::default(),
        }),
        ..Default::default()
    })];
    parts.extend(images.into_iter().filter_map(reference_part));
    let image_config = size.and_then(|size| serde_json::from_value(serde_json::json!(size)).ok());
    crate::wire!(gemini::GenerateContentRequest {
        model: Some(model.to_owned()),
        contents: vec![crate::wire!(gemini::Content {
            parts,
            role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::User)),
            rest: Default::default(),
        })],
        generation_config: Some(gemini::GenerationConfig {
            response_modalities: Some(vec![gemini::ResponseModality::Known(
                gemini::ResponseModalityKnown::Image,
            )]),
            candidate_count: n.map(|value| i32::try_from(value).unwrap_or(i32::MAX)),
            image_config: Some(gemini::ImageConfig {
                aspect_ratio: image_config,
                ..Default::default()
            }),
            response_mime_type: output_format.and_then(|format| {
                serde_json::from_value(serde_json::json!(mime(Some(&format)))).ok()
            }),
            ..Default::default()
        }),
        ..Default::default()
    })
}

fn reference_part(reference: openai_images::ImageReference) -> Option<gemini::Part> {
    let url = reference.image_url.or(reference.file_id)?;
    let data = if let Some(data) = url.strip_prefix("data:") {
        let (mime_type, data) = data.split_once(";base64,")?;
        gemini::PartData::InlineData {
            inline_data: crate::wire!(gemini::Blob {
                mime_type: mime_type.into(),
                data: data.into(),
                rest: Default::default(),
            }),
            rest: Default::default(),
        }
    } else {
        gemini::PartData::FileData {
            file_data: crate::wire!(gemini::FileData {
                mime_type: None,
                file_uri: url,
                rest: Default::default(),
            }),
            rest: Default::default(),
        }
    };
    Some(crate::wire!(gemini::Part {
        data: Some(data),
        ..Default::default()
    }))
}

fn candidate_count(config: Option<&gemini::GenerationConfig>) -> Option<u32> {
    config?
        .candidate_count
        .and_then(|value| u32::try_from(value).ok())
}

fn wire_string(value: &impl serde::Serialize) -> Option<String> {
    serde_json::to_value(value)
        .ok()?
        .as_str()
        .map(str::to_owned)
}

fn mime(format: Option<&openai::ImageOutputFormat>) -> String {
    format
        .and_then(wire_string)
        .map(|format| format!("image/{format}"))
        .unwrap_or_else(|| "image/png".into())
}

fn nonnegative(value: Option<i32>) -> u64 {
    value
        .and_then(|value| u64::try_from(value).ok())
        .unwrap_or_default()
}
