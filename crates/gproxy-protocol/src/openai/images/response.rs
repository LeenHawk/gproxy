use serde::{Deserialize, Serialize};

use crate::openai::common::{
    ImageOutputFormat, ImageResponseBackground, ImageResponseQuality, ImageResponseSize, Rest,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
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
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct Image {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub b64_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revised_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ImageUsage {
    pub input_tokens: u64,
    pub input_tokens_details: ImageTokenDetails,
    pub output_tokens: u64,
    pub total_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_tokens_details: Option<ImageTokenDetails>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub struct ImageTokenDetails {
    pub image_tokens: u64,
    pub text_tokens: u64,
    #[serde(default, flatten)]
    pub rest: Rest,
}
