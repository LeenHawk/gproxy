use gproxy_protocol::{gemini, openai};

use crate::TransformError;

pub(super) fn convert(
    config: &gemini::GenerationConfig,
) -> Result<Option<openai::ChatResponseFormat>, TransformError> {
    let schema = config
        .response_json_schema
        .clone()
        .or_else(|| config.private_response_json_schema.clone())
        .or_else(|| {
            config
                .response_schema
                .as_ref()
                .and_then(|schema| serde_json::to_value(schema).ok())
        });
    if let Some(schema) = schema {
        let schema = schema
            .as_object()
            .cloned()
            .ok_or_else(|| TransformError::shape("Gemini response schema", "expected an object"))?;
        return Ok(Some(openai::ChatResponseFormat::JsonSchema(crate::wire!(
            openai::ChatJsonSchemaFormat {
                type_: openai::JsonSchemaResponseFormatType::JsonSchema,
                json_schema: openai::JsonSchemaFormat {
                    name: "response".into(),
                    description: None,
                    schema: Some(schema),
                    strict: None,
                    rest: Default::default(),
                },
                rest: Default::default(),
            }
        ))));
    }
    Ok(match config.response_mime_type.as_ref() {
        Some(gemini::ResponseMimeType::Known(gemini::ResponseMimeTypeKnown::ApplicationJson)) => {
            Some(openai::ChatResponseFormat::JsonObject(crate::wire!(
                openai::JsonObjectResponseFormat {
                    type_: openai::JsonObjectResponseFormatType::JsonObject,
                    rest: Default::default(),
                }
            )))
        }
        Some(gemini::ResponseMimeType::Known(
            gemini::ResponseMimeTypeKnown::TextPlain | gemini::ResponseMimeTypeKnown::TextXEnum,
        )) => Some(openai::ChatResponseFormat::Text(crate::wire!(
            openai::TextResponseFormat {
                type_: openai::TextResponseFormatType::Text,
                rest: Default::default(),
            }
        ))),
        Some(other) => {
            return Err(TransformError::unsupported(
                "Gemini response MIME type",
                serde_json::to_string(other)?,
            ));
        }
        None => None,
    })
}
