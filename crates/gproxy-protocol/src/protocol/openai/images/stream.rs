use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de};
use serde_json::Value;

use super::super::common::*;
use super::responses::ImageUsage;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ImageStreamEvent {
    Known(KnownImageStreamEvent),
    Unknown(UnknownImageStreamEvent),
}

impl<'de> Deserialize<'de> for ImageStreamEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
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
pub enum KnownImageStreamEvent {
    #[serde(rename = "image_generation.partial_image")]
    ImageGenerationPartialImage {
        b64_json: String,
        partial_image_index: u32,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "image_generation.completed")]
    ImageGenerationCompleted {
        b64_json: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<ImageUsage>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "image_edit.partial_image")]
    ImageEditPartialImage {
        b64_json: String,
        partial_image_index: u32,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "image_edit.completed")]
    ImageEditCompleted {
        b64_json: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<ImageUsage>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ImageGenerationStreamEvent {
    Known(KnownImageGenerationStreamEvent),
    Unknown(UnknownImageStreamEvent),
}

impl<'de> Deserialize<'de> for ImageGenerationStreamEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        match image_stream_event_type::<D::Error>(&value)? {
            Some(ImageStreamEventType::Known(
                ImageStreamEventTypeKnown::ImageGenerationPartialImage
                | ImageStreamEventTypeKnown::ImageGenerationCompleted,
            )) => serde_json::from_value(value)
                .map(Self::Known)
                .map_err(de::Error::custom),
            Some(ImageStreamEventType::Known(_)) => Err(de::Error::custom(
                "known image edit stream event cannot deserialize as image generation stream event",
            )),
            Some(ImageStreamEventType::Unknown(_)) | None => serde_json::from_value(value)
                .map(Self::Unknown)
                .map_err(de::Error::custom),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum KnownImageGenerationStreamEvent {
    #[serde(rename = "image_generation.partial_image")]
    PartialImage {
        b64_json: String,
        partial_image_index: u32,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "image_generation.completed")]
    Completed {
        b64_json: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<ImageUsage>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ImageEditStreamEvent {
    Known(KnownImageEditStreamEvent),
    Unknown(UnknownImageStreamEvent),
}

impl<'de> Deserialize<'de> for ImageEditStreamEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        match image_stream_event_type::<D::Error>(&value)? {
            Some(ImageStreamEventType::Known(
                ImageStreamEventTypeKnown::ImageEditPartialImage
                | ImageStreamEventTypeKnown::ImageEditCompleted,
            )) => serde_json::from_value(value)
                .map(Self::Known)
                .map_err(de::Error::custom),
            Some(ImageStreamEventType::Known(_)) => Err(de::Error::custom(
                "known image generation stream event cannot deserialize as image edit stream event",
            )),
            Some(ImageStreamEventType::Unknown(_)) | None => serde_json::from_value(value)
                .map(Self::Unknown)
                .map_err(de::Error::custom),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum KnownImageEditStreamEvent {
    #[serde(rename = "image_edit.partial_image")]
    PartialImage {
        b64_json: String,
        partial_image_index: u32,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
    #[serde(rename = "image_edit.completed")]
    Completed {
        b64_json: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        usage: Option<ImageUsage>,
        #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
        extra: Extra,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnknownImageStreamEvent {
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub type_: Option<ImageStreamEventType>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

fn image_stream_event_type<E>(value: &Value) -> Result<Option<ImageStreamEventType>, E>
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
