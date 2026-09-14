use super::super::resources::{bound_value, limit, read};
use super::*;
pub(super) fn validate(
    state: &ReverseVideoState,
    index: usize,
    result: &ReverseVideoResult,
) -> Result<(), TransformError> {
    let binding = &state.binding;
    let path = binding.query_path(result.id())?;
    let child = &state.children[index];
    match (result, child.request.as_ref()) {
        (ReverseVideoResult::Native(v), Some(ReverseVideoRequest::Native(request))) => {
            if v.created_at < 0
                || !(0..=100).contains(&v.progress)
                || (v.status == o::NativeVideoStatus::Completed
                    && (v.progress != 100 || v.error.as_ref().and_then(|v| v.as_ref()).is_some()))
                || binding.kind != ReverseVideoKind::Native
                || v.model != binding.model
                || request
                    .seconds
                    .map(|v| serde_json::to_value(v).expect("enum"))
                    .as_ref()
                    != Some(&serde_json::Value::String(v.seconds.clone()))
                || request.size != Some(v.size)
            {
                return Err(TransformError::invalid_result(
                    "video.result.metadata",
                    "native model/seconds/size differ from saved request",
                ));
            }
            if v.status != o::NativeVideoStatus::Completed {
                project(result.clone(), binding, None)?;
            }
            if let Some(ReverseVideoResult::Native(old)) = &child.result
                && (v.created_at != old.created_at || v.progress < old.progress)
            {
                return Err(TransformError::invalid_result(
                    "video.result.lifecycle",
                    "native identity facts or progress regressed",
                ));
            }
        }
        (ReverseVideoResult::OpenRouter(v), Some(ReverseVideoRequest::OpenRouter(_))) => {
            if binding.kind != ReverseVideoKind::OpenRouter
                || v.polling_url != format!("{}{path}", binding.api_origin_url)
            {
                return Err(TransformError::invalid_result(
                    "video.polling_url",
                    "returned polling URL differs from selected origin and resource path",
                ));
            }
        }
        _ => {
            return Err(TransformError::shape(
                "video.result.kind",
                "result differs from prepared target wire",
            ));
        }
    }
    if matches!(result, ReverseVideoResult::OpenRouter(_)) {
        let converted = project(result.clone(), binding, None)?;
        if converted
            .value
            .response
            .as_ref()
            .and_then(|r| r.generate_video_response.as_ref())
            .and_then(|r| r.generated_samples.as_ref())
            .is_some_and(|v| v.len() != 1)
        {
            return Err(TransformError::invalid_result(
                "video.sample_count",
                "each one-sample child must return exactly one result",
            ));
        }
    }
    if child
        .result
        .as_ref()
        .is_some_and(|old| match (old, result) {
            (ReverseVideoResult::Native(a), ReverseVideoResult::Native(b)) => {
                a.status == o::NativeVideoStatus::InProgress
                    && b.status == o::NativeVideoStatus::Queued
            }
            (ReverseVideoResult::OpenRouter(a), ReverseVideoResult::OpenRouter(b)) => {
                a.status == o::VideoStatus::InProgress && b.status == o::VideoStatus::Pending
            }
            _ => false,
        })
    {
        return Err(TransformError::invalid_result(
            "video.result.lifecycle",
            "running child returned to the initial queue",
        ));
    }
    if child.result.as_ref().is_some_and(|old| {
        old.id() != result.id() || old.terminal() && !old.same_projection(result)
    }) {
        return Err(TransformError::invalid_result(
            "video.result.lifecycle",
            "identity or terminal result changed",
        ));
    }
    if state
        .children
        .iter()
        .enumerate()
        .any(|(i, c)| i != index && c.result.as_ref().is_some_and(|v| v.id() == result.id()))
    {
        return Err(TransformError::invalid_result(
            "video.child.id",
            "different creates returned the same resource identity",
        ));
    }
    Ok(())
}
fn project(
    result: ReverseVideoResult,
    binding: &ReverseVideoBinding,
    video: Option<g::Video>,
) -> Result<Converted<g::VideoOperation>, TransformError> {
    Ok(match result {
        ReverseVideoResult::Native(v) => video::native_to_gemini_operation(
            v.clone(),
            video::NativeToVeoContext {
                source_id: v.id,
                operation_name: binding.operation_name.clone(),
                video,
            },
        )?,
        ReverseVideoResult::OpenRouter(v) => video::openai_response_to_gemini_operation(
            v.clone(),
            &GeminiOperationContext {
                source_id: v.id,
                source_polling_url: v.polling_url,
                operation_name: binding.operation_name.clone(),
            },
        )?,
    })
}
#[allow(clippy::too_many_arguments)]
pub(super) async fn finish<R: ResourceAccess, S: StateStore>(
    access: &R,
    target_scope: &R::Scope,
    publish_scope: &R::Scope,
    store: &S,
    state_scope: &S::Scope,
    state: &mut ReverseVideoState,
    expiry: SystemTime,
    progress: &mut ReverseVideoProgress,
    limits: VideoLimits,
) -> Result<Converted<g::VideoOperation>, TransformError> {
    let mut report = Report::default();
    let mut remaining = limits.codec.max_body_bytes;
    for index in 0..state.children.len() {
        let result = state.children[index]
            .result
            .clone()
            .ok_or_else(|| conflict("video.child.result"))?;
        validate(state, index, &result)?;
        if let Some(published) = &state.children[index].published {
            let video = published
                .response
                .as_ref()
                .and_then(|r| r.generate_video_response.as_ref())
                .and_then(|r| r.generated_samples.as_ref())
                .and_then(|s| s.first())
                .and_then(|s| s.video.clone());
            report
                .diagnostics
                .append(&mut project(result, &state.binding, video)?.report.diagnostics);
            continue;
        }
        let video = if let ReverseVideoResult::Native(v) = &result {
            if v.status == o::NativeVideoStatus::Completed {
                Some(
                    g::Video::builder()
                        .uri(format!(
                            "{}{}/content",
                            state.binding.api_origin_url,
                            state.binding.query_path(&v.id)?
                        ))
                        .build(),
                )
            } else {
                None
            }
        } else {
            None
        };
        // This URI names the actual selected native content endpoint. It is read
        // below and never exposed as a Gemini media URL without publication.
        let mut converted = project(result, &state.binding, video)?;
        report.diagnostics.append(&mut converted.report.diagnostics);
        if let Some(samples) = converted
            .value
            .response
            .as_mut()
            .and_then(|r| r.generate_video_response.as_mut())
            .and_then(|r| r.generated_samples.as_mut())
        {
            if samples.len() != 1 {
                return Err(TransformError::invalid_result(
                    "video.sample_count",
                    "each one-sample child must return exactly one result",
                ));
            }
            for (sample_index, sample) in samples.iter_mut().enumerate() {
                let uri = sample
                    .video
                    .as_ref()
                    .and_then(|v| v.uri.as_ref())
                    .ok_or_else(|| TransformError::missing_metadata("video.result.uri"))?;
                let (bytes, mime) = read(
                    access,
                    target_scope,
                    &ResourceReference::Url(uri.clone()),
                    false,
                    limits,
                    remaining.min(access.limits().write_bytes),
                )
                .await?;
                remaining -= bytes.len() as u64;
                let id = format!(
                    "video/reverse/{}:{}/output/{index}/{sample_index}",
                    state.binding.operation_name.len(),
                    state.binding.operation_name
                );
                progress.publication_ids.push(id.clone());
                let published = publish(access, publish_scope, &id, bytes, &mime, expiry).await?;
                progress.publications.push(published.reference.clone());
                let ResourceReference::Url(url) = published.reference else {
                    unreachable!()
                };
                sample.video = Some(g::Video::builder().uri(url).encoding(mime).build());
            }
        }
        state.children[index].published = Some(converted.value);
        save(store, state_scope, state, progress, limits).await?;
    }
    let mut output = g::VideoOperation::builder()
        .name(state.binding.operation_name.clone())
        .done(false)
        .build();
    let all_done = state
        .children
        .iter()
        .all(|c| c.published.as_ref().is_some_and(|v| v.done == Some(true)));
    if all_done {
        output.done = Some(true);
        if let Some(error) = state
            .children
            .iter()
            .find_map(|c| c.published.as_ref().and_then(|v| v.error.as_ref()))
        {
            output.error = Some(error.clone());
            report.omitted("video.fanout.partial_results","child outcomes remain in durable state; a failed group does not advertise successful completeness");
        } else {
            let mut samples = Vec::new();
            for child in &state.children {
                let values = child
                    .published
                    .as_ref()
                    .and_then(|v| v.response.as_ref())
                    .and_then(|v| v.generate_video_response.as_ref())
                    .and_then(|v| v.generated_samples.as_ref())
                    .filter(|v| v.len() == 1)
                    .ok_or_else(|| {
                        TransformError::invalid_result(
                            "video.fanout",
                            "completed child lacks its one requested sample",
                        )
                    })?;
                samples.extend(values.iter().cloned());
            }
            if samples.len() > limits.max_resource_facts {
                return Err(limit("video.outputs"));
            }
            output.response = Some(
                g::VideoOperationResponse::builder()
                    .generate_video_response(
                        g::GenerateVideoResponse::builder()
                            .generated_samples(samples)
                            .build(),
                    )
                    .build(),
            );
        }
    }
    bound_value(&output, limits)?;
    Ok(Converted {
        value: output,
        report,
    })
}
