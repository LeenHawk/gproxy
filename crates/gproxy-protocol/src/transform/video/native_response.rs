use crate::{
    transform::{Converted, Report, TransformError},
    wire::{DeclaredFields, gemini::video as g, openai::video as o},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum NativePendingStatus {
    Queued,
    InProgress,
}
/// Facts recorded by the invocation or measured from its result. Veo does not
/// provide native Sora's required timestamps, size, duration or percentage.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct NativeVideoResponseFacts {
    pub operation_name: String,
    pub client_id: String,
    pub created_at: i64,
    pub model: String,
    pub seconds: o::NativeVideoSeconds,
    pub size: o::NativeVideoSize,
    pub progress: i64,
    pub pending_status: NativePendingStatus,
    pub completed_at: Option<i64>,
    pub expires_at: Option<i64>,
    pub prompt: Option<String>,
    /// A caller-supplied real classification when a Google error has no code.
    pub failure_code: Option<String>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct NativeVeoResult {
    pub body: o::NativeVideo,
    /// Must be retained for the native content endpoint before exposing `id`.
    pub videos: Vec<g::Video>,
}
pub fn gemini_operation_to_native(
    input: g::VideoOperation,
    facts: &NativeVideoResponseFacts,
) -> Result<Converted<NativeVeoResult>, TransformError> {
    let input = input.into_declared();
    super::response::operation_name(&facts.operation_name)?;
    if input.name.as_deref() != Some(&facts.operation_name)
        || facts.client_id.is_empty()
        || facts.model.is_empty()
    {
        return Err(TransformError::shape(
            "video.binding",
            "actual operation and client identity/model required",
        ));
    }
    if facts.created_at < 0
        || !(0..=100).contains(&facts.progress)
        || facts.completed_at.is_some_and(|v| v < facts.created_at)
        || facts.expires_at.is_some_and(|v| v < facts.created_at)
    {
        return Err(TransformError::shape(
            "video.facts",
            "invalid time/progress facts",
        ));
    }
    if input.error.is_some() && input.response.is_some()
        || input.done != Some(true) && (input.error.is_some() || input.response.is_some())
    {
        return Err(TransformError::invalid_result(
            "video.operation",
            "inconsistent operation lifecycle",
        ));
    }
    let mut report = Report::default();
    let mut videos = Vec::new();
    let mut error = None;
    let status = if let Some(source) = input.error {
        let code = source
            .code
            .map(|v| v.to_string())
            .or_else(|| facts.failure_code.clone())
            .filter(|v| !v.is_empty())
            .ok_or_else(|| TransformError::missing_metadata("native.error.code"))?;
        let message = source
            .message
            .filter(|v| !v.is_empty())
            .ok_or_else(|| TransformError::missing_metadata("native.error.message"))?;
        if source.details.is_some() {
            report.omitted(
                "error.details",
                "native Sora error lacks Google RPC detail variants",
            );
        }
        error = Some(Some(o::NativeVideoError::builder(code, message).build()));
        o::NativeVideoStatus::Failed
    } else if input.done == Some(true) {
        let result = input
            .response
            .and_then(|v| v.generate_video_response)
            .ok_or_else(|| {
                TransformError::invalid_result(
                    "video.response",
                    "completed operation has no generateVideoResponse",
                )
            })?;
        let filtered = result.rai_media_filtered_count.unwrap_or(0);
        if filtered < 0 {
            return Err(TransformError::invalid_result(
                "raiMediaFilteredCount",
                "negative count",
            ));
        }
        for sample in result.generated_samples.unwrap_or_default() {
            let video = sample.video.ok_or_else(|| {
                TransformError::invalid_result("generatedSamples.video", "missing video")
            })?;
            match (&video.uri, &video.encoded_video) {
                (Some(uri), None) if !uri.is_empty() => {}
                (None, Some(bytes))
                    if !bytes.is_empty()
                        && video.encoding.as_ref().is_some_and(|m| !m.is_empty()) => {}
                _ => {
                    return Err(TransformError::invalid_result(
                        "video.result",
                        "one nonempty URI or bytes with MIME required",
                    ));
                }
            };
            videos.push(video);
        }
        if videos.len() > 1 {
            return Err(TransformError::unsupported(
                "generatedSamples",
                "native Sora exposes one video per ID; sample fanout binding required",
            ));
        }
        if videos.is_empty() {
            if filtered == 0 {
                return Err(TransformError::invalid_result(
                    "generatedSamples",
                    "completed operation produced no result",
                ));
            }
            let message = result
                .rai_media_filtered_reasons
                .filter(|v| !v.is_empty())
                .map(|v| v.join("; "))
                .unwrap_or_else(|| format!("{filtered} video samples filtered"));
            error = Some(Some(
                o::NativeVideoError::builder("content_filter".into(), message).build(),
            ));
            o::NativeVideoStatus::Failed
        } else {
            if filtered > 0 || result.rai_media_filtered_reasons.is_some() {
                report.omitted(
                    "rai_filtering",
                    "native Sora lacks partial filter count/reasons; generated video retained",
                );
            }
            if facts.progress != 100 {
                return Err(TransformError::shape(
                    "video.progress",
                    "completed result contradicts supplied progress",
                ));
            }
            o::NativeVideoStatus::Completed
        }
    } else {
        match facts.pending_status {
            NativePendingStatus::Queued => o::NativeVideoStatus::Queued,
            NativePendingStatus::InProgress => o::NativeVideoStatus::InProgress,
        }
    };
    if matches!(
        status,
        o::NativeVideoStatus::Queued | o::NativeVideoStatus::InProgress
    ) && facts.completed_at.is_some()
    {
        return Err(TransformError::shape(
            "video.completed_at",
            "pending operation cannot have a completion timestamp",
        ));
    }
    if input.metadata.is_some() {
        report.omitted(
            "metadata",
            "native Sora has no arbitrary operation metadata field",
        );
    }
    let seconds = match facts.seconds {
        o::NativeVideoSeconds::Four => "4",
        o::NativeVideoSeconds::Eight => "8",
        o::NativeVideoSeconds::Twelve => "12",
    };
    let mut body = o::NativeVideo::builder(
        facts.client_id.clone(),
        facts.created_at,
        facts.model.clone(),
        o::NativeVideoObject::Video,
        facts.progress,
        seconds.into(),
        facts.size,
        status,
    )
    .build();
    body.error = error;
    body.completed_at = facts.completed_at.map(Some);
    body.expires_at = facts.expires_at.map(Some);
    body.prompt = facts.prompt.clone().map(Some);
    Ok(Converted {
        value: NativeVeoResult { body, videos },
        report,
    })
}
#[derive(Debug, Clone, PartialEq)]
pub struct NativeToVeoContext {
    pub source_id: String,
    pub operation_name: String,
    /// Actual content obtained through the native download capability when the
    /// source completed. A native ID is never reinterpreted as a media URL.
    pub video: Option<g::Video>,
}
pub fn native_to_gemini_operation(
    input: o::NativeVideo,
    context: NativeToVeoContext,
) -> Result<Converted<g::VideoOperation>, TransformError> {
    let input = input.into_declared();
    super::response::operation_name(&context.operation_name)?;
    if input.id != context.source_id || input.id.is_empty() {
        return Err(TransformError::shape(
            "video.binding",
            "native ID differs from recorded source",
        ));
    }
    if input.created_at < 0
        || !(0..=100).contains(&input.progress)
        || !matches!(input.seconds.as_str(), "4" | "8" | "12")
        || input.model.is_empty()
    {
        return Err(TransformError::invalid_result(
            "native.video",
            "invalid native metadata",
        ));
    }
    let mut out = g::VideoOperation::builder()
        .name(context.operation_name)
        .build();
    let mut report = Report::default();
    match input.status {
        o::NativeVideoStatus::Queued | o::NativeVideoStatus::InProgress => {
            if input.error.flatten().is_some()
                || input.completed_at.flatten().is_some()
                || context.video.is_some()
            {
                return Err(TransformError::invalid_result(
                    "native.status",
                    "pending video carries terminal fields",
                ));
            }
            out.done = Some(false);
        }
        o::NativeVideoStatus::Failed => {
            if context.video.is_some() {
                return Err(TransformError::invalid_result(
                    "native.status",
                    "failed video has successful content",
                ));
            }
            let error = input
                .error
                .flatten()
                .ok_or_else(|| TransformError::missing_metadata("native.error"))?;
            if error.message.is_empty() || error.code.is_empty() {
                return Err(TransformError::invalid_result(
                    "native.error",
                    "nonempty error code/message required",
                ));
            }
            out.done = Some(true);
            out.error = Some(
                g::GoogleRpcStatus::builder()
                    .message(format!("{}: {}", error.code, error.message))
                    .build(),
            );
            if error.misalignment.is_some() {
                report.omitted(
                    "error.misalignment",
                    "Veo RPC status has no declared native safety explanation shape",
                );
            }
        }
        o::NativeVideoStatus::Completed => {
            if input.error.flatten().is_some() || input.progress != 100 {
                return Err(TransformError::invalid_result(
                    "native.status",
                    "completed video has error or nonfinal progress",
                ));
            }
            let video = context
                .video
                .map(DeclaredFields::into_declared)
                .ok_or_else(|| TransformError::missing_metadata("native.downloaded_video"))?;
            match (&video.uri, &video.encoded_video) {
                (Some(uri), None) => {
                    super::resources::url(uri)?;
                }
                (None, Some(encoded))
                    if !encoded.is_empty()
                        && video.encoding.as_ref().is_some_and(|v| !v.is_empty()) => {}
                _ => {
                    return Err(TransformError::shape(
                        "native.downloaded_video",
                        "exactly one usable URI or bytes with MIME required",
                    ));
                }
            };
            out.done = Some(true);
            out.response = Some(
                g::VideoOperationResponse::builder()
                    .generate_video_response(
                        g::GenerateVideoResponse::builder()
                            .generated_samples(vec![
                                g::GeneratedVideoSample::builder().video(video).build(),
                            ])
                            .build(),
                    )
                    .build(),
            );
        }
    }
    report.omitted(
        "native.video_metadata",
        "Veo operation has no native created_at/progress/duration/size/model fields",
    );
    if input.expires_at.is_some() {
        report.omitted("expires_at", "Veo operation has no expiry field");
    }
    if input.prompt.is_some() {
        report.omitted("prompt", "Veo operation does not echo the prompt");
    }
    if input.remixed_from_video_id.is_some() {
        report.omitted(
            "remixed_from_video_id",
            "Veo operation has no remix ancestry field",
        );
    }
    Ok(Converted { value: out, report })
}
