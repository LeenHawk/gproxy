use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::openai::common::Rest;

use super::{
    VideoDeletedObjectType, VideoListObjectType, VideoModelId, VideoObjectType, VideoSize,
    VideoStatus,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Video {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<u64>,
    pub created_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<VideoError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    pub model: VideoModelId,
    pub object: VideoObjectType,
    pub progress: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remixed_from_video_id: Option<String>,
    pub seconds: VideoSecondsValue,
    pub size: VideoSize,
    pub status: VideoStatus,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VideoSecondsValue {
    String(String),
    Raw(Value),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoError {
    pub code: String,
    pub message: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoListResponse {
    pub data: Vec<Video>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<VideoListObjectType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_id: Option<String>,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoDeleteResponse {
    pub id: String,
    pub deleted: bool,
    pub object: VideoDeletedObjectType,
    #[serde(default, flatten)]
    pub rest: Rest,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoCharacter {
    pub id: String,
    pub created_at: u64,
    pub name: String,
    #[serde(default, flatten)]
    pub rest: Rest,
}

/// Binary bytes returned by the video content endpoint.
pub type VideoContent = Vec<u8>;
