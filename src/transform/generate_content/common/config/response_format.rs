use serde_json::{Number, Value};

use crate::protocol::{claude, gemini, openai};

pub(in crate::transform::generate_content) fn chat_response_format_to_claude(
    format: Option<openai::ChatResponseFormat>,
) -> Option<claude::JsonSchemaFormat> {
    match format? {
        openai::ChatResponseFormat::ChatJsonSchema(schema) => {
            let schema = schema.json_schema.schema.unwrap_or_default();
            Some(claude::JsonSchemaFormat {
                type_: claude::JsonSchemaFormatType::Known(
                    claude::JsonSchemaFormatTypeKnown::JsonSchema,
                ),
                schema,
                extra: Default::default(),
            })
        }
        openai::ChatResponseFormat::JsonObject(_) | openai::ChatResponseFormat::Text(_) => None,
    }
}

pub(in crate::transform::generate_content) fn response_text_config_to_claude(
    text: Option<openai::TextConfig>,
) -> Option<claude::JsonSchemaFormat> {
    let format = text?.format?;
    match format {
        openai::ResponseFormat::JsonSchema(format) => Some(claude::JsonSchemaFormat {
            type_: claude::JsonSchemaFormatType::Known(
                claude::JsonSchemaFormatTypeKnown::JsonSchema,
            ),
            schema: format.schema,
            extra: Default::default(),
        }),
        openai::ResponseFormat::JsonObject(_) | openai::ResponseFormat::Text(_) => None,
    }
}

pub(in crate::transform::generate_content) fn claude_output_format_to_chat(
    format: Option<claude::JsonSchemaFormat>,
) -> Option<openai::ChatResponseFormat> {
    let format = format?;
    Some(openai::ChatResponseFormat::ChatJsonSchema(
        openai::ChatJsonSchemaFormat {
            type_: openai::JsonSchemaResponseFormatType::JsonSchema,
            json_schema: openai::JsonSchemaFormat {
                name: "response".to_owned(),
                description: None,
                schema: Some(format.schema),
                strict: None,
                extra: Default::default(),
            },
            extra: Default::default(),
        },
    ))
}

pub(in crate::transform::generate_content) fn chat_response_format_to_gemini(
    format: Option<openai::ChatResponseFormat>,
) -> Option<gemini::ResponseMimeType> {
    match format? {
        openai::ChatResponseFormat::Text(_) => Some(gemini::ResponseMimeType::Known(
            gemini::ResponseMimeTypeKnown::TextPlain,
        )),
        openai::ChatResponseFormat::JsonObject(_)
        | openai::ChatResponseFormat::ChatJsonSchema(_) => Some(gemini::ResponseMimeType::Known(
            gemini::ResponseMimeTypeKnown::ApplicationJson,
        )),
    }
}

pub(in crate::transform::generate_content) fn response_format_to_gemini_schema(
    format: Option<openai::ChatResponseFormat>,
) -> Option<Value> {
    let openai::ChatResponseFormat::ChatJsonSchema(format) = format? else {
        return None;
    };
    format.json_schema.schema.map(json_schema_to_value)
}

pub(in crate::transform::generate_content) fn gemini_response_mime_to_chat(
    config: Option<&gemini::GenerationConfig>,
) -> Option<openai::ChatResponseFormat> {
    match config?.response_mime_type.as_ref()? {
        gemini::ResponseMimeType::Known(gemini::ResponseMimeTypeKnown::TextPlain) => Some(
            openai::ChatResponseFormat::Text(openai::TextResponseFormat {
                type_: openai::TextResponseFormatType::Text,
                extra: Default::default(),
            }),
        ),
        gemini::ResponseMimeType::Known(gemini::ResponseMimeTypeKnown::ApplicationJson) => Some(
            openai::ChatResponseFormat::JsonObject(openai::JsonObjectResponseFormat {
                type_: openai::JsonObjectResponseFormatType::JsonObject,
                extra: Default::default(),
            }),
        ),
        _ => None,
    }
}

fn json_schema_to_value(schema: openai::JsonSchema) -> Value {
    Value::Object(
        schema
            .into_iter()
            .map(|(key, value)| (key, normalize_json_schema_value(value)))
            .collect(),
    )
}

fn normalize_json_schema_value(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(normalize_json_schema_value)
                .collect(),
        ),
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, value)| (key, normalize_json_schema_value(value)))
                .collect(),
        ),
        Value::Number(number) => normalize_number(number),
        other => other,
    }
}

fn normalize_number(number: Number) -> Value {
    Value::Number(number)
}
