use serde::{Deserialize, Serialize, de};
use serde_json::Value;

use crate::openai::common::{ImageStreamEventType, ImageStreamEventTypeKnown};

use super::{
    ImageCompletedEvent, ImagePartialEvent, UnknownImageStreamEvent, image_stream_event_type,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ImageGenerationStreamEvent {
    Known(KnownImageGenerationStreamEvent),
    Unknown(UnknownImageStreamEvent),
}

impl<'de> Deserialize<'de> for ImageGenerationStreamEvent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        match image_stream_event_type::<D::Error>(&value)? {
            Some(ImageStreamEventType::Known(
                ImageStreamEventTypeKnown::ImageGenerationPartialImage
                | ImageStreamEventTypeKnown::ImageGenerationCompleted,
            )) => serde_json::from_value(value)
                .map(Self::Known)
                .map_err(de::Error::custom),
            Some(ImageStreamEventType::Known(_)) => Err(de::Error::custom(
                "known image edit event cannot deserialize as an image generation event",
            )),
            Some(ImageStreamEventType::Unknown(_)) | None => serde_json::from_value(value)
                .map(Self::Unknown)
                .map_err(de::Error::custom),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum KnownImageGenerationStreamEvent {
    #[serde(rename = "image_generation.partial_image")]
    PartialImage(ImagePartialEvent),
    #[serde(rename = "image_generation.completed")]
    Completed(ImageCompletedEvent),
}
