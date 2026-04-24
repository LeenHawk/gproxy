use serde::{Deserialize, Serialize};

use crate::openai::create_image::types::{
    OpenAiApiError, OpenAiImageBackground, OpenAiImageOutputFormat, OpenAiImageQuality,
    OpenAiImageSize, OpenAiImageUsage,
};

/// Parsed SSE stream body for `POST /images/generations` with `stream=true`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct OpenAiCreateImageSseStreamBody {
    /// SSE events in receive order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<OpenAiCreateImageSseEvent>,
}

/// A single SSE event frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenAiCreateImageSseEvent {
    /// Optional SSE `event` field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    /// SSE `data` field payload.
    pub data: OpenAiCreateImageSseData,
}

/// SSE `data` payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenAiCreateImageSseData {
    /// A regular stream event object.
    Event(ImageGenerationStreamEvent),
    /// Stream end marker (`[DONE]`).
    Done(String),
}

impl OpenAiCreateImageSseData {
    pub fn is_done(&self) -> bool {
        matches!(self, Self::Done(marker) if marker == "[DONE]")
    }
}

/// Stream event union documented by OpenAI image generation streaming.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ImageGenerationStreamEvent {
    #[serde(rename = "image_generation.partial_image")]
    PartialImage {
        b64_json: String,
        background: OpenAiImageBackground,
        created_at: u64,
        output_format: OpenAiImageOutputFormat,
        partial_image_index: u32,
        quality: OpenAiImageQuality,
        size: OpenAiImageSize,
    },
    #[serde(rename = "image_generation.completed")]
    Completed {
        b64_json: String,
        background: OpenAiImageBackground,
        created_at: u64,
        output_format: OpenAiImageOutputFormat,
        quality: OpenAiImageQuality,
        size: OpenAiImageSize,
        usage: OpenAiImageUsage,
    },
    #[serde(rename = "error")]
    Error { error: OpenAiApiError },
    /// Undocumented heartbeat frame some OpenAI-compatible backends ship
    /// mid-stream (`{"type":"keepalive"}`). Ignored during aggregation.
    #[serde(rename = "keepalive")]
    Keepalive {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sequence_number: Option<u64>,
    },
}
