use serde::{Deserialize, Serialize};

use crate::openai::create_image::types::{
    OpenAiApiError, OpenAiImageBackground, OpenAiImageOutputFormat, OpenAiImageUsage,
};
use crate::openai::create_image_edit::types::{OpenAiImageEditQuality, OpenAiImageEditSize};

/// Parsed SSE stream body for `POST /images/edits` with `stream=true`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct OpenAiCreateImageEditSseStreamBody {
    /// SSE events in receive order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<OpenAiCreateImageEditSseEvent>,
}

/// A single SSE event frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OpenAiCreateImageEditSseEvent {
    /// Optional SSE `event` field.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<String>,
    /// SSE `data` field payload.
    pub data: OpenAiCreateImageEditSseData,
}

/// SSE `data` payload.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OpenAiCreateImageEditSseData {
    /// A regular stream event object.
    Event(ImageEditStreamEvent),
    /// Stream end marker (`[DONE]`).
    Done(String),
}

impl OpenAiCreateImageEditSseData {
    pub fn is_done(&self) -> bool {
        matches!(self, Self::Done(marker) if marker == "[DONE]")
    }
}

/// Stream event union documented by OpenAI image edit streaming.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ImageEditStreamEvent {
    #[serde(rename = "image_edit.partial_image")]
    PartialImage {
        b64_json: String,
        background: OpenAiImageBackground,
        created_at: u64,
        output_format: OpenAiImageOutputFormat,
        partial_image_index: u32,
        quality: OpenAiImageEditQuality,
        size: OpenAiImageEditSize,
    },
    #[serde(rename = "image_edit.completed")]
    Completed {
        b64_json: String,
        background: OpenAiImageBackground,
        created_at: u64,
        output_format: OpenAiImageOutputFormat,
        quality: OpenAiImageEditQuality,
        size: OpenAiImageEditSize,
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
