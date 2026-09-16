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

pub(super) fn restore(
    value: r::ImageGenerationCall,
    context: &mut super::identity::GeminiReplayContext,
    max_bytes: u64,
) -> Result<g::Part, TransformError> {
    let decoded = to_gemini(&value, max_bytes)?;
    if let Some(proof) = context.image_files.remove(&value.id) {
        let part = (super::identity::RestoredGeminiPart {
            state: proof.state,
            part: proof.part,
        })
        .part
        .into_declared();
        if part.file_data.is_none()
            || part.inline_data.is_some()
            || part.thought == Some(true)
            || part.text.is_some()
            || part.function_call.is_some()
            || part.function_response.is_some()
        {
            return Err(TransformError::shape(
                "image.replay",
                "signed file image must restore the exact original fileData part",
            ));
        }

        if decoded.inline_data.as_ref() != Some(&proof.materialized.clone().into_declared()) {
            return Err(TransformError::shape(
                "image.replay",
                "returned image differs from scoped materialization proof",
            ));
        }
        if part
            .file_data
            .as_ref()
            .and_then(|v| v.mime_type.as_deref())
            .is_some_and(|v| v != proof.materialized.mime_type)
        {
            return Err(TransformError::shape(
                "image.replay",
                "original image MIME differs from scoped bytes",
            ));
        }
        return Ok(part);
    }
    if !context.parts.contains_key(&value.id) {
        return Ok(decoded);
    }
    let native = context.parts.remove(&value.id).expect("known image replay");
    let part = (native).part.into_declared();

    if part.inline_data != decoded.inline_data {
        return Err(TransformError::shape(
            "image.replay",
            "modified image cannot reuse the original Gemini signature",
        ));
    }
    Ok(part)
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
