use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::super::common::Extra;
use super::{
    VideoDeletedObjectType, VideoListObjectType, VideoModelId, VideoObjectType, VideoSize,
    VideoStatus,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct Video {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<u64>,
    pub created_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<VideoCreateError>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<u64>,
    pub model: VideoModelId,
    pub object: VideoObjectType,
    pub progress: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remixed_from_video_id: Option<String>,
    pub seconds: String,
    pub size: VideoSize,
    pub status: VideoStatus,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct VideoCreateError {
    pub code: String,
    pub message: String,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct VideoListResponse {
    pub data: Vec<Video>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub object: Option<VideoListObjectType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub has_more: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_id: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct VideoDeleteResponse {
    pub id: String,
    pub deleted: bool,
    pub object: VideoDeletedObjectType,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, gproxy_protocol_macros::WireBuilder)]
#[non_exhaustive]
pub struct VideoCharacter {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub created_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: Extra,
}
