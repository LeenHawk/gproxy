use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de};

use super::super::common::Extra;
use super::{
    VideoContentVariant, VideoExtensionSeconds, VideoListOrder, VideoModelId, VideoSeconds,
    VideoSize,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct CreateVideoRequest {
    pub prompt: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_reference: Option<VideoInputReference>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<VideoModelId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seconds: Option<VideoSeconds>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<VideoSize>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

/// An image guiding video generation. Multipart files are represented by a
/// normalized string; JSON requests use exactly one of `file_id` or
/// `image_url`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum VideoInputReference {
    File(String),
    Image(VideoImageReference),
}

#[derive(Debug, Clone, PartialEq, Serialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct VideoImageReference {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

impl<'de> Deserialize<'de> for VideoImageReference {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            file_id: Option<String>,
            image_url: Option<String>,
            #[serde(default, flatten)]
            extra: Extra,
        }

        let raw = Raw::deserialize(deserializer)?;
        match (raw.file_id.is_some(), raw.image_url.is_some()) {
            (true, false) | (false, true) => Ok(Self {
                file_id: raw.file_id,
                image_url: raw.image_url,
                extra: raw.extra,
            }),
            (true, true) => Err(de::Error::custom(
                "video image reference must contain exactly one of file_id or image_url",
            )),
            (false, false) => Err(de::Error::custom(
                "video image reference must contain file_id or image_url",
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct RetrieveVideoRequest {
    pub video_id: String,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct ListVideosRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<VideoListOrder>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct DeleteVideoRequest {
    pub video_id: String,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct DownloadVideoContentRequest {
    pub video_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<VideoContentVariant>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct RemixVideoRequest {
    pub video_id: String,
    pub prompt: String,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct CreateVideoCharacterRequest {
    pub name: String,
    /// Multipart video content normalized to a `data:<mime>;base64,...` URL.
    pub video: String,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct GetVideoCharacterRequest {
    pub character_id: String,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct EditVideoRequest {
    pub prompt: String,
    pub video: VideoReference,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct ExtendVideoRequest {
    pub prompt: String,
    pub seconds: VideoExtensionSeconds,
    pub video: VideoReference,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

/// A normalized multipart upload string or a reference to an existing video.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
#[non_exhaustive]
pub enum VideoReference {
    File(String),
    Existing(VideoIdReference),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct VideoIdReference {
    pub id: String,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}
