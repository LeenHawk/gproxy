mod edit;
mod generation;

#[cfg(test)]
mod tests;

use serde::{Deserialize, Serialize, de};
use serde_json::Value;

use crate::openai::common::{ImageStreamEventType, Rest};

use super::ImageUsage;

pub use edit::*;
pub use generation::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImagePartialEvent {
    pub b64_json: String,
    pub partial_image_index: u32,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageCompletedEvent {
    pub b64_json: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub usage: Option<ImageUsage>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnknownImageStreamEvent {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<ImageStreamEventType>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ImageStreamEvent {
    Known(KnownImageStreamEvent),
    Unknown(UnknownImageStreamEvent),
}

impl<'de> Deserialize<'de> for ImageStreamEvent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        match image_stream_event_type::<D::Error>(&value)? {
            Some(ImageStreamEventType::Known(_)) => serde_json::from_value(value)
                .map(Self::Known)
                .map_err(de::Error::custom),
            Some(ImageStreamEventType::Unknown(_)) | None => serde_json::from_value(value)
                .map(Self::Unknown)
                .map_err(de::Error::custom),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum KnownImageStreamEvent {
    #[serde(rename = "image_generation.partial_image")]
    ImageGenerationPartialImage(ImagePartialEvent),
    #[serde(rename = "image_generation.completed")]
    ImageGenerationCompleted(ImageCompletedEvent),
    #[serde(rename = "image_edit.partial_image")]
    ImageEditPartialImage(ImagePartialEvent),
    #[serde(rename = "image_edit.completed")]
    ImageEditCompleted(ImageCompletedEvent),
}

pub(super) fn image_stream_event_type<E>(value: &Value) -> Result<Option<ImageStreamEventType>, E>
where
    E: de::Error,
{
    let Some(type_name) = value.get("type").and_then(Value::as_str) else {
        return Ok(None);
    };
    serde_json::from_value(Value::String(type_name.to_owned()))
        .map(Some)
        .map_err(de::Error::custom)
}
