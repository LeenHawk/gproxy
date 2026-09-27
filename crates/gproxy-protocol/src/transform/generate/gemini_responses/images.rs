//! Native generated-image payloads. MIME is verified from actual bytes.

use crate::{
    transform::{TransformError, images::decode_image},
    wire::{DeclaredFields, gemini as g, openai::responses::input as r},
};

pub(super) fn to_responses(
    blob: g::Blob,
    id: String,
    _max_bytes: u64,
) -> Result<r::ImageGenerationCall, TransformError> {
    let blob = blob.into_declared();

    Ok(r::ImageGenerationCall::builder(
        r::ImageGenerationCallType::ImageGenerationCall,
        id,
        Some(blob.data),
        r::ImageGenerationStatus::Completed,
    )
    .build())
}

pub(super) fn to_gemini(
    value: &r::ImageGenerationCall,
    max_bytes: u64,
) -> Result<g::Part, TransformError> {
    if value.status != r::ImageGenerationStatus::Completed {
        return Err(TransformError::invalid_result(
            "image.status",
            "in-progress or failed image generation is not a completed image",
        ));
    }
    let data = value
        .result
        .as_ref()
        .ok_or_else(|| TransformError::missing_metadata("image.result"))?;
    let image = decode_image(data, None, max_bytes)?;
    Ok(g::Part::builder()
        .inline_data(g::Blob::builder(image.metadata.mime().to_owned(), data.clone()).build())
        .build())
}

pub(super) fn requested_format(
    blob: &g::Blob,
    request: &crate::wire::openai::responses::GenerateContentRequestBody,
) -> Result<(), TransformError> {
    use crate::wire::openai::responses::tools::{ImageOutputFormat, Tool};
    for tool in request.tools.iter().flatten() {
        if let Tool::ImageGeneration(tool) = tool
            && let Some(format) = &tool.output_format
        {
            let mime = match format {
                ImageOutputFormat::Png => "image/png",
                ImageOutputFormat::Jpeg => "image/jpeg",
                ImageOutputFormat::Webp => "image/webp",
            };
            if blob.mime_type != mime {
                return Err(TransformError::invalid_result(
                    "image.mime",
                    "actual image format differs from requested image tool format",
                ));
            }
        }
    }
    Ok(())
}
