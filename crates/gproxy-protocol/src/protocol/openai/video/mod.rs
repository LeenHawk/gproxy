mod enums;
mod requests;
mod responses;

pub use enums::*;
pub use requests::*;
pub use responses::*;

use super::common::OpenAiWireModel;
use crate::protocol::{Endpoint, HttpMethod, Operation, Provider};

pub type CreateVideoWireModel = OpenAiWireModel<CreateVideoRequest, Video>;
pub type RetrieveVideoWireModel = OpenAiWireModel<RetrieveVideoRequest, Video>;
pub type ListVideosWireModel = OpenAiWireModel<ListVideosRequest, VideoListResponse>;
pub type DeleteVideoWireModel = OpenAiWireModel<DeleteVideoRequest, VideoDeleteResponse>;
pub type DownloadVideoContentWireModel = OpenAiWireModel<DownloadVideoContentRequest, Vec<u8>>;
pub type RemixVideoWireModel = OpenAiWireModel<RemixVideoRequest, Video>;
pub type CreateVideoCharacterWireModel =
    OpenAiWireModel<CreateVideoCharacterRequest, VideoCharacter>;
pub type GetVideoCharacterWireModel = OpenAiWireModel<GetVideoCharacterRequest, VideoCharacter>;
pub type EditVideoWireModel = OpenAiWireModel<EditVideoRequest, Video>;
pub type ExtendVideoWireModel = OpenAiWireModel<ExtendVideoRequest, Video>;

/// OpenAI video endpoint metadata. Resource identifiers remain path-template
/// parameters so routers can bind them from the inbound request.
pub fn openai_video_endpoints() -> [Endpoint; 10] {
    use HttpMethod::{Delete, Get, Post};
    use Operation::{
        CreateVideo, CreateVideoCharacter, DeleteVideo, DownloadVideoContent, EditVideo,
        ExtendVideo, GetVideoCharacter, ListVideos, RemixVideo, RetrieveVideo,
    };

    [
        Endpoint::provider(CreateVideo, Provider::OpenAi, Post, "/v1/videos"),
        Endpoint::provider(
            RetrieveVideo,
            Provider::OpenAi,
            Get,
            "/v1/videos/{video_id}",
        ),
        Endpoint::provider(ListVideos, Provider::OpenAi, Get, "/v1/videos"),
        Endpoint::provider(
            DeleteVideo,
            Provider::OpenAi,
            Delete,
            "/v1/videos/{video_id}",
        ),
        Endpoint::provider(
            DownloadVideoContent,
            Provider::OpenAi,
            Get,
            "/v1/videos/{video_id}/content",
        ),
        Endpoint::provider(
            RemixVideo,
            Provider::OpenAi,
            Post,
            "/v1/videos/{video_id}/remix",
        ),
        Endpoint::provider(
            CreateVideoCharacter,
            Provider::OpenAi,
            Post,
            "/v1/videos/characters",
        ),
        Endpoint::provider(
            GetVideoCharacter,
            Provider::OpenAi,
            Get,
            "/v1/videos/characters/{character_id}",
        ),
        Endpoint::provider(EditVideo, Provider::OpenAi, Post, "/v1/videos/edits"),
        Endpoint::provider(ExtendVideo, Provider::OpenAi, Post, "/v1/videos/extensions"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_video_models_multipart_and_json_references() {
        let file: CreateVideoRequest = serde_json::from_value(serde_json::json!({
            "prompt": "A calico cat playing piano",
            "input_reference": "data:image/png;base64,AAAA",
            "model": "sora-2",
            "seconds": "8",
            "size": "1024x1792"
        }))
        .unwrap();
        assert!(matches!(
            file.input_reference,
            Some(VideoInputReference::File(_))
        ));

        let object: CreateVideoRequest = serde_json::from_value(serde_json::json!({
            "prompt": "A calico cat playing piano",
            "input_reference": {"file_id": "file_123"}
        }))
        .unwrap();
        assert!(matches!(
            object.input_reference,
            Some(VideoInputReference::Image(_))
        ));
    }

    #[test]
    fn image_reference_requires_exactly_one_source() {
        assert!(serde_json::from_value::<VideoImageReference>(serde_json::json!({})).is_err());
        assert!(
            serde_json::from_value::<VideoImageReference>(serde_json::json!({
                "file_id": "file_123",
                "image_url": "https://example.com/reference.png"
            }))
            .is_err()
        );
    }

    #[test]
    fn video_response_preserves_future_enum_values_and_fields() {
        let video: Video = serde_json::from_value(serde_json::json!({
            "id": "video_123",
            "object": "video",
            "model": "sora-next",
            "status": "paused",
            "progress": 42,
            "created_at": 1712697600,
            "size": "2048x2048",
            "seconds": "16",
            "future_field": true
        }))
        .unwrap();
        assert!(matches!(video.model, VideoModelId::Unknown(_)));
        assert!(matches!(video.status, VideoStatus::Unknown(_)));
        assert!(matches!(video.size, VideoSize::Unknown(_)));
        assert_eq!(
            video.extra.get("future_field"),
            Some(&serde_json::json!(true))
        );
    }

    #[test]
    fn extension_accepts_documented_longer_durations() {
        let request: ExtendVideoRequest = serde_json::from_value(serde_json::json!({
            "prompt": "Continue the scene",
            "seconds": "20",
            "video": {"id": "video_123"}
        }))
        .unwrap();
        assert!(matches!(
            request.seconds,
            VideoExtensionSeconds::Known(VideoExtensionSecondsKnown::Twenty)
        ));
        assert!(matches!(request.video, VideoReference::Existing(_)));
    }

    #[test]
    fn list_and_delete_responses_model_cursor_and_discriminator() {
        let list: VideoListResponse = serde_json::from_value(serde_json::json!({
            "data": [],
            "object": "list",
            "has_more": false,
            "last_id": null
        }))
        .unwrap();
        assert_eq!(list.has_more, Some(false));

        let deleted: VideoDeleteResponse = serde_json::from_value(serde_json::json!({
            "id": "video_123",
            "deleted": true,
            "object": "video.deleted"
        }))
        .unwrap();
        assert!(deleted.deleted);
    }

    #[test]
    fn endpoint_metadata_covers_every_video_operation() {
        let endpoints = openai_video_endpoints();
        assert_eq!(endpoints.len(), 10);
        assert!(endpoints.iter().all(|endpoint| {
            endpoint.provider_family() == Provider::OpenAi
                && endpoint.group() == crate::protocol::OperationGroup::Video
        }));
        assert!(endpoints.iter().any(|endpoint| {
            endpoint.operation_key.operation() == Operation::DownloadVideoContent
                && endpoint.method == HttpMethod::Get
                && endpoint.path == "/v1/videos/{video_id}/content"
        }));
        assert!(endpoints.iter().any(|endpoint| {
            endpoint.operation_key.operation() == Operation::ExtendVideo
                && endpoint.method == HttpMethod::Post
                && endpoint.path == "/v1/videos/extensions"
        }));
    }
}
