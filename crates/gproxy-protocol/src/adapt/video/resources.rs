use super::*;
use crate::{
    HttpBody,
    capability::{
        PublicationKind, PublishedResource, ResourceAccess, ResourceMetadata, ResourceReference,
    },
    transform::TransformErrorKind,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::Serialize;
use std::time::SystemTime;

pub(super) fn limit(path: &str) -> TransformError {
    TransformError::new(
        TransformErrorKind::Limit,
        path,
        "video byte or resource bound exceeded",
    )
}

pub(super) fn bound_value<T: Serialize>(
    value: &T,
    limits: VideoLimits,
) -> Result<(), TransformError> {
    crate::codec::encode_json(value, limits.codec)
        .map(|_| ())
        .map_err(|e| TransformError::new(TransformErrorKind::Limit, "video.value", e.to_string()))
}

fn byte_limits(limits: VideoLimits, cap: u64) -> CodecLimits {
    let mut out = limits.codec;
    out.max_body_bytes = out.max_body_bytes.min(cap);
    out.max_buffer_bytes = out.max_buffer_bytes.min(out.max_body_bytes);
    out
}

fn check_media(bytes: &[u8], mime: &str, image: bool) -> Result<(), TransformError> {
    if image {
        let actual = crate::transform::images::inspect_image(bytes)?;
        if actual.mime() != mime {
            return Err(TransformError::invalid_result(
                "video.image.mime",
                "MIME differs from actual image",
            ));
        }
    } else if mime != "video/mp4" || bytes.len() < 12 || &bytes[4..8] != b"ftyp" {
        return Err(TransformError::invalid_result(
            "video.mime",
            "MP4 MIME and ISO BMFF file header required",
        ));
    }
    Ok(())
}

pub(super) async fn read<R: ResourceAccess>(
    access: &R,
    scope: &R::Scope,
    reference: &ResourceReference,
    image: bool,
    limits: VideoLimits,
    remaining: u64,
) -> Result<(bytes::Bytes, String), TransformError> {
    let read = access.read(scope, reference).await?;
    let cap = remaining
        .min(access.limits().read_bytes)
        .min(limits.codec.max_body_bytes);
    if read.metadata.length.is_some_and(|v| v > cap) {
        return Err(limit("video.resource.length"));
    }
    let bytes = crate::codec::read_http_body(read.body, byte_limits(limits, cap))
        .await
        .map_err(|e| {
            TransformError::new(
                if e.kind() == crate::codec::CodecErrorKind::Limit {
                    TransformErrorKind::Limit
                } else {
                    TransformErrorKind::Host
                },
                "video.resource.read",
                e.to_string(),
            )
        })?;
    if read
        .metadata
        .length
        .is_some_and(|v| v != bytes.len() as u64)
    {
        return Err(TransformError::invalid_result(
            "video.resource.length",
            "metadata length differs from body",
        ));
    }
    let mime = read
        .metadata
        .mime
        .ok_or_else(|| TransformError::missing_metadata("video.resource.mime"))?;
    check_media(&bytes, &mime, image)?;
    Ok((bytes, mime))
}

pub(super) async fn resolve_resources<R: ResourceAccess>(
    access: &R,
    scope: &R::Scope,
    input: &o::CreateVideoRequestBody,
    limits: VideoLimits,
) -> Result<BTreeMap<String, ResolvedVideoResource>, TransformError> {
    let needs = video::resource_needs(input);

    let mut out = BTreeMap::new();
    let mut remaining = limits.codec.max_body_bytes;
    for need in needs {
        let image = match need.role {
            video::VideoResourceRole::ReferenceAudio => {
                return Err(TransformError::unsupported(
                    "video.audio",
                    "Veo has no audio reference input",
                ));
            }
            video::VideoResourceRole::ReferenceVideo => false,
            _ => true,
        };
        if let Some(old) = out.get(&need.reference) {
            let old: &ResolvedVideoResource = old;
            if old
                .mime_type
                .as_deref()
                .is_some_and(|m| m.starts_with("image/"))
                != image
            {
                return Err(TransformError::shape(
                    "video.resource.role",
                    "same resource used with conflicting media roles",
                ));
            }
            continue;
        }
        let reference = ResourceReference::Url(need.reference.clone());
        let (bytes, mime) = read(access, scope, &reference, image, limits, remaining).await?;
        remaining -= bytes.len() as u64;
        out.insert(
            need.reference.clone(),
            ResolvedVideoResource {
                reference: need.reference,
                url: None,
                bytes_base64_encoded: Some(STANDARD.encode(bytes)),
                mime_type: Some(mime),
            },
        );
    }
    Ok(out)
}

pub(super) fn decoded(
    encoded: &str,
    mime: &str,
    image: bool,
    cap: u64,
) -> Result<bytes::Bytes, TransformError> {
    // Check encoded length before allocating. Padding may add at most three bytes.
    if encoded.len() as u64 > cap.saturating_add(2).saturating_div(3).saturating_mul(4) {
        return Err(limit("video.base64"));
    }
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|e| TransformError::shape("video.base64", e.to_string()))?;
    if bytes.len() as u64 > cap {
        return Err(limit("video.decoded"));
    }
    check_media(&bytes, mime, image)?;
    Ok(bytes.into())
}

pub(super) async fn publish<R: ResourceAccess>(
    access: &R,
    scope: &R::Scope,
    id: &str,
    bytes: bytes::Bytes,
    mime: &str,
    expires_at: SystemTime,
) -> Result<PublishedResource<R::PublishedHandle>, TransformError> {
    let length = bytes.len() as u64;
    let out = access
        .publish(
            scope,
            id,
            PublicationKind::Url,
            ResourceMetadata {
                mime: Some(mime.into()),
                length: Some(length),
                filename: Some(
                    if mime.starts_with("video/") {
                        "video.mp4"
                    } else {
                        "reference-image"
                    }
                    .into(),
                ),
                expires_at: Some(expires_at),
            },
            HttpBody::Bytes(bytes),
        )
        .await?;
    let ResourceReference::Url(url) = &out.reference else {
        return Err(TransformError::invalid_result(
            "video.publication",
            "host returned wrong publication kind",
        ));
    };
    super::state::public_url(url)?;
    if out.metadata.mime.as_deref() != Some(mime)
        || out.metadata.length != Some(length)
        || out
            .metadata
            .expires_at
            .is_none_or(|e| e <= SystemTime::now())
    {
        return Err(TransformError::invalid_result(
            "video.publication.metadata",
            "publication differs from requested bytes/MIME or has expired",
        ));
    }
    Ok(out)
}

pub async fn publish_video_output<R: ResourceAccess>(
    access: &R,
    scope: &R::Scope,
    operation_id: &str,
    encoded_base64: &str,
    mime: &str,
    expires_at: SystemTime,
    limits: VideoLimits,
) -> Result<PublishedResource<R::PublishedHandle>, TransformError> {
    if operation_id.is_empty() || expires_at <= SystemTime::now() {
        return Err(TransformError::shape(
            "video.publish",
            "nonempty operation ID and future expiry required",
        ));
    }
    let bytes = decoded(
        encoded_base64,
        mime,
        false,
        limits.codec.max_body_bytes.min(access.limits().write_bytes),
    )?;
    publish(access, scope, operation_id, bytes, mime, expires_at).await
}

/// Every output URI is resolved in the selected upstream resource scope. The
/// returned client URLs come from explicit host publication, including when an
/// upstream URI is private or authenticated.
pub(super) async fn outputs<R: ResourceAccess>(
    access: &R,
    scope: &R::Scope,
    operation: &g::VideoOperation,
    binding: &VideoBinding,
    expires_at: SystemTime,
    progress: &mut VideoProgress,
    limits: VideoLimits,
) -> Result<BTreeMap<String, ResolvedVideoResource>, TransformError> {
    let samples = operation
        .response
        .as_ref()
        .and_then(|r| r.generate_video_response.as_ref())
        .and_then(|r| r.generated_samples.as_ref());
    let Some(samples) = samples else {
        return Ok(BTreeMap::new());
    };

    let mut remaining = limits.codec.max_body_bytes;
    let mut out = BTreeMap::new();
    for (index, sample) in samples.iter().enumerate() {
        let video = sample
            .video
            .as_ref()
            .ok_or_else(|| TransformError::invalid_result("video.sample", "missing video"))?;
        if video.uri.is_some() && video.encoded_video.is_some() {
            return Err(TransformError::invalid_result(
                "video.sample",
                "both URI and bytes present",
            ));
        }
        let (key, bytes, mime) = if let Some(uri) = &video.uri {
            let (bytes, mime) = read(
                access,
                scope,
                &ResourceReference::Url(uri.clone()),
                false,
                limits,
                remaining,
            )
            .await?;
            (uri.clone(), bytes, mime)
        } else {
            let encoded = video
                .encoded_video
                .as_ref()
                .ok_or_else(|| TransformError::missing_metadata("video.output.bytes"))?;
            let mime = video
                .encoding
                .as_deref()
                .ok_or_else(|| TransformError::missing_metadata("video.output.mime"))?;
            (
                encoded.clone(),
                decoded(
                    encoded,
                    mime,
                    false,
                    remaining.min(access.limits().write_bytes),
                )?,
                mime.to_owned(),
            )
        };
        remaining -= bytes.len() as u64;
        let id = format!(
            "video/output/{}:{}/{index}",
            binding.client_id.len(),
            binding.client_id
        );
        progress.publication_ids.push(id.clone());
        let encoded = STANDARD.encode(&bytes);
        if bytes.len() as u64 > access.limits().write_bytes {
            return Err(limit("video.publication"));
        }
        let published = publish(access, scope, &id, bytes, &mime, expires_at).await?;
        progress.publications.push(published.reference.clone());
        let ResourceReference::Url(url) = published.reference else {
            unreachable!("validated")
        };
        out.insert(
            key.clone(),
            ResolvedVideoResource {
                reference: key,
                url: Some(url),
                bytes_base64_encoded: Some(encoded),
                mime_type: Some(mime),
            },
        );
    }
    Ok(out)
}
