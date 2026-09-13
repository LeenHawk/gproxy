use crate::transform::{Report, TransformError};
use crate::wire::{gemini, openai};
use base64::{Engine as _, engine::general_purpose::STANDARD};

pub(crate) fn converted<T>(value: T, report: Report) -> crate::transform::Converted<T> {
    crate::transform::Converted { value, report }
}

pub(crate) fn model_resource(value: &str) -> Result<String, TransformError> {
    if value.is_empty() || value == "models/" {
        return Err(TransformError::missing_metadata("target_model"));
    }
    if value.starts_with("models/") {
        Ok(value.to_owned())
    } else {
        Ok(format!("models/{value}"))
    }
}

pub(crate) fn target_model_name(value: &str) -> Result<String, TransformError> {
    if value.is_empty() {
        Err(TransformError::missing_metadata("target_model"))
    } else {
        Ok(value.to_owned())
    }
}

pub(crate) fn source_model_resource(value: &str) -> Result<String, TransformError> {
    if value.starts_with("models/") && value.len() > "models/".len() {
        Ok(value.to_owned())
    } else {
        Err(TransformError::shape(
            "embedding.model",
            "Gemini batch model must be a complete models/{name} resource",
        ))
    }
}

pub(crate) fn text_content(value: String) -> gemini::content::Content {
    gemini::content::Content {
        parts: Some(vec![gemini::content::Part {
            thought: None,
            thought_signature: None,
            part_metadata: None,
            media_resolution: None,
            text: Some(value),
            inline_data: None,
            function_call: None,
            function_response: None,
            file_data: None,
            executable_code: None,
            code_execution_result: None,
            tool_call: None,
            tool_response: None,
            video_metadata: None,
            rest: Default::default(),
        }]),
        role: None,
        rest: Default::default(),
    }
}

pub(crate) fn content_text(content: gemini::content::Content) -> Result<String, TransformError> {
    if content.role.is_some() {
        return Err(TransformError::unsupported(
            "embedding.content.role",
            "role-annotated embedding content has no OpenAI equivalent",
        ));
    }
    let parts = content
        .parts
        .ok_or_else(|| TransformError::shape("embedding.content", "content has no parts"))?;
    if parts.is_empty() {
        return Err(TransformError::shape(
            "embedding.content",
            "content has no parts",
        ));
    }
    let mut text = String::new();
    for part in parts {
        if part.text.is_none()
            || part.thought.is_some()
            || part.thought_signature.is_some()
            || part.inline_data.is_some()
            || part.function_call.is_some()
            || part.function_response.is_some()
            || part.file_data.is_some()
            || part.executable_code.is_some()
            || part.code_execution_result.is_some()
            || part.tool_call.is_some()
            || part.tool_response.is_some()
            || part.video_metadata.is_some()
            || part.part_metadata.is_some()
            || part.media_resolution.is_some()
        {
            return Err(TransformError::unsupported(
                "embedding.content",
                "non-text or annotated parts require embedding capability",
            ));
        }
        text.push_str(part.text.as_deref().unwrap_or_default());
    }
    if text.is_empty() {
        return Err(TransformError::shape("embedding.content", "text is empty"));
    }
    Ok(text)
}

pub(crate) fn dimensions(value: Option<i64>) -> Result<Option<i64>, TransformError> {
    value
        .map(|dimension| {
            if dimension <= 0 {
                Err(TransformError::shape(
                    "embedding.dimensions",
                    "dimension must be positive",
                ))
            } else {
                Ok(dimension)
            }
        })
        .transpose()
}

pub(crate) fn gemini_dimensions(
    body: &gemini::embeddings::EmbedContentRequestBody,
) -> Result<Option<i64>, TransformError> {
    let top = dimensions(body.output_dimensionality)?;
    let config = body
        .embed_content_config
        .as_ref()
        .and_then(|config| config.output_dimensionality);
    let config = dimensions(config)?;
    match (top, config) {
        (Some(top), Some(config)) if top != config => Err(TransformError::new(
            crate::transform::TransformErrorKind::Conflict,
            "embedding.output_dimensionality",
            "top-level and config dimensions disagree",
        )),
        (Some(value), _) | (_, Some(value)) => Ok(Some(value)),
        (None, None) => Ok(None),
    }
}

pub(crate) fn decode_base64(value: &str) -> Result<Vec<serde_json::Number>, TransformError> {
    let bytes = STANDARD.decode(value).map_err(|error| {
        TransformError::shape(
            "embedding.base64",
            format!("invalid base64 vector: {error}"),
        )
    })?;
    if bytes.len() % 4 != 0 {
        return Err(TransformError::shape(
            "embedding.base64",
            "IEEE754 binary32 little-endian vector has incomplete bytes",
        ));
    }
    bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|chunk| {
            let value = f32::from_le_bytes(*chunk);
            serde_json::Number::from_f64(f64::from(value)).ok_or_else(|| {
                TransformError::invalid_result(
                    "embedding.base64",
                    "vector contains non-finite value",
                )
            })
        })
        .collect()
}

pub(crate) fn vector_values(
    vector: openai::embeddings::EmbeddingVector,
) -> Result<Vec<serde_json::Number>, TransformError> {
    match vector {
        openai::embeddings::EmbeddingVector::Floats(values) => Ok(values),
        openai::embeddings::EmbeddingVector::Base64(value) => decode_base64(&value),
    }
}

pub(crate) fn encode_base64(values: &[serde_json::Number]) -> Result<String, TransformError> {
    let mut bytes = Vec::with_capacity(values.len() * 4);
    for value in values {
        let value = value.as_f64().ok_or_else(|| {
            TransformError::invalid_result("embedding.values", "value is not a JSON number")
        })?;
        let value = value as f32;
        if !value.is_finite() {
            return Err(TransformError::invalid_result(
                "embedding.values",
                "vector contains non-finite value",
            ));
        }
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    Ok(STANDARD.encode(bytes))
}
