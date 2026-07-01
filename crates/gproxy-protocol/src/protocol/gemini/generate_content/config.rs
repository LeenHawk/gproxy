use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::super::common::{
    AudioResponseDelivery, AudioResponseFormatMimeType, ExtraFields, GenerationMediaResolution,
    ImageAspectRatio, ImageResponseAspectRatio, ImageResponseDelivery, ImageResponseFormatMimeType,
    ImageResponseSize, ImageSize, ResponseMimeType, ResponseModality, Schema, SpeechConfig,
    TextResponseFormatMimeType, ThinkingLevel,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GenerationConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stop_sequences: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_mime_type: Option<ResponseMimeType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_schema: Option<Schema>,
    #[serde(
        rename = "_responseJsonSchema",
        skip_serializing_if = "Option::is_none"
    )]
    pub private_response_json_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_json_schema: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ResponseFormatConfig>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub response_modalities: Vec<ResponseModality>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub candidate_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frequency_penalty: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_logprobs: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub logprobs: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_enhanced_civic_answers: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speech_config: Option<SpeechConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_config: Option<ThinkingConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_config: Option<ImageConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media_resolution: Option<GenerationMediaResolution>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ResponseFormatConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<TextResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio: Option<AudioResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<ImageResponseFormat>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TextResponseFormat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<TextResponseFormatMimeType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Value>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AudioResponseFormat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<AudioResponseFormatMimeType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery: Option<AudioResponseDelivery>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sample_rate: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bit_rate: Option<i32>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImageResponseFormat {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<ImageResponseFormatMimeType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delivery: Option<ImageResponseDelivery>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<ImageResponseAspectRatio>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_size: Option<ImageResponseSize>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ThinkingConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_thoughts: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_budget: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<ThinkingLevel>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ImageConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub aspect_ratio: Option<ImageAspectRatio>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_size: Option<ImageSize>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty", flatten)]
    pub extra: ExtraFields,
}
