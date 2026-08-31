use serde::{Deserialize, Serialize};

use crate::openai::common::{
    ImageBackground, ImageEditQuality, ImageEditSize, ImageInputFidelity, ImageModeration,
    ImageOutputFormat, ImageQuality, ImageResponseFormat, ImageSize, ImageStyle, OpenAiModelId,
    Rest,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateImageRequest {
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<ImageBackground>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<OpenAiModelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moderation: Option<ImageModeration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_compression: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<ImageOutputFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_images: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<ImageQuality>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_format: Option<ImageResponseFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<ImageSize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<ImageStyle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditImageRequest {
    pub images: Vec<ImageReference>,
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<ImageBackground>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_fidelity: Option<ImageInputFidelity>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mask: Option<ImageReference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<OpenAiModelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moderation: Option<ImageModeration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_compression: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<ImageOutputFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub partial_images: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<ImageEditQuality>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<ImageEditSize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageReference {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}
