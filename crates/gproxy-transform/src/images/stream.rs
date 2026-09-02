use bytes::Bytes;
use gproxy_protocol::openai::images as openai_images;
use gproxy_protocol::{gemini, openai};

use crate::TransformError;
use crate::envelope::{Converter, SseFrame};

pub(crate) fn from_gemini(edit: bool) -> Box<dyn Converter> {
    Box::new(GeminiImageStream {
        edit,
        next_index: 0,
        last_image: None,
        usage: None,
    })
}

pub(crate) fn from_responses(edit: bool) -> Box<dyn Converter> {
    Box::new(ResponsesImageStream {
        edit,
        last_image: None,
        usage: None,
    })
}

struct GeminiImageStream {
    edit: bool,
    next_index: u32,
    last_image: Option<String>,
    usage: Option<openai_images::ImageUsage>,
}

struct ResponsesImageStream {
    edit: bool,
    last_image: Option<String>,
    usage: Option<openai_images::ImageUsage>,
}

impl Converter for GeminiImageStream {
    fn frame(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        let input: gemini::GenerateContentResponse = serde_json::from_str(&frame.data)?;
        if let Some(usage) = input.usage_metadata {
            let input_tokens = nonnegative(usage.prompt_token_count);
            let output_tokens = nonnegative(usage.candidates_token_count);
            self.usage = Some(openai_images::ImageUsage {
                input_tokens,
                input_tokens_details: openai_images::ImageTokenDetails {
                    image_tokens: 0,
                    text_tokens: input_tokens,
                    rest: Default::default(),
                },
                output_tokens,
                total_tokens: nonnegative(usage.total_token_count),
                output_tokens_details: None,
                rest: usage.rest,
            });
        }
        let mut output = Vec::new();
        for candidate in input.candidates {
            if let Some(content) = candidate.content {
                for part in content.parts {
                    if let Some(gemini::PartData::InlineData { inline_data, rest }) = part.data {
                        self.last_image = Some(inline_data.data.clone());
                        output.push(image_partial(
                            self.edit,
                            inline_data.data,
                            self.next_index,
                            rest,
                        )?);
                        self.next_index = self.next_index.saturating_add(1);
                    }
                }
            }
            if candidate.finish_reason.is_some()
                && let Some(image) = self.last_image.take()
            {
                output.push(image_completed(self.edit, image, self.usage.take())?);
            }
        }
        Ok(output)
    }

    fn finish(&mut self) -> Result<Vec<Bytes>, TransformError> {
        match self.last_image.take() {
            Some(image) => Ok(vec![image_completed(self.edit, image, self.usage.take())?]),
            None => Ok(Vec::new()),
        }
    }
}

impl Converter for ResponsesImageStream {
    fn frame(&mut self, frame: SseFrame) -> Result<Vec<Bytes>, TransformError> {
        let event: openai::ResponseStreamEvent = serde_json::from_str(&frame.data)?;
        let openai::ResponseStreamEvent::Known(event) = event else {
            return Ok(Vec::new());
        };
        match *event {
            openai::KnownResponseStreamEvent::ResponseImageGenerationCallPartialImage(event) => {
                self.last_image = Some(event.partial_image_b64.clone());
                Ok(vec![image_partial(
                    self.edit,
                    event.partial_image_b64,
                    event.partial_image_index,
                    event.rest,
                )?])
            }
            openai::KnownResponseStreamEvent::ResponseCompleted(event)
            | openai::KnownResponseStreamEvent::ResponseIncomplete(event) => {
                self.usage = event.response.usage.map(response_usage);
                if self.last_image.is_none() {
                    self.last_image =
                        event
                            .response
                            .output
                            .into_iter()
                            .find_map(|item| match item {
                                openai::ResponseItem::Typed(item) => match *item {
                                    openai::TypedResponseItem::ImageGenerationCall {
                                        result,
                                        ..
                                    } => result,
                                    _ => None,
                                },
                                _ => None,
                            });
                }
                Ok(self
                    .last_image
                    .take()
                    .map(|image| image_completed(self.edit, image, self.usage.take()))
                    .transpose()?
                    .into_iter()
                    .collect())
            }
            openai::KnownResponseStreamEvent::ResponseFailed(_)
            | openai::KnownResponseStreamEvent::Error(_) => Err(TransformError::unsupported(
                "Responses image stream",
                "failed response",
            )),
            _ => Ok(Vec::new()),
        }
    }

    fn finish(&mut self) -> Result<Vec<Bytes>, TransformError> {
        match self.last_image.take() {
            Some(image) => Ok(vec![image_completed(self.edit, image, self.usage.take())?]),
            None => Ok(Vec::new()),
        }
    }
}

fn image_partial(
    edit: bool,
    b64_json: String,
    partial_image_index: u32,
    rest: openai::Rest,
) -> Result<Bytes, TransformError> {
    let event = openai_images::ImagePartialEvent {
        b64_json,
        partial_image_index,
        rest,
    };
    let name = if edit {
        "image_edit.partial_image"
    } else {
        "image_generation.partial_image"
    };
    let mut value = serde_json::to_value(event)?;
    value["type"] = serde_json::Value::String(name.into());
    Ok(SseFrame::encode(
        Some(name),
        &serde_json::to_string(&value)?,
    ))
}

fn image_completed(
    edit: bool,
    b64_json: String,
    usage: Option<openai_images::ImageUsage>,
) -> Result<Bytes, TransformError> {
    let event = openai_images::ImageCompletedEvent {
        b64_json,
        usage,
        rest: Default::default(),
    };
    let name = if edit {
        "image_edit.completed"
    } else {
        "image_generation.completed"
    };
    let mut value = serde_json::to_value(event)?;
    value["type"] = serde_json::Value::String(name.into());
    Ok(SseFrame::encode(
        Some(name),
        &serde_json::to_string(&value)?,
    ))
}

fn response_usage(usage: openai::ResponseUsage) -> openai_images::ImageUsage {
    openai_images::ImageUsage {
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
    }
}

fn nonnegative(value: Option<i32>) -> u64 {
    value
        .and_then(|value| u64::try_from(value).ok())
        .unwrap_or_default()
}
