use http::Method;

use crate::match_ingress;
use crate::operation::{Operation, OperationGroup};
use crate::spec::{Affinity, Seg, SettleMode, streaming_sibling};
use crate::specs::REGISTRY;

#[test]
fn ingress_registry_matches_canonical_paths() {
    for (operation, spec) in REGISTRY.iter() {
        assert!(std::ptr::eq(operation.spec(), spec));

        for ingress in spec.ingress {
            let (path, expected_params) = example_path(ingress.pattern.0);
            let matched = match_ingress(ingress.method, &path)
                .unwrap_or_else(|| panic!("registry path did not match: {path}"));

            assert_eq!(matched.operation, *operation, "path: {path}");
            assert_eq!(matched.kind, ingress.kind, "path: {path}");
            assert_eq!(matched.stream, ingress.stream, "path: {path}");
            assert_eq!(matched.upgrade, ingress.upgrade, "path: {path}");
            assert_eq!(matched.params, expected_params, "path: {path}");
        }
    }

    for (method, path) in [
        (&Method::PATCH, "/v1/models"),
        (&Method::GET, "v1/models"),
        (&Method::GET, "/v1/models/"),
        (&Method::GET, "/v1//models"),
        (&Method::GET, "/v1/models/id/extra"),
        (&Method::GET, "/v1/videos/id/extra"),
        (&Method::GET, "/v1/videos/id/remix"),
        (&Method::POST, "/v1/videos/id/content"),
        (&Method::DELETE, "/v1/videos/characters/id"),
        (&Method::POST, "/v1beta/models/:generateContent"),
        (&Method::POST, "/v1beta/models/model:wrongAction"),
    ] {
        assert!(
            match_ingress(method, path).is_none(),
            "invalid path matched: {path}"
        );
    }

    let promotions: Vec<_> = REGISTRY
        .iter()
        .filter_map(|(operation, _)| streaming_sibling(*operation).map(|next| (*operation, next)))
        .collect();
    assert_eq!(
        promotions,
        vec![(Operation::GenerateContent, Operation::StreamGenerateContent)]
    );
    assert_eq!(streaming_sibling(Operation::CreateImage), None);
    assert_eq!(streaming_sibling(Operation::EditImage), None);
}

#[test]
fn video_specs_keep_job_settlement_and_resource_affinity() {
    use Operation::*;

    for operation in [
        CreateVideo,
        RetrieveVideo,
        ListVideos,
        DeleteVideo,
        DownloadVideoContent,
        RemixVideo,
        CreateVideoCharacter,
        GetVideoCharacter,
        EditVideo,
        ExtendVideo,
    ] {
        assert_eq!(operation.group(), OperationGroup::Video);
    }

    assert_eq!(RetrieveVideo.spec().settle, SettleMode::OnCompletedStatus);
    assert_eq!(RetrieveVideo.spec().affinity, Affinity::Resource("video"));
    assert_eq!(ListVideos.spec().settle, SettleMode::Free);
    assert_eq!(ListVideos.spec().affinity, Affinity::Resource("video"));
    for operation in [
        CreateVideo,
        ListVideos,
        DeleteVideo,
        DownloadVideoContent,
        RemixVideo,
        EditVideo,
        ExtendVideo,
    ] {
        assert_eq!(operation.spec().settle, SettleMode::Free);
        assert_eq!(operation.spec().affinity, Affinity::Resource("video"));
    }
    for operation in [CreateVideoCharacter, GetVideoCharacter] {
        assert_eq!(operation.spec().settle, SettleMode::Free);
        assert_eq!(
            operation.spec().affinity,
            Affinity::Resource("video_character")
        );
    }
}

fn example_path(pattern: &[Seg]) -> (String, Vec<(&'static str, String)>) {
    let mut segments = Vec::with_capacity(pattern.len());
    let mut params = Vec::new();

    for segment in pattern {
        match segment {
            Seg::Lit(value) => segments.push((*value).to_owned()),
            Seg::Param(name) => {
                let value = format!("{name}-value");
                segments.push(value.clone());
                params.push((*name, value));
            }
            Seg::ParamAction(name, action) => {
                let value = format!("{name}-value");
                segments.push(format!("{value}:{action}"));
                params.push((*name, value));
            }
            Seg::Rest(name) => {
                let value = format!("{name}-first/{name}-second");
                segments.push(value.clone());
                params.push((*name, value));
            }
        }
    }

    (format!("/{}", segments.join("/")), params)
}
