use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::Rest;

use super::{
    VideoContentVariant, VideoExtensionSeconds, VideoListOrder, VideoModelId, VideoSeconds,
    VideoSize,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VideoInputReference {
    File(String),
    Image(VideoImageReference),
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoImageReference {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetrieveVideoRequest {
    pub video_id: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListVideosRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<VideoListOrder>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeleteVideoRequest {
    pub video_id: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DownloadVideoContentRequest {
    pub video_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub variant: Option<VideoContentVariant>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RemixVideoRequest {
    pub video_id: String,
    pub prompt: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateVideoCharacterRequest {
    pub name: String,
    pub video: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GetVideoCharacterRequest {
    pub character_id: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EditVideoRequest {
    pub prompt: String,
    pub video: VideoReference,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExtendVideoRequest {
    pub prompt: String,
    pub seconds: VideoExtensionSeconds,
    pub video: VideoReference,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VideoReference {
    File(String),
    Existing(VideoIdReference),
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoIdReference {
    pub id: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}
