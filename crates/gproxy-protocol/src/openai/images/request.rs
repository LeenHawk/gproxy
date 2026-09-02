use serde::{Deserialize, Deserializer, Serialize, de};

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
    #[serde(alias = "image")]
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

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImageReference {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

impl<'de> Deserialize<'de> for ImageReference {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = serde_json::Value::deserialize(deserializer)?;
        if let serde_json::Value::String(value) = value {
            if value.trim().is_empty() {
                return Err(de::Error::custom("image reference must not be empty"));
            }
            let image_url = value.starts_with("http://")
                || value.starts_with("https://")
                || value.starts_with("data:");
            return Ok(Self {
                file_id: (!image_url).then_some(value.clone()),
                image_url: image_url.then_some(value),
                rest: Default::default(),
            });
        }
        #[derive(Deserialize)]
        struct Object {
            file_id: Option<String>,
            image_url: Option<String>,
            #[serde(default, flatten)]
            rest: Rest,
        }
        let object: Object = serde_json::from_value(value).map_err(de::Error::custom)?;
        if object.file_id.is_some() == object.image_url.is_some() {
            return Err(de::Error::custom(
                "image reference requires exactly one of file_id or image_url",
            ));
        }
        Ok(Self {
            file_id: object.file_id,
            image_url: object.image_url,
            rest: object.rest,
        })
    }
}
