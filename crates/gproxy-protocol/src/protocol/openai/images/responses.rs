use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::super::common::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImagesResponse {
    pub created: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background: Option<ImageResponseBackground>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Vec<Image>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_format: Option<ImageOutputFormat>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<ImageResponseQuality>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<ImageResponseSize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ImageUsage>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Image {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b64_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revised_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageUsage {
    pub input_tokens: u32,
    pub input_tokens_details: ImageTokenDetails,
    pub output_tokens: u32,
    pub total_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens_details: Option<ImageTokenDetails>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageTokenDetails {
    pub image_tokens: u32,
    pub text_tokens: u32,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}
