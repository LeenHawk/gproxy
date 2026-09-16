use super::request::{ResolvedVideoResource, VideoResourceRole};
use crate::{
    Rest,
    transform::TransformError,
    wire::{gemini::video as g, openai::video as o},
};
use std::collections::BTreeMap;

pub(super) fn image_resource(
    reference: &str,
    resources: &BTreeMap<String, ResolvedVideoResource>,
    role: VideoResourceRole,
) -> Result<g::VideoImage, TransformError> {
    let resource = bound(reference, resources, "image/")?;
    let _ = role;
    Ok(g::VideoImage {
        bytes_base64_encoded: Some(resource.bytes_base64_encoded.clone().ok_or_else(|| {
            TransformError::missing_metadata(format!("video resource {reference} bytes"))
        })?),
        mime_type: Some(resource.mime_type.clone().ok_or_else(|| {
            TransformError::missing_metadata(format!("video resource {reference} mime"))
        })?),
        rest: Rest::new(),
    })
}

pub(super) fn frame_role(frame: &o::FrameImage) -> VideoResourceRole {
    match frame.frame_type {
        o::FrameType::FirstFrame => VideoResourceRole::FirstFrame,
        o::FrameType::LastFrame => VideoResourceRole::LastFrame,
    }
}

pub(super) fn frame_from_resource(
    image: &g::VideoImage,
    frame_type: o::FrameType,
    resources: &BTreeMap<String, ResolvedVideoResource>,
) -> Result<o::FrameImage, TransformError> {
    Ok(o::FrameImage::builder(
        o::VideoImageType::ImageUrl,
        o::ReferenceUrl::builder(published_url(image, resources)?).build(),
        frame_type,
    )
    .build())
}

pub(super) fn published_url(
    image: &g::VideoImage,
    resources: &BTreeMap<String, ResolvedVideoResource>,
) -> Result<String, TransformError> {
    let reference = image
        .bytes_base64_encoded
        .as_deref()
        .ok_or_else(|| TransformError::missing_metadata("reference image bytes"))?;
    let resource = bound(reference, resources, "image/")?;
    if resource.bytes_base64_encoded.as_deref() != Some(reference)
        || resource.mime_type.as_ref() != image.mime_type.as_ref()
    {
        return Err(TransformError::shape(
            "image.resource",
            "published resource does not match source bytes and MIME",
        ));
    }
    url(resource
        .url
        .as_deref()
        .ok_or_else(|| TransformError::missing_metadata("published image URL"))?)
}

pub(super) fn video_url(
    video: &g::Video,
    resources: &BTreeMap<String, ResolvedVideoResource>,
) -> Result<String, TransformError> {
    if let Some(uri) = &video.uri {
        if uri.is_empty() {
            return Err(TransformError::invalid_result(
                "instances.video.uri",
                "empty video URI",
            ));
        }
        return url(uri);
    }
    let encoded = video
        .encoded_video
        .as_deref()
        .ok_or_else(|| TransformError::missing_metadata("instances.video.uri or encodedVideo"))?;
    let resource = bound(encoded, resources, "video/")?;
    if resource.bytes_base64_encoded.as_deref() != Some(encoded)
        || resource.mime_type.as_ref() != video.encoding.as_ref()
    {
        return Err(TransformError::shape(
            "video.resource",
            "published resource does not match bytes/MIME",
        ));
    }
    url(resource
        .url
        .as_deref()
        .ok_or_else(|| TransformError::missing_metadata("published video URL"))?)
}

pub(super) fn bound<'a>(
    key: &str,
    resources: &'a BTreeMap<String, ResolvedVideoResource>,
    mime_prefix: &str,
) -> Result<&'a ResolvedVideoResource, TransformError> {
    use base64::Engine;
    let resource = resources
        .get(key)
        .ok_or_else(|| TransformError::missing_metadata("bound video/image resource"))?;
    if resource.reference != key {
        return Err(TransformError::shape(
            "resource.reference",
            "resource key and declared reference disagree",
        ));
    }
    let bytes = resource
        .bytes_base64_encoded
        .as_deref()
        .ok_or_else(|| TransformError::missing_metadata("resource bytes"))?;
    base64::engine::general_purpose::STANDARD
        .decode(bytes)
        .map_err(|e| TransformError::shape("resource.bytes", e.to_string()))?;
    let mime = resource
        .mime_type
        .as_deref()
        .ok_or_else(|| TransformError::missing_metadata("resource MIME"))?;
    if !mime.starts_with(mime_prefix) {
        return Err(TransformError::shape(
            "resource MIME",
            "resource MIME differs from required media role",
        ));
    }
    Ok(resource)
}

pub(super) fn url(value: &str) -> Result<String, TransformError> {
    let uri: http::Uri = value
        .parse()
        .map_err(|e: http::uri::InvalidUri| TransformError::shape("resource.url", e.to_string()))?;
    if !matches!(uri.scheme_str(), Some("http" | "https"))
        || uri.authority().is_none()
        || uri.authority().unwrap().as_str().contains('@')
    {
        return Err(TransformError::shape(
            "resource.url",
            "absolute public HTTP(S) URL required",
        ));
    }
    Ok(value.to_owned())
}
