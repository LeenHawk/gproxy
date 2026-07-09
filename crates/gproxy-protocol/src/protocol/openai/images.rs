use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de, de::DeserializeOwned};
use serde_json::{Map, Value};

use super::common::*;

pub type ImageGenerationWireModel = OpenAiWireModel<ImageGenerationRequest, ImagesResponse>;
pub type ImageGenerationStreamWireModel =
    OpenAiWireModel<ImageGenerationRequest, ImageGenerationStreamEvent>;
pub type ImageEditWireModel = OpenAiWireModel<ImageEditRequest, ImagesResponse>;
pub type ImageEditStreamWireModel = OpenAiWireModel<ImageEditRequest, ImageEditStreamEvent>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ImageGenerationRequest {
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
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImageEditRequest {
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
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

impl<'de> Deserialize<'de> for ImageEditRequest {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        let Value::Object(mut map) = value else {
            return Err(de::Error::custom("image edit request must be an object"));
        };

        Ok(Self {
            images: take_image_references(&mut map).map_err(de::Error::custom)?,
            prompt: take_required(&mut map, "prompt").map_err(de::Error::custom)?,
            background: take_optional(&mut map, "background").map_err(de::Error::custom)?,
            input_fidelity: take_optional(&mut map, "input_fidelity").map_err(de::Error::custom)?,
            mask: take_optional_image_reference(&mut map, "mask").map_err(de::Error::custom)?,
            model: take_optional(&mut map, "model").map_err(de::Error::custom)?,
            moderation: take_optional(&mut map, "moderation").map_err(de::Error::custom)?,
            n: take_optional_u32(&mut map, "n").map_err(de::Error::custom)?,
            output_compression: take_optional_u32(&mut map, "output_compression")
                .map_err(de::Error::custom)?,
            output_format: take_optional(&mut map, "output_format").map_err(de::Error::custom)?,
            partial_images: take_optional_u32(&mut map, "partial_images")
                .map_err(de::Error::custom)?,
            quality: take_optional(&mut map, "quality").map_err(de::Error::custom)?,
            size: take_optional(&mut map, "size").map_err(de::Error::custom)?,
            stream: take_optional_bool(&mut map, "stream").map_err(de::Error::custom)?,
            user: take_optional(&mut map, "user").map_err(de::Error::custom)?,
            extra: map.into_iter().collect(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImageReference {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

impl<'de> Deserialize<'de> for ImageReference {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = Value::deserialize(deserializer)?;
        if let Value::String(value) = value {
            return string_image_reference(value).map_err(de::Error::custom);
        }

        #[derive(Deserialize)]
        struct RawImageReference {
            file_id: Option<String>,
            image_url: Option<String>,
            #[serde(default, flatten)]
            extra: Extra,
        }

        let raw: RawImageReference = serde_json::from_value(value).map_err(de::Error::custom)?;
        match (raw.file_id.is_some(), raw.image_url.is_some()) {
            (true, false) | (false, true) => Ok(Self {
                file_id: raw.file_id,
                image_url: raw.image_url,
                extra: raw.extra,
            }),
            (true, true) => Err(de::Error::custom(
                "image reference must contain exactly one of file_id or image_url",
            )),
            (false, false) => Err(de::Error::custom(
                "image reference must contain file_id or image_url",
            )),
        }
    }
}

fn take_required<T: DeserializeOwned>(
    map: &mut Map<String, Value>,
    key: &str,
) -> Result<T, String> {
    let Some(value) = map.remove(key) else {
        return Err(format!("missing required field `{key}`"));
    };
    serde_json::from_value(value).map_err(|e| format!("{key}: {e}"))
}

fn take_optional<T: DeserializeOwned>(
    map: &mut Map<String, Value>,
    key: &str,
) -> Result<Option<T>, String> {
    match map.remove(key) {
        Some(Value::Null) | None => Ok(None),
        Some(value) => serde_json::from_value(value)
            .map(Some)
            .map_err(|e| format!("{key}: {e}")),
    }
}

fn take_optional_u32(map: &mut Map<String, Value>, key: &str) -> Result<Option<u32>, String> {
    match map.remove(key) {
        Some(Value::Null) | None => Ok(None),
        Some(Value::String(value)) => value
            .parse::<u32>()
            .map(Some)
            .map_err(|e| format!("{key}: {e}")),
        Some(value) => serde_json::from_value(value)
            .map(Some)
            .map_err(|e| format!("{key}: {e}")),
    }
}

fn take_optional_bool(map: &mut Map<String, Value>, key: &str) -> Result<Option<bool>, String> {
    match map.remove(key) {
        Some(Value::Null) | None => Ok(None),
        Some(Value::String(value)) => value
            .parse::<bool>()
            .map(Some)
            .map_err(|e| format!("{key}: {e}")),
        Some(value) => serde_json::from_value(value)
            .map(Some)
            .map_err(|e| format!("{key}: {e}")),
    }
}

fn take_image_references(map: &mut Map<String, Value>) -> Result<Vec<ImageReference>, String> {
    let mut images = Vec::new();
    if let Some(value) = map.remove("image") {
        images.extend(image_references_from_value(value)?);
    }
    if let Some(value) = map.remove("images") {
        images.extend(image_references_from_value(value)?);
    }
    if images.is_empty() {
        return Err("missing required field `images`".to_owned());
    }
    Ok(images)
}

fn take_optional_image_reference(
    map: &mut Map<String, Value>,
    key: &str,
) -> Result<Option<ImageReference>, String> {
    match map.remove(key) {
        Some(Value::Null) | None => Ok(None),
        Some(value) => serde_json::from_value(value)
            .map(Some)
            .map_err(|e| format!("{key}: {e}")),
    }
}

fn image_references_from_value(value: Value) -> Result<Vec<ImageReference>, String> {
    match value {
        Value::Array(values) => values
            .into_iter()
            .map(|value| serde_json::from_value(value).map_err(|e| e.to_string()))
            .collect(),
        value => serde_json::from_value(value)
            .map(|reference| vec![reference])
            .map_err(|e| e.to_string()),
    }
}

fn string_image_reference(value: String) -> Result<ImageReference, String> {
    if value.trim().is_empty() {
        return Err("image reference string must not be empty".to_owned());
    }
    if value.starts_with("http://") || value.starts_with("https://") || value.starts_with("data:") {
        Ok(ImageReference {
            file_id: None,
            image_url: Some(value),
            extra: Default::default(),
        })
    } else {
        Ok(ImageReference {
            file_id: Some(value),
            image_url: None,
            extra: Default::default(),
        })
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_edit_accepts_generic_multipart_json_shape() {
        let req: ImageEditRequest = serde_json::from_str(
            r#"{
                "image": [
                    "data:image/png;base64,AAAA",
                    "file_123"
                ],
                "mask": "data:image/png;base64,BBBB",
                "prompt": "make it blue",
                "model": "gpt-image-1.5",
                "n": "2",
                "stream": "true"
            }"#,
        )
        .unwrap();

        assert_eq!(req.images.len(), 2);
        assert_eq!(
            req.images[0].image_url.as_deref(),
            Some("data:image/png;base64,AAAA")
        );
        assert_eq!(req.images[1].file_id.as_deref(), Some("file_123"));
        assert_eq!(
            req.mask.as_ref().and_then(|mask| mask.image_url.as_deref()),
            Some("data:image/png;base64,BBBB")
        );
        assert_eq!(req.n, Some(2));
        assert_eq!(req.stream, Some(true));
    }
}
