use crate::protocol::{gemini, openai};

use super::mime::{infer_mime_type_from_uri, is_image_mime, parse_data_url};

pub(in crate::transform::images) fn prompt_content(
    prompt: String,
    images: Vec<openai::ImageReference>,
) -> gemini::Content {
    let mut parts = vec![text_part(prompt)];
    parts.extend(images.into_iter().map(image_reference_part));

    gemini::Content {
        parts,
        role: Some(gemini::ContentRole::Known(gemini::ContentRoleKnown::User)),
        extra: Default::default(),
    }
}

pub(super) fn text_part(text: String) -> gemini::Part {
    gemini::Part {
        thought: None,
        thought_signature: None,
        part_metadata: None,
        media_resolution: None,
        data: Some(gemini::PartData::Text { text }),
        metadata: None,
        extra: Default::default(),
    }
}

pub(super) fn inline_image_part(data: String, mime_type: String) -> gemini::Part {
    gemini::Part {
        thought: None,
        thought_signature: None,
        part_metadata: None,
        media_resolution: None,
        data: Some(gemini::PartData::InlineData {
            inline_data: gemini::Blob {
                mime_type,
                data,
                extra: Default::default(),
            },
        }),
        metadata: None,
        extra: Default::default(),
    }
}

pub(super) fn file_image_part(file_uri: String, mime_type: Option<String>) -> gemini::Part {
    gemini::Part {
        thought: None,
        thought_signature: None,
        part_metadata: None,
        media_resolution: None,
        data: Some(gemini::PartData::FileData {
            file_data: gemini::FileData {
                mime_type,
                file_uri,
                extra: Default::default(),
            },
        }),
        metadata: None,
        extra: Default::default(),
    }
}

fn image_reference_part(reference: openai::ImageReference) -> gemini::Part {
    if let Some(url) = reference.image_url {
        if let Some((mime_type, data)) = parse_data_url(&url) {
            return inline_image_part(data.to_owned(), mime_type.to_owned());
        }
        let mime_type = infer_mime_type_from_uri(Some(&url));
        return file_image_part(url, mime_type);
    }

    file_image_part(reference.file_id.unwrap_or_default(), None)
}

pub(in crate::transform::images) fn gemini_request_prompt(
    request: &gemini::GenerateContentRequest,
) -> String {
    let mut values = Vec::new();

    if let Some(system) = request.system_instruction.as_ref() {
        push_text_content(&mut values, system);
    }

    for content in &request.contents {
        push_text_content(&mut values, content);
    }

    values.join("\n")
}

pub(super) fn push_text_content(values: &mut Vec<String>, content: &gemini::Content) {
    for part in &content.parts {
        if let Some(gemini::PartData::Text { text }) = part.data.as_ref()
            && !text.is_empty()
        {
            values.push(text.clone());
        }
    }
}

pub(in crate::transform::images) fn gemini_request_image_references(
    request: &gemini::GenerateContentRequest,
) -> Vec<openai::ImageReference> {
    request
        .contents
        .iter()
        .flat_map(|content| content.parts.iter())
        .filter_map(part_to_openai_image_reference)
        .collect()
}

fn part_to_openai_image_reference(part: &gemini::Part) -> Option<openai::ImageReference> {
    match part.data.as_ref()? {
        gemini::PartData::InlineData { inline_data } if is_image_mime(&inline_data.mime_type) => {
            Some(openai_image_url_reference(format!(
                "data:{};base64,{}",
                inline_data.mime_type, inline_data.data
            )))
        }
        gemini::PartData::FileData { file_data }
            if file_data
                .mime_type
                .as_deref()
                .map(is_image_mime)
                .unwrap_or(true) =>
        {
            Some(openai_image_url_reference(file_data.file_uri.clone()))
        }
        _ => None,
    }
}

fn openai_image_url_reference(image_url: String) -> openai::ImageReference {
    openai::ImageReference {
        file_id: None,
        image_url: Some(image_url),
        extra: Default::default(),
    }
}
