use crate::{
    transform::TransformError,
    wire::{gemini as g, openai::responses::input as r},
};
use base64::{Engine as _, engine::general_purpose::STANDARD};

pub(super) fn inline(value: g::Blob) -> Result<r::InputContent, TransformError> {
    STANDARD
        .decode(&value.data)
        .map_err(|e| TransformError::shape("inline_data", e.to_string()))?;
    let uri = format!("data:{};base64,{}", value.mime_type, value.data);
    if value.mime_type.starts_with("image/") {
        let mut out = r::ResponseInputImage::builder(
            r::ResponseInputImageType::ResponseInputImage,
            r::ImageDetail::Auto,
        )
        .build();
        out.image_url = Some(Some(uri));
        Ok(r::InputContent::Image(out))
    } else if value.mime_type == "application/pdf" || value.mime_type == "text/plain" {
        Ok(r::InputContent::File(
            r::ResponseInputFile::builder(r::ResponseInputFileType::ResponseInputFile)
                .file_data(uri)
                .build(),
        ))
    } else {
        Err(TransformError::unsupported(
            "inline_data.mime",
            "Responses input has no direct audio/video media part",
        ))
    }
}

pub(super) fn file(value: g::FileData) -> Result<r::InputContent, TransformError> {
    if !value.file_uri.starts_with("https://") && !value.file_uri.starts_with("http://") {
        return Err(TransformError::missing_metadata(
            "file_uri requires resource URL mapping",
        ));
    }
    if value
        .mime_type
        .as_deref()
        .is_some_and(|v| v.starts_with("image/"))
    {
        let mut out = r::ResponseInputImage::builder(
            r::ResponseInputImageType::ResponseInputImage,
            r::ImageDetail::Auto,
        )
        .build();
        out.image_url = Some(Some(value.file_uri));
        Ok(r::InputContent::Image(out))
    } else {
        Ok(r::InputContent::File(
            r::ResponseInputFile::builder(r::ResponseInputFileType::ResponseInputFile)
                .file_url(value.file_uri)
                .build(),
        ))
    }
}

pub(super) fn to_gemini(value: r::InputContent) -> Result<g::Part, TransformError> {
    match value {
        r::InputContent::Text(value) => Ok(g::Part::builder().text(value.text).build()),
        r::InputContent::Image(value) => {
            if value.file_id.flatten().is_some() {
                return Err(TransformError::missing_metadata(
                    "input_image file needs Gemini resource facts",
                ));
            }
            if value.detail != r::ImageDetail::Auto {
                return Err(TransformError::unsupported(
                    "image.detail",
                    "Gemini resolution requires explicit adapter",
                ));
            }
            let url = value
                .image_url
                .flatten()
                .ok_or_else(|| TransformError::missing_metadata("image_url"))?;
            Ok(g::Part::builder().inline_data(blob(&url)?).build())
        }
        r::InputContent::File(value) => {
            if value.file_id.flatten().is_some() || value.file_url.is_some() {
                return Err(TransformError::missing_metadata(
                    "input_file needs bytes and exact Gemini MIME",
                ));
            }
            let data = value
                .file_data
                .ok_or_else(|| TransformError::missing_metadata("file_data"))?;
            Ok(g::Part::builder().inline_data(blob(&data)?).build())
        }
    }
}

fn blob(value: &str) -> Result<g::Blob, TransformError> {
    let (mime, bytes) = value
        .strip_prefix("data:")
        .and_then(|v| v.split_once(";base64,"))
        .ok_or_else(|| TransformError::missing_metadata("media data URI with exact MIME"))?;
    STANDARD
        .decode(bytes)
        .map_err(|e| TransformError::shape("media.base64", e.to_string()))?;
    Ok(g::Blob::builder(mime.into(), bytes.into()).build())
}

pub(super) fn output(input: g::FunctionResponse) -> Result<r::FunctionOutput, TransformError> {
    let text = serde_json::to_string(&input.response)?;
    let Some(parts) = input.parts else {
        return Ok(r::FunctionOutput::Text(text));
    };
    let mut content = vec![r::FunctionOutputContent::Text(
        r::FunctionOutputText::builder(r::ResponseInputTextType::ResponseInputText, text).build(),
    )];
    for part in parts {
        let blob = part
            .inline_data
            .ok_or_else(|| TransformError::shape("function_response.parts", "empty part"))?;
        content.push(match inline(blob)? {
            r::InputContent::Image(value) => {
                let mut image =
                    r::FunctionOutputImage::builder(r::ResponseInputImageType::ResponseInputImage)
                        .build();
                image.image_url = value.image_url;
                image.detail = Some(Some(value.detail));
                r::FunctionOutputContent::Image(image)
            }
            r::InputContent::File(value) => {
                let mut file =
                    r::FunctionOutputFile::builder(r::ResponseInputFileType::ResponseInputFile)
                        .build();
                file.file_data = value.file_data.map(Some);
                file.filename = value.filename.map(Some);
                r::FunctionOutputContent::File(file)
            }
            r::InputContent::Text(_) => unreachable!("inline media"),
        });
    }
    Ok(r::FunctionOutput::Content(content))
}

pub(super) fn output_to_gemini(
    output: r::FunctionOutput,
    name: String,
    id: String,
) -> Result<g::Part, TransformError> {
    let mut text = String::new();
    let mut parts = Vec::new();
    match output {
        r::FunctionOutput::Text(value) => text = value,
        r::FunctionOutput::Content(content) => {
            for value in content {
                let input = match value {
                    r::FunctionOutputContent::Text(value) => {
                        text.push_str(&value.text);
                        continue;
                    }
                    r::FunctionOutputContent::Image(value) => {
                        let mut input = r::ResponseInputImage::builder(
                            r::ResponseInputImageType::ResponseInputImage,
                            value.detail.flatten().unwrap_or(r::ImageDetail::Auto),
                        )
                        .build();
                        input.file_id = value.file_id;
                        input.image_url = value.image_url;
                        r::InputContent::Image(input)
                    }
                    r::FunctionOutputContent::File(value) => {
                        let mut input = r::ResponseInputFile::builder(
                            r::ResponseInputFileType::ResponseInputFile,
                        )
                        .build();
                        input.file_data = value.file_data.flatten();
                        input.file_id = value.file_id;
                        input.file_url = value.file_url.flatten();
                        r::InputContent::File(input)
                    }
                };
                let part = to_gemini(input)?;
                parts.push(
                    g::FunctionResponsePart::builder()
                        .inline_data(part.inline_data.ok_or_else(|| {
                            TransformError::missing_metadata("tool output inline bytes")
                        })?)
                        .build(),
                );
            }
        }
    }
    let mut response = serde_json::Map::new();
    response.insert("output".into(), text.into());
    let mut function = g::FunctionResponse::builder(name, response).id(id).build();
    if !parts.is_empty() {
        function.parts = Some(parts);
    }
    Ok(g::Part::builder().function_response(function).build())
}
