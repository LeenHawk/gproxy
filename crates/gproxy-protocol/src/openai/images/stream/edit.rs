use serde::{Deserialize, Serialize, de};
use serde_json::Value;

use crate::openai::common::{ImageStreamEventType, ImageStreamEventTypeKnown};

use super::{
    ImageCompletedEvent, ImagePartialEvent, UnknownImageStreamEvent, image_stream_event_type,
};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
#[cfg_attr(not(feature = "exhaustive"), non_exhaustive)]
pub enum ImageEditStreamEvent {
    Known(KnownImageEditStreamEvent),
    Unknown(UnknownImageStreamEvent),
}

impl<'de> Deserialize<'de> for ImageEditStreamEvent {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        match image_stream_event_type::<D::Error>(&value)? {
            Some(ImageStreamEventType::Known(
                ImageStreamEventTypeKnown::ImageEditPartialImage
                | ImageStreamEventTypeKnown::ImageEditCompleted,
            )) => serde_json::from_value(value)
                .map(Self::Known)
                .map_err(de::Error::custom),
            Some(ImageStreamEventType::Known(_)) => Err(de::Error::custom(
                "known image generation event cannot deserialize as an image edit event",
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
pub enum KnownImageEditStreamEvent {
    #[serde(rename = "image_edit.partial_image")]
    PartialImage(ImagePartialEvent),
    #[serde(rename = "image_edit.completed")]
    Completed(ImageCompletedEvent),
}
