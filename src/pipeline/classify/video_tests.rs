use bytes::Bytes;
use http::{HeaderMap, Method};

use super::classify;
use crate::protocol::{Operation, OperationKind, Provider};

#[test]
fn video_paths_classify_by_method_and_resource_shape() {
    for (method, path, operation) in [
        (Method::POST, "/v1/videos", Operation::CreateVideo),
        (Method::GET, "/v1/videos", Operation::ListVideos),
        (
            Method::GET,
            "/v1/videos/video_123",
            Operation::RetrieveVideo,
        ),
        (
            Method::DELETE,
            "/v1/videos/video_123",
            Operation::DeleteVideo,
        ),
        (
            Method::GET,
            "/v1/videos/video_123/content",
            Operation::DownloadVideoContent,
        ),
        (
            Method::POST,
            "/v1/videos/video_123/remix",
            Operation::RemixVideo,
        ),
        (
            Method::POST,
            "/v1/videos/characters",
            Operation::CreateVideoCharacter,
        ),
        (
            Method::GET,
            "/v1/videos/characters/char_123",
            Operation::GetVideoCharacter,
        ),
        (Method::POST, "/v1/videos/edits", Operation::EditVideo),
        (
            Method::POST,
            "/v1/videos/extensions",
            Operation::ExtendVideo,
        ),
    ] {
        let classified = classify(
            &method,
            path,
            &HeaderMap::new(),
            &Bytes::from_static(br#"{"model":"sora-2"}"#),
        )
        .unwrap();
        assert_eq!(classified.op.operation(), operation, "{method} {path}");
        assert_eq!(
            classified.op.kind(),
            OperationKind::Provider(Provider::OpenAi)
        );
        assert!(!classified.stream);
    }
}

#[test]
fn video_paths_reject_extra_segments_and_wrong_methods() {
    for (method, path) in [
        (Method::GET, "/v1/videos/video_123/remix"),
        (Method::POST, "/v1/videos/video_123/content"),
        (Method::GET, "/v1/videos/video_123/extra"),
        (Method::DELETE, "/v1/videos/characters/char_123"),
    ] {
        assert!(
            classify(&method, path, &HeaderMap::new(), &Bytes::new()).is_err(),
            "{method} {path}"
        );
    }
}
