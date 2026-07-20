use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de, de::DeserializeOwned};
use serde_json::{Map, Value};

use super::super::common::*;

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
