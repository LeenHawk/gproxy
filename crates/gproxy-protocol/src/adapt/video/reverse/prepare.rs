use super::super::resources::{bound_value, decoded, limit, read};
use super::*;

pub(super) fn split(
    input: g::PredictLongRunningRequestBody,
    binding: ReverseVideoBinding,
    effective_samples: Option<usize>,
    native_defaults: Option<video::NativeVideoDefaults>,
    expires_at: SystemTime,
    limits: VideoLimits,
) -> Result<ReverseVideoState, TransformError> {
    let input = input.into_declared();
    bound_value(&input, limits)?;
    let samples = match input.parameters.as_ref().and_then(|p| p.sample_count) {
        Some(v) if v > 0 => v as usize,
        Some(_) => {
            return Err(TransformError::shape(
                "sampleCount",
                "positive count required",
            ));
        }
        None => effective_samples
            .filter(|v| *v > 0)
            .ok_or_else(|| TransformError::missing_metadata("veo.effective_sample_count"))?,
    };
    let count = input
        .instances
        .len()
        .checked_mul(samples)
        .ok_or_else(|| limit("video.fanout"))?;
    if count == 0 || count > limits.max_resource_facts {
        return Err(limit("video.fanout"));
    }
    let mut resource_count = 0usize;
    for instance in &input.instances {
        let count = usize::from(instance.image.is_some())
            + usize::from(instance.last_frame.is_some())
            + usize::from(instance.video.is_some())
            + instance.reference_images.as_ref().map_or(0, Vec::len);
        resource_count = resource_count
            .checked_add(
                count
                    .checked_mul(samples)
                    .ok_or_else(|| limit("video.input_resources"))?,
            )
            .ok_or_else(|| limit("video.input_resources"))?;
    }
    if resource_count > limits.max_resource_facts {
        return Err(limit("video.input_resources"));
    }
    let mut children = Vec::with_capacity(count);
    for (instance_index, instance) in input.instances.iter().enumerate() {
        for sample_index in 0..samples {
            let mut source = input.clone();
            source.instances = vec![instance.clone()];
            source
                .parameters
                .get_or_insert_with(|| g::VideoGenerationParameters::builder().build())
                .sample_count = Some(1);
            children.push(ReverseVideoChild {
                instance_index,
                sample_index,
                source,
                request: None,
                started: false,
                result: None,
                published: None,
            });
        }
    }
    let state = ReverseVideoState {
        schema: 1,
        native_defaults,
        expires_at,
        binding,
        original: input,
        children,
    };
    bound_value(&state, limits)?;
    Ok(state)
}

pub(super) fn map(
    source: g::PredictLongRunningRequestBody,
    binding: &ReverseVideoBinding,
    defaults: Option<video::NativeVideoDefaults>,
    facts: &BTreeMap<String, ResolvedVideoResource>,
) -> Result<ReverseVideoRequest, TransformError> {
    Ok(match binding.kind {
        ReverseVideoKind::Native => ReverseVideoRequest::Native(
            video::gemini_to_native_request(source, &binding.model, defaults, facts)?
                .value
                .body,
        ),
        ReverseVideoKind::OpenRouter => ReverseVideoRequest::OpenRouter(
            video::gemini_to_openai_request(source, &binding.model, facts)?
                .value
                .body,
        ),
    })
}

/// Validate controls without reading or publishing media. Full resource mapping
/// runs against the original child after publication and verifies bytes/MIME.
pub(super) fn controls(
    state: &ReverseVideoState,
    defaults: Option<video::NativeVideoDefaults>,
) -> Result<(), TransformError> {
    for child in &state.children {
        let mut source = child.source.clone();
        if state.binding.kind == ReverseVideoKind::Native {
            // Native mapper checks unsupported conditioning before resource lookup.
            source.instances[0].image = None;
        } else {
            let instance = &mut source.instances[0];
            if instance.reference_images.as_ref().is_some_and(|r| {
                r.iter().any(|r| {
                    r.reference_type != Some(g::VideoReferenceType::Asset) || r.image.is_none()
                })
            }) {
                return Err(TransformError::unsupported(
                    "referenceImages",
                    "ASSET image required",
                ));
            }
            instance.image = None;
            instance.last_frame = None;
            instance.video = None;
            instance.reference_images = None;
        }
        map(source, &state.binding, defaults.clone(), &BTreeMap::new())?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn prepare<R: ResourceAccess>(
    access: &R,
    source_scope: &R::Scope,
    publish_scope: &R::Scope,
    state: &ReverseVideoState,
    index: usize,
    defaults: Option<video::NativeVideoDefaults>,
    expiry: SystemTime,
    progress: &mut ReverseVideoProgress,
    limits: VideoLimits,
    remaining: &mut u64,
) -> Result<ReverseVideoRequest, TransformError> {
    let child = &state.children[index];
    let mut source = child.source.clone();
    let instance = &mut source.instances[0];
    let mut media = Vec::new();
    for image in [&instance.image, &instance.last_frame]
        .into_iter()
        .flatten()
        .chain(
            instance
                .reference_images
                .iter()
                .flatten()
                .filter_map(|r| r.image.as_ref()),
        )
    {
        let encoded = image
            .bytes_base64_encoded
            .as_ref()
            .ok_or_else(|| TransformError::missing_metadata("video.image.bytes"))?;
        let mime = image
            .mime_type
            .as_ref()
            .ok_or_else(|| TransformError::missing_metadata("video.image.mime"))?;
        media.push((
            encoded.clone(),
            Some(encoded.clone()),
            Some(mime.clone()),
            true,
        ));
    }
    if let Some(video) = &instance.video {
        match (&video.uri, &video.encoded_video) {
            (Some(uri), None) => media.push((uri.clone(), None, video.encoding.clone(), false)),
            (None, Some(bytes)) => media.push((
                bytes.clone(),
                Some(bytes.clone()),
                video.encoding.clone(),
                false,
            )),
            _ => {
                return Err(TransformError::shape(
                    "video.input",
                    "exactly one URI or bytes required",
                ));
            }
        }
    }
    if media.len() > limits.max_resource_facts {
        return Err(limit("video.input"));
    }
    let mut facts = BTreeMap::new();
    for (ordinal, (key, encoded, mime, image)) in media.into_iter().enumerate() {
        if facts.contains_key(&key) {
            continue;
        }
        let cap = (*remaining).min(access.limits().write_bytes);
        let (bytes, mime) = if let Some(encoded) = encoded {
            let mime = mime.ok_or_else(|| TransformError::missing_metadata("video.media.mime"))?;
            (decoded(&encoded, &mime, image, cap)?, mime)
        } else {
            read(
                access,
                source_scope,
                &ResourceReference::Url(key.clone()),
                image,
                limits,
                cap,
            )
            .await?
        };
        *remaining -= bytes.len() as u64;
        let id = format!(
            "video/reverse/{}:{}/input/{index}/{ordinal}",
            state.binding.operation_name.len(),
            state.binding.operation_name
        );
        progress.publication_ids.push(id.clone());
        use base64::Engine;
        let encoded = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let published = publish(access, publish_scope, &id, bytes, &mime, expiry).await?;
        progress.publications.push(published.reference.clone());
        let ResourceReference::Url(url) = published.reference else {
            // publish() requests PublicationKind::Url, so the host returns a Url reference.
            unreachable!()
        };
        if !image {
            instance.video = Some(g::Video::builder().uri(url.clone()).build());
        }
        facts.insert(
            key.clone(),
            ResolvedVideoResource {
                reference: key,
                url: Some(url),
                bytes_base64_encoded: Some(encoded),
                mime_type: Some(mime),
            },
        );
    }
    let request = map(source, &state.binding, defaults, &facts)?;
    bound_value(&request, limits)?;
    Ok(request)
}
