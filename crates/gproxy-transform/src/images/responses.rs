use bytes::Bytes;
use gproxy_protocol::openai;
use gproxy_protocol::openai::images as openai_images;

use crate::TransformError;

pub(crate) fn image_request(body: Bytes, model: &str, edit: bool) -> Result<Bytes, TransformError> {
    let (input, images, mask) = if edit {
        let input: openai_images::EditImageRequest = serde_json::from_slice(&body)?;
        let create = openai_images::CreateImageRequest {
            prompt: input.prompt,
            background: input.background,
            model: input.model,
            moderation: input.moderation,
            n: input.n,
            output_compression: input.output_compression,
            output_format: input.output_format,
            partial_images: input.partial_images,
            quality: None,
            response_format: None,
            size: None,
            stream: input.stream,
            style: None,
            user: input.user,
            rest: input.rest,
        };
        (create, input.images, input.mask)
    } else {
        (
            serde_json::from_slice::<openai_images::CreateImageRequest>(&body)?,
            Vec::new(),
            None,
        )
    };
    let action = if edit {
        openai::ImageGenerationAction::Edit
    } else {
        openai::ImageGenerationAction::Generate
    };
    let tool = openai::ResponseTool::ImageGeneration {
        action: Some(action),
        background: input.background,
        input_fidelity: None,
        input_image_mask: mask.map(|mask| openai::ImageMask {
            file_id: mask.file_id,
            image_url: mask.image_url,
            rest: mask.rest,
        }),
        model: input.model.clone(),
        moderation: input.moderation,
        output_compression: input.output_compression,
        output_format: input.output_format,
        partial_images: input.partial_images,
        quality: None,
        size: input.size.and_then(|size| {
            serde_json::to_value(size)
                .ok()
                .and_then(|value| serde_json::from_value(value).ok())
        }),
        rest: Default::default(),
    };
    let response_input = if images.is_empty() {
        openai::ResponseInput::Text(input.prompt)
    } else {
        let mut content = vec![openai::ResponseInputContentPart::InputText(
            openai::ResponseInputText {
                text: input.prompt,
                prompt_cache_breakpoint: None,
                rest: Default::default(),
            },
        )];
        content.extend(images.into_iter().map(|image| {
            openai::ResponseInputContentPart::InputImage(openai::ResponseInputImage {
                detail: None,
                file_id: image.file_id,
                image_url: image.image_url,
                prompt_cache_breakpoint: None,
                rest: image.rest,
            })
        }));
        openai::ResponseInput::Items(vec![openai::ResponseItem::Message(
            openai::ResponseMessageItem::Input(openai::ResponseInputMessageItem {
                id: None,
                type_: Some(openai::ResponseMessageItemType::Message),
                role: openai::ResponseInputMessageRole::User,
                content,
                status: None,
                rest: Default::default(),
            }),
        )])
    };
    super::encode(&openai::ResponseCreateRequest {
        input: Some(response_input),
        model: input.model.or_else(|| Some(model.into())),
        tools: Some(vec![tool]),
        stream: input.stream,
        rest: input.rest,
        ..Default::default()
    })
}

pub(crate) fn responses_request(
    body: Bytes,
    model: &str,
    edit: bool,
) -> Result<Bytes, TransformError> {
    let input: openai::ResponseCreateRequest = serde_json::from_slice(&body)?;
    let prompt = match input.input {
        Some(openai::ResponseInput::Text(text)) => text,
        _ => input.instructions.unwrap_or_default(),
    };
    if edit {
        return super::encode(&openai_images::EditImageRequest {
            images: Vec::new(),
            prompt,
            model: Some(model.into()),
            rest: input.rest,
            background: None,
            input_fidelity: None,
            mask: None,
            moderation: None,
            n: None,
            output_compression: None,
            output_format: None,
            partial_images: None,
            quality: None,
            size: None,
            stream: input.stream,
            user: None,
        });
    }
    super::encode(&openai_images::CreateImageRequest {
        prompt,
        model: Some(model.into()),
        stream: input.stream,
        rest: input.rest,
        background: None,
        moderation: None,
        n: None,
        output_compression: None,
        output_format: None,
        partial_images: None,
        quality: None,
        response_format: None,
        size: None,
        style: None,
        user: None,
    })
}

pub(crate) fn responses_to_images(body: Bytes) -> Result<Bytes, TransformError> {
    let input: openai::ResponseObject = serde_json::from_slice(&body)?;
    let data = input
        .output
        .into_iter()
        .filter_map(|item| match item {
            openai::ResponseItem::Typed(item) => match *item {
                openai::TypedResponseItem::ImageGenerationCall {
                    result: Some(data),
                    rest,
                    ..
                } => Some(openai_images::Image {
                    b64_json: Some(data),
                    revised_prompt: None,
                    url: None,
                    rest,
                }),
                _ => None,
            },
            _ => None,
        })
        .collect();
    super::encode(&openai_images::ImagesResponse {
        created: input.created_at.unwrap_or_default(),
        data: Some(data),
        rest: input.rest,
        background: None,
        output_format: None,
        quality: None,
        size: None,
        usage: input.usage.map(|usage| openai_images::ImageUsage {
            input_tokens: u64::from(usage.input_tokens),
            input_tokens_details: openai_images::ImageTokenDetails {
                image_tokens: 0,
                text_tokens: u64::from(usage.input_tokens),
                rest: Default::default(),
            },
            output_tokens: u64::from(usage.output_tokens),
            total_tokens: u64::from(usage.total_tokens),
            output_tokens_details: None,
            rest: usage.rest,
        }),
    })
}

pub(crate) fn images_to_responses(body: Bytes) -> Result<Bytes, TransformError> {
    let input: openai_images::ImagesResponse = serde_json::from_slice(&body)?;
    let output = input
        .data
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .filter_map(|(index, image)| {
            image.b64_json.map(|result| {
                openai::ResponseItem::Typed(Box::new(
                    openai::TypedResponseItem::ImageGenerationCall {
                        id: format!("image_{index}"),
                        result: Some(result),
                        status: openai::ResponseImageGenerationCallStatus::Completed,
                        rest: image.rest,
                    },
                ))
            })
        })
        .collect();
    let usage = input.usage.map(|usage| openai::ResponseUsage {
        input_tokens: u32::try_from(usage.input_tokens).unwrap_or(u32::MAX),
        output_tokens: u32::try_from(usage.output_tokens).unwrap_or(u32::MAX),
        total_tokens: u32::try_from(usage.total_tokens).unwrap_or(u32::MAX),
        input_tokens_details: None,
        output_tokens_details: None,
        rest: usage.rest,
    });
    super::encode(&openai::ResponseObject {
        id: format!("resp_image_{}", input.created),
        created_at: Some(input.created),
        object: openai::ResponseObjectType::Response,
        output,
        status: Some(openai::ResponseStatus::Completed),
        usage,
        rest: input.rest,
        background: None,
        completed_at: None,
        conversation: None,
        error: None,
        incomplete_details: None,
        instructions: None,
        max_output_tokens: None,
        max_tool_calls: None,
        metadata: None,
        model: None,
        moderation: None,
        multi_agent: None,
        output_text: None,
        parallel_tool_calls: None,
        prompt: None,
        prompt_cache_key: None,
        prompt_cache_options: None,
        prompt_cache_retention: None,
        previous_response_id: None,
        reasoning: None,
        safety_identifier: None,
        service_tier: None,
        store: None,
        temperature: None,
        text: None,
        tool_choice: None,
        tools: None,
        top_logprobs: None,
        top_p: None,
        truncation: None,
        user: None,
    })
}
