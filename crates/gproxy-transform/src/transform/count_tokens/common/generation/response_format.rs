use crate::protocol::{claude, gemini, openai};

use crate::transform::count_tokens::common::util::{json_object, json_value};

pub(super) fn apply_openai_response_format(
    config: &mut gemini::GenerationConfig,
    format: openai::ResponseFormat,
) {
    match format {
        openai::ResponseFormat::Text(_) => {
            config.response_mime_type = Some(gemini::ResponseMimeType::Known(
                gemini::ResponseMimeTypeKnown::TextPlain,
            ));
        }
        openai::ResponseFormat::JsonObject(_) => {
            config.response_mime_type = Some(gemini::ResponseMimeType::Known(
                gemini::ResponseMimeTypeKnown::ApplicationJson,
            ));
        }
        openai::ResponseFormat::JsonSchema(format) => {
            config.response_mime_type = Some(gemini::ResponseMimeType::Known(
                gemini::ResponseMimeTypeKnown::ApplicationJson,
            ));
            config.response_json_schema = Some(json_value(format.schema));
        }
    }
}

pub(in crate::transform::count_tokens) fn claude_generation_to_openai_text(
    output_config: Option<&claude::OutputConfig>,
    output_format: Option<claude::JsonSchemaFormat>,
) -> Option<openai::TextConfig> {
    let format = output_config
        .and_then(|config| config.format.clone())
        .or(output_format)?;
    Some(openai::TextConfig {
        format: Some(openai::ResponseFormat::JsonSchema(
            openai::JsonSchemaResponseFormat {
                type_: openai::JsonSchemaResponseFormatType::JsonSchema,
                name: "response".to_owned(),
                schema: json_object(json_value(format.schema)),
                description: None,
                strict: None,
                extra: Default::default(),
            },
        )),
        verbosity: None,
        extra: Default::default(),
    })
}

pub(in crate::transform::count_tokens) fn gemini_generation_to_openai_text(
    generation_config: Option<&gemini::GenerationConfig>,
) -> Option<openai::TextConfig> {
    let config = generation_config?;
    let format = if let Some(schema) = config
        .response_json_schema
        .clone()
        .or_else(|| config.response_schema.clone().map(json_value))
    {
        openai::ResponseFormat::JsonSchema(openai::JsonSchemaResponseFormat {
            type_: openai::JsonSchemaResponseFormatType::JsonSchema,
            name: "response".to_owned(),
            schema: json_object(schema),
            description: None,
            strict: None,
            extra: Default::default(),
        })
    } else if matches!(
        config.response_mime_type,
        Some(gemini::ResponseMimeType::Known(
            gemini::ResponseMimeTypeKnown::ApplicationJson
        ))
    ) {
        openai::ResponseFormat::JsonObject(openai::JsonObjectResponseFormat {
            type_: openai::JsonObjectResponseFormatType::JsonObject,
            extra: Default::default(),
        })
    } else if matches!(
        config.response_mime_type,
        Some(gemini::ResponseMimeType::Known(
            gemini::ResponseMimeTypeKnown::TextPlain
        ))
    ) {
        openai::ResponseFormat::Text(openai::TextResponseFormat {
            type_: openai::TextResponseFormatType::Text,
            extra: Default::default(),
        })
    } else {
        return None;
    };

    Some(openai::TextConfig {
        format: Some(format),
        verbosity: None,
        extra: Default::default(),
    })
}

pub(in crate::transform::count_tokens) fn gemini_generation_to_claude_output_format(
    generation_config: Option<&gemini::GenerationConfig>,
) -> Option<claude::JsonSchemaFormat> {
    let config = generation_config?;
    let schema = config
        .response_json_schema
        .clone()
        .or_else(|| config.private_response_json_schema.clone())
        .or_else(|| {
            config
                .response_format
                .as_ref()
                .and_then(|format| format.text.as_ref())
                .and_then(|format| format.schema.clone())
        })
        .or_else(|| config.response_schema.clone().map(json_value))?;

    Some(claude::JsonSchemaFormat {
        type_: claude::JsonSchemaFormatType::Known(claude::JsonSchemaFormatTypeKnown::JsonSchema),
        schema: json_object(schema),
        extra: Default::default(),
    })
}

pub(in crate::transform::count_tokens) fn openai_text_to_claude_output_format(
    text: Option<openai::TextConfig>,
) -> Option<claude::JsonSchemaFormat> {
    openai_response_format_to_claude(&text?.format?)
}

pub(super) fn openai_response_format_to_claude(
    format: &openai::ResponseFormat,
) -> Option<claude::JsonSchemaFormat> {
    let openai::ResponseFormat::JsonSchema(format) = format else {
        return None;
    };
    Some(claude::JsonSchemaFormat {
        type_: claude::JsonSchemaFormatType::Known(claude::JsonSchemaFormatTypeKnown::JsonSchema),
        schema: format.schema.clone(),
        extra: Default::default(),
    })
}
