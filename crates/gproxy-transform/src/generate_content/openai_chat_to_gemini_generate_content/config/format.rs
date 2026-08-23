use gproxy_protocol::{gemini, openai};

use crate::TransformError;

pub(super) fn convert(
    format: Option<openai::ChatResponseFormat>,
) -> Result<(Option<gemini::ResponseMimeType>, Option<serde_json::Value>), TransformError> {
    Ok(match format {
        None => (None, None),
        Some(openai::ChatResponseFormat::Text(_)) => (
            Some(gemini::ResponseMimeType::Known(
                gemini::ResponseMimeTypeKnown::TextPlain,
            )),
            None,
        ),
        Some(openai::ChatResponseFormat::JsonObject(_)) => (
            Some(gemini::ResponseMimeType::Known(
                gemini::ResponseMimeTypeKnown::ApplicationJson,
            )),
            None,
        ),
        Some(openai::ChatResponseFormat::JsonSchema(format)) => {
            let schema = format.json_schema.schema.ok_or_else(|| {
                TransformError::shape("Chat response format", "JSON schema is missing")
            })?;
            (
                Some(gemini::ResponseMimeType::Known(
                    gemini::ResponseMimeTypeKnown::ApplicationJson,
                )),
                Some(serde_json::Value::Object(schema)),
            )
        }
        Some(openai::ChatResponseFormat::Unknown(raw)) => {
            return Err(TransformError::unsupported(
                "Chat response format",
                raw.to_string(),
            ));
        }
    })
}
