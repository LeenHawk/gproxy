use crate::openai::count_tokens::types as ot;
use crate::openai::create_image::types as it;
use crate::openai::create_image_edit::types as iet;
use crate::openai::create_response::response::ResponseBody;
use crate::openai::create_response::types as rt;
use crate::transform::utils::TransformError;

pub(crate) fn user_message_from_parts(parts: Vec<ot::ResponseInputContent>) -> ot::ResponseInput {
    ot::ResponseInput::Items(vec![ot::ResponseInputItem::Message(
        ot::ResponseInputMessage {
            content: ot::ResponseInputMessageContent::List(parts),
            role: ot::ResponseInputMessageRole::User,
            phase: None,
            status: None,
            type_: Some(ot::ResponseInputMessageType::Message),
        },
    )])
}

pub(crate) fn response_image_background_from_request(
    background: Option<it::OpenAiImageBackground>,
) -> Option<ot::ResponseImageGenerationBackground> {
    match background {
        Some(it::OpenAiImageBackground::Transparent) => {
            Some(ot::ResponseImageGenerationBackground::Transparent)
        }
        Some(it::OpenAiImageBackground::Opaque) => {
            Some(ot::ResponseImageGenerationBackground::Opaque)
        }
        Some(it::OpenAiImageBackground::Auto) => Some(ot::ResponseImageGenerationBackground::Auto),
        None => None,
    }
}

pub(crate) fn response_image_output_format_from_request(
    output_format: Option<it::OpenAiImageOutputFormat>,
) -> Option<ot::ResponseImageGenerationOutputFormat> {
    match output_format {
        Some(it::OpenAiImageOutputFormat::Png) => {
            Some(ot::ResponseImageGenerationOutputFormat::Png)
        }
        Some(it::OpenAiImageOutputFormat::Jpeg) => {
            Some(ot::ResponseImageGenerationOutputFormat::Jpeg)
        }
        Some(it::OpenAiImageOutputFormat::Webp) => {
            Some(ot::ResponseImageGenerationOutputFormat::Webp)
        }
        None => None,
    }
}

pub(crate) fn response_image_moderation_from_request(
    moderation: Option<it::OpenAiImageModeration>,
) -> Option<ot::ResponseImageGenerationModeration> {
    match moderation {
        Some(it::OpenAiImageModeration::Auto) => Some(ot::ResponseImageGenerationModeration::Auto),
        Some(it::OpenAiImageModeration::Low) => Some(ot::ResponseImageGenerationModeration::Low),
        None => None,
    }
}

pub(crate) fn response_image_quality_from_create_image_request(
    quality: Option<it::OpenAiImageQuality>,
) -> Option<ot::ResponseImageGenerationQuality> {
    match quality {
        Some(it::OpenAiImageQuality::Low) => Some(ot::ResponseImageGenerationQuality::Low),
        Some(it::OpenAiImageQuality::Medium) => Some(ot::ResponseImageGenerationQuality::Medium),
        Some(it::OpenAiImageQuality::High) => Some(ot::ResponseImageGenerationQuality::High),
        Some(it::OpenAiImageQuality::Auto) => Some(ot::ResponseImageGenerationQuality::Auto),
        Some(it::OpenAiImageQuality::Hd) => Some(ot::ResponseImageGenerationQuality::High),
        Some(it::OpenAiImageQuality::Standard) => Some(ot::ResponseImageGenerationQuality::Auto),
        None => None,
    }
}

pub(crate) fn response_image_quality_from_create_image_edit_request(
    quality: Option<iet::OpenAiImageEditQuality>,
) -> Option<ot::ResponseImageGenerationQuality> {
    match quality {
        Some(iet::OpenAiImageEditQuality::Low) => Some(ot::ResponseImageGenerationQuality::Low),
        Some(iet::OpenAiImageEditQuality::Medium) => {
            Some(ot::ResponseImageGenerationQuality::Medium)
        }
        Some(iet::OpenAiImageEditQuality::High) => Some(ot::ResponseImageGenerationQuality::High),
        Some(iet::OpenAiImageEditQuality::Auto) => Some(ot::ResponseImageGenerationQuality::Auto),
        None => None,
    }
}

pub(crate) fn response_image_size_from_create_image_request(
    size: Option<it::OpenAiImageSize>,
) -> Result<Option<ot::ResponseImageGenerationSize>, TransformError> {
    match size {
        Some(it::OpenAiImageSize::Auto) => Ok(Some(ot::ResponseImageGenerationSize::Auto)),
        Some(it::OpenAiImageSize::S1024x1024) => {
            Ok(Some(ot::ResponseImageGenerationSize::S1024x1024))
        }
        Some(it::OpenAiImageSize::S1536x1024) => {
            Ok(Some(ot::ResponseImageGenerationSize::S1536x1024))
        }
        Some(it::OpenAiImageSize::S1024x1536) => {
            Ok(Some(ot::ResponseImageGenerationSize::S1024x1536))
        }
        Some(
            it::OpenAiImageSize::S256x256
            | it::OpenAiImageSize::S512x512
            | it::OpenAiImageSize::S1792x1024
            | it::OpenAiImageSize::S1024x1792,
        ) => Err(TransformError::not_implemented(
            "cannot convert OpenAI image request with unsupported size to OpenAI responses.create request",
        )),
        None => Ok(None),
    }
}

pub(crate) fn response_image_size_from_create_image_edit_request(
    size: Option<iet::OpenAiImageEditSize>,
) -> Option<ot::ResponseImageGenerationSize> {
    match size {
        Some(iet::OpenAiImageEditSize::Auto) => Some(ot::ResponseImageGenerationSize::Auto),
        Some(iet::OpenAiImageEditSize::S1024x1024) => {
            Some(ot::ResponseImageGenerationSize::S1024x1024)
        }
        Some(iet::OpenAiImageEditSize::S1536x1024) => {
            Some(ot::ResponseImageGenerationSize::S1536x1024)
        }
        Some(iet::OpenAiImageEditSize::S1024x1536) => {
            Some(ot::ResponseImageGenerationSize::S1024x1536)
        }
        None => None,
    }
}

pub(crate) fn response_image_model_from_create_image_model(
    model: it::OpenAiImageModel,
) -> ot::ResponseImageGenerationModel {
    match model {
        it::OpenAiImageModel::Known(it::OpenAiImageModelKnown::GptImage1) => {
            ot::ResponseImageGenerationModel::Known(
                ot::ResponseImageGenerationModelKnown::GptImage1,
            )
        }
        it::OpenAiImageModel::Known(it::OpenAiImageModelKnown::GptImage1Mini) => {
            ot::ResponseImageGenerationModel::Known(
                ot::ResponseImageGenerationModelKnown::GptImage1Mini,
            )
        }
        it::OpenAiImageModel::Known(it::OpenAiImageModelKnown::GptImage15) => {
            ot::ResponseImageGenerationModel::Known(
                ot::ResponseImageGenerationModelKnown::GptImage15,
            )
        }
        it::OpenAiImageModel::Known(it::OpenAiImageModelKnown::DallE2) => {
            ot::ResponseImageGenerationModel::Custom("dall-e-2".to_string())
        }
        it::OpenAiImageModel::Known(it::OpenAiImageModelKnown::DallE3) => {
            ot::ResponseImageGenerationModel::Custom("dall-e-3".to_string())
        }
        it::OpenAiImageModel::Custom(model) => ot::ResponseImageGenerationModel::Custom(model),
    }
}

pub(crate) fn response_image_model_from_create_image_edit_model(
    model: iet::OpenAiImageEditModel,
) -> ot::ResponseImageGenerationModel {
    match model {
        iet::OpenAiImageEditModel::Known(iet::OpenAiImageEditModelKnown::GptImage1) => {
            ot::ResponseImageGenerationModel::Known(
                ot::ResponseImageGenerationModelKnown::GptImage1,
            )
        }
        iet::OpenAiImageEditModel::Known(iet::OpenAiImageEditModelKnown::GptImage1Mini) => {
            ot::ResponseImageGenerationModel::Known(
                ot::ResponseImageGenerationModelKnown::GptImage1Mini,
            )
        }
        iet::OpenAiImageEditModel::Known(iet::OpenAiImageEditModelKnown::GptImage15) => {
            ot::ResponseImageGenerationModel::Known(
                ot::ResponseImageGenerationModelKnown::GptImage15,
            )
        }
        iet::OpenAiImageEditModel::Known(iet::OpenAiImageEditModelKnown::ChatgptImageLatest) => {
            ot::ResponseImageGenerationModel::Custom("chatgpt-image-latest".to_string())
        }
        iet::OpenAiImageEditModel::Custom(model) => ot::ResponseImageGenerationModel::Custom(model),
    }
}

pub(crate) fn create_image_model_to_string(model: &it::OpenAiImageModel) -> String {
    match model {
        it::OpenAiImageModel::Known(it::OpenAiImageModelKnown::GptImage1) => {
            "gpt-image-1".to_string()
        }
        it::OpenAiImageModel::Known(it::OpenAiImageModelKnown::GptImage1Mini) => {
            "gpt-image-1-mini".to_string()
        }
        it::OpenAiImageModel::Known(it::OpenAiImageModelKnown::GptImage15) => {
            "gpt-image-1.5".to_string()
        }
        it::OpenAiImageModel::Known(it::OpenAiImageModelKnown::DallE2) => "dall-e-2".to_string(),
        it::OpenAiImageModel::Known(it::OpenAiImageModelKnown::DallE3) => "dall-e-3".to_string(),
        it::OpenAiImageModel::Custom(model) => model.clone(),
    }
}

pub(crate) fn create_image_edit_model_to_string(model: &iet::OpenAiImageEditModel) -> String {
    match model {
        iet::OpenAiImageEditModel::Known(iet::OpenAiImageEditModelKnown::GptImage1) => {
            "gpt-image-1".to_string()
        }
        iet::OpenAiImageEditModel::Known(iet::OpenAiImageEditModelKnown::GptImage1Mini) => {
            "gpt-image-1-mini".to_string()
        }
        iet::OpenAiImageEditModel::Known(iet::OpenAiImageEditModelKnown::GptImage15) => {
            "gpt-image-1.5".to_string()
        }
        iet::OpenAiImageEditModel::Known(iet::OpenAiImageEditModelKnown::ChatgptImageLatest) => {
            "chatgpt-image-latest".to_string()
        }
        iet::OpenAiImageEditModel::Custom(model) => model.clone(),
    }
}

pub(crate) fn image_tool_choice() -> ot::ResponseToolChoice {
    ot::ResponseToolChoice::Types(ot::ResponseToolChoiceTypes {
        type_: ot::ResponseToolChoiceBuiltinType::ImageGeneration,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreferredImageAction {
    Generate,
    Edit,
}

fn generated_image_background_from_response(
    background: Option<&ot::ResponseImageGenerationBackground>,
) -> Option<it::OpenAiGeneratedImageBackground> {
    match background {
        Some(ot::ResponseImageGenerationBackground::Transparent) => {
            Some(it::OpenAiGeneratedImageBackground::Transparent)
        }
        Some(ot::ResponseImageGenerationBackground::Opaque) => {
            Some(it::OpenAiGeneratedImageBackground::Opaque)
        }
        Some(ot::ResponseImageGenerationBackground::Auto) | None => None,
    }
}

fn output_format_from_response(
    output_format: Option<&ot::ResponseImageGenerationOutputFormat>,
) -> Option<it::OpenAiImageOutputFormat> {
    match output_format {
        Some(ot::ResponseImageGenerationOutputFormat::Png) => {
            Some(it::OpenAiImageOutputFormat::Png)
        }
        Some(ot::ResponseImageGenerationOutputFormat::Jpeg) => {
            Some(it::OpenAiImageOutputFormat::Jpeg)
        }
        Some(ot::ResponseImageGenerationOutputFormat::Webp) => {
            Some(it::OpenAiImageOutputFormat::Webp)
        }
        None => None,
    }
}

fn generated_image_quality_from_response(
    quality: Option<&ot::ResponseImageGenerationQuality>,
) -> Option<it::OpenAiGeneratedImageQuality> {
    match quality {
        Some(ot::ResponseImageGenerationQuality::Low) => Some(it::OpenAiGeneratedImageQuality::Low),
        Some(ot::ResponseImageGenerationQuality::Medium) => {
            Some(it::OpenAiGeneratedImageQuality::Medium)
        }
        Some(ot::ResponseImageGenerationQuality::High) => {
            Some(it::OpenAiGeneratedImageQuality::High)
        }
        Some(ot::ResponseImageGenerationQuality::Auto) | None => None,
    }
}

fn generated_image_size_from_response(
    size: Option<&ot::ResponseImageGenerationSize>,
) -> Option<it::OpenAiGeneratedImageSize> {
    match size {
        Some(ot::ResponseImageGenerationSize::S1024x1024) => {
            Some(it::OpenAiGeneratedImageSize::S1024x1024)
        }
        Some(ot::ResponseImageGenerationSize::S1024x1536) => {
            Some(it::OpenAiGeneratedImageSize::S1024x1536)
        }
        Some(ot::ResponseImageGenerationSize::S1536x1024) => {
            Some(it::OpenAiGeneratedImageSize::S1536x1024)
        }
        Some(ot::ResponseImageGenerationSize::Auto) | None => None,
    }
}

fn image_generation_tool_from_tools(
    tools: &[rt::ResponseTool],
    preferred_action: PreferredImageAction,
) -> Option<&ot::ResponseImageGenerationTool> {
    let mut fallback = None;
    let mut auto_or_unspecified = None;

    for tool in tools {
        let rt::ResponseTool::ImageGeneration(image_tool) = tool else {
            continue;
        };

        if fallback.is_none() {
            fallback = Some(image_tool);
        }

        match (preferred_action, image_tool.action.as_ref()) {
            (PreferredImageAction::Generate, Some(ot::ResponseImageGenerationAction::Generate))
            | (PreferredImageAction::Edit, Some(ot::ResponseImageGenerationAction::Edit)) => {
                return Some(image_tool);
            }
            (_, Some(ot::ResponseImageGenerationAction::Auto)) | (_, None)
                if auto_or_unspecified.is_none() =>
            {
                auto_or_unspecified = Some(image_tool);
            }
            _ => {}
        }
    }

    auto_or_unspecified.or(fallback)
}

pub(crate) fn create_image_response_body_from_response(
    body: ResponseBody,
    preferred_action: PreferredImageAction,
) -> Result<it::OpenAiCreateImageResponseBody, TransformError> {
    if !body
        .output
        .iter()
        .any(|item| matches!(item, rt::ResponseOutputItem::ImageGenerationCall(_)))
    {
        return Err(TransformError::not_implemented(
            "cannot convert OpenAI response without image_generation_call",
        ));
    }

    let image_tool = image_generation_tool_from_tools(&body.tools, preferred_action);
    let data = body
        .output
        .into_iter()
        .filter_map(|item| match item {
            rt::ResponseOutputItem::ImageGenerationCall(call) => Some(it::OpenAiGeneratedImage {
                b64_json: call.result.filter(|s| !s.is_empty()),
                ..Default::default()
            }),
            _ => None,
        })
        .collect::<Vec<_>>();

    Ok(it::OpenAiCreateImageResponseBody {
        created: body.created_at,
        background: image_tool
            .and_then(|tool| generated_image_background_from_response(tool.background.as_ref())),
        data: Some(data),
        output_format: image_tool
            .and_then(|tool| output_format_from_response(tool.output_format.as_ref())),
        quality: image_tool
            .and_then(|tool| generated_image_quality_from_response(tool.quality.as_ref())),
        size: image_tool.and_then(|tool| generated_image_size_from_response(tool.size.as_ref())),
        usage: None,
    })
}

pub(crate) fn stream_background_from_response_config(
    background: Option<&ot::ResponseImageGenerationBackground>,
) -> it::OpenAiImageBackground {
    match background {
        Some(ot::ResponseImageGenerationBackground::Transparent) => {
            it::OpenAiImageBackground::Transparent
        }
        Some(ot::ResponseImageGenerationBackground::Opaque) => it::OpenAiImageBackground::Opaque,
        Some(ot::ResponseImageGenerationBackground::Auto) | None => it::OpenAiImageBackground::Auto,
    }
}

pub(crate) fn stream_output_format_from_response_config(
    output_format: Option<&ot::ResponseImageGenerationOutputFormat>,
) -> it::OpenAiImageOutputFormat {
    match output_format {
        Some(ot::ResponseImageGenerationOutputFormat::Png) | None => {
            it::OpenAiImageOutputFormat::Png
        }
        Some(ot::ResponseImageGenerationOutputFormat::Jpeg) => it::OpenAiImageOutputFormat::Jpeg,
        Some(ot::ResponseImageGenerationOutputFormat::Webp) => it::OpenAiImageOutputFormat::Webp,
    }
}

pub(crate) fn stream_quality_from_response_config_for_create_image(
    quality: Option<&ot::ResponseImageGenerationQuality>,
) -> it::OpenAiImageQuality {
    match quality {
        Some(ot::ResponseImageGenerationQuality::Low) => it::OpenAiImageQuality::Low,
        Some(ot::ResponseImageGenerationQuality::Medium) => it::OpenAiImageQuality::Medium,
        Some(ot::ResponseImageGenerationQuality::High) => it::OpenAiImageQuality::High,
        Some(ot::ResponseImageGenerationQuality::Auto) | None => it::OpenAiImageQuality::Auto,
    }
}

pub(crate) fn stream_size_from_response_config_for_create_image(
    size: Option<&ot::ResponseImageGenerationSize>,
) -> it::OpenAiImageSize {
    match size {
        Some(ot::ResponseImageGenerationSize::S1024x1024) => it::OpenAiImageSize::S1024x1024,
        Some(ot::ResponseImageGenerationSize::S1024x1536) => it::OpenAiImageSize::S1024x1536,
        Some(ot::ResponseImageGenerationSize::S1536x1024) => it::OpenAiImageSize::S1536x1024,
        Some(ot::ResponseImageGenerationSize::Auto) | None => it::OpenAiImageSize::Auto,
    }
}

pub(crate) fn best_effort_image_usage_from_response_usage(
    usage: Option<&rt::ResponseUsage>,
) -> it::OpenAiImageUsage {
    let Some(usage) = usage else {
        return it::OpenAiImageUsage {
            input_tokens: 0,
            input_tokens_details: it::OpenAiImageTokenDetails {
                image_tokens: 0,
                text_tokens: 0,
            },
            output_tokens: 0,
            total_tokens: 0,
            output_tokens_details: None,
        };
    };

    it::OpenAiImageUsage {
        input_tokens: usage.input_tokens,
        input_tokens_details: it::OpenAiImageTokenDetails {
            image_tokens: 0,
            text_tokens: 0,
        },
        output_tokens: usage.output_tokens,
        total_tokens: usage.total_tokens,
        output_tokens_details: None,
    }
}

pub(crate) fn stream_error_from_response_error(
    code: Option<String>,
    message: String,
    param: Option<String>,
) -> crate::openai::types::OpenAiApiError {
    crate::openai::types::OpenAiApiError {
        message,
        type_: "stream_error".to_string(),
        param,
        code,
    }
}
