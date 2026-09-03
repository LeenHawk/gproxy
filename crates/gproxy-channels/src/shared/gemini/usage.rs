use gproxy_channel_api::{NormalizedUsage, UsageCtx};
use gproxy_protocol::Operation;
use gproxy_protocol::gemini::{self, Modality, ModalityKnown, UsageMetadata};
use rust_decimal::Decimal;
use std::collections::BTreeSet;

pub(crate) fn from_body(ctx: UsageCtx<'_>) -> Option<NormalizedUsage> {
    match ctx.key.operation() {
        Operation::GenerateContent | Operation::StreamGenerateContent => {
            let response =
                serde_json::from_slice::<gemini::GenerateContentResponse>(ctx.response_body)
                    .ok()?;
            let mut usage = normalize(response.usage_metadata.as_ref()?).ok()?;
            apply_response_tier(&mut usage, ctx.response_headers);
            Some(usage)
        }
        Operation::CreateEmbedding => {
            let response =
                serde_json::from_slice::<gemini::EmbedContentResponse>(ctx.response_body).ok()?;
            embedding(response.usage_metadata.as_ref()?)
        }
        Operation::BatchCreateEmbedding => {
            let response =
                serde_json::from_slice::<gemini::BatchEmbedContentsResponse>(ctx.response_body)
                    .ok()?;
            embedding(response.usage_metadata.as_ref()?)
        }
        Operation::CreateImage => {
            let response =
                serde_json::from_slice::<gemini::ImagenPredictResponse>(ctx.response_body).ok()?;
            image_outputs(response.predictions.len())
        }
        Operation::RetrieveVideo => {
            let operation =
                serde_json::from_slice::<gemini::VeoOperation>(ctx.response_body).ok()?;
            video_outputs(operation.response.as_ref()?)
        }
        _ => None,
    }
}

fn video_outputs(response: &serde_json::Value) -> Option<NormalizedUsage> {
    let outputs = [
        "/generateVideoResponse/generatedSamples",
        "/generateVideoResponse/generatedVideos",
        "/generatedVideos",
    ]
    .into_iter()
    .filter_map(|pointer| response.pointer(pointer)?.as_array())
    .flatten()
    .filter_map(|video| video.pointer("/video/uri")?.as_str())
    .collect::<BTreeSet<_>>();
    let count = u64::try_from(outputs.len()).ok()?;
    (count > 0).then(|| {
        let mut usage = NormalizedUsage::default();
        usage
            .metrics
            .insert("video_outputs".into(), Decimal::from(count));
        usage
    })
}

pub(crate) fn normalize(metadata: &UsageMetadata) -> Result<NormalizedUsage, String> {
    let prompt = required(metadata.prompt_token_count, "promptTokenCount")?;
    let candidates = required(metadata.candidates_token_count, "candidatesTokenCount")?;
    let cached = optional(
        metadata.cached_content_token_count,
        "cachedContentTokenCount",
    )?;
    if cached > prompt {
        return Err("cachedContentTokenCount exceeds promptTokenCount".into());
    }
    let thoughts = optional(metadata.thoughts_token_count, "thoughtsTokenCount")?;
    validate_details(&metadata.candidates_tokens_details)?;
    let image = modality_tokens(&metadata.candidates_tokens_details, ModalityKnown::Image)?;
    let audio = modality_tokens(&metadata.candidates_tokens_details, ModalityKnown::Audio)?;
    let media = image
        .checked_add(audio)
        .ok_or_else(|| "candidate modality token count overflow".to_owned())?;
    let ordinary = candidates
        .checked_sub(media)
        .ok_or_else(|| "candidate modality token counts exceed candidatesTokenCount".to_owned())?;
    let output = ordinary
        .checked_add(thoughts)
        .ok_or_else(|| "output token count overflow".to_owned())?;
    if let Some(total) = metadata.total_token_count {
        let total = nonnegative(total, "totalTokenCount")?;
        let declared = prompt
            .checked_add(candidates)
            .and_then(|value| value.checked_add(thoughts))
            .ok_or_else(|| "total token count overflow".to_owned())?;
        if total != declared {
            return Err(
                "totalTokenCount disagrees with prompt, candidate, and thought counts".into(),
            );
        }
    }
    validate_details(&metadata.prompt_tokens_details)?;
    validate_details(&metadata.cache_tokens_details)?;
    validate_details(&metadata.tool_use_prompt_tokens_details)?;
    let mut usage = NormalizedUsage {
        input_tokens: prompt,
        output_tokens: output,
        cached_input_tokens: cached,
        ..Default::default()
    };
    add_metric(&mut usage, "reasoning_tokens", thoughts);
    add_metric(&mut usage, "image_output_tokens", image);
    add_metric(&mut usage, "audio_output_tokens", audio);
    if let Some(tier) = metadata.service_tier.as_ref().and_then(tier_name) {
        usage.dimensions.insert("service_tier".into(), tier);
    }
    Ok(usage)
}

fn embedding(metadata: &gemini::EmbeddingUsageMetadata) -> Option<NormalizedUsage> {
    let input_tokens = nonnegative(metadata.prompt_token_count?, "promptTokenCount").ok()?;
    validate_details(&metadata.prompt_token_details).ok()?;
    Some(NormalizedUsage {
        input_tokens,
        ..Default::default()
    })
}

fn image_outputs(count: usize) -> Option<NormalizedUsage> {
    let count = u64::try_from(count).ok()?;
    (count > 0).then(|| {
        let mut usage = NormalizedUsage::default();
        usage
            .metrics
            .insert("image_outputs".into(), Decimal::from(count));
        usage
    })
}

fn modality_tokens(
    details: &[gemini::ModalityTokenCount],
    modality: ModalityKnown,
) -> Result<u64, String> {
    details
        .iter()
        .filter(|detail| detail.modality.as_ref() == Some(&Modality::Known(modality.clone())))
        .try_fold(0_u64, |total, detail| {
            let count = required_i64(detail.token_count, "modality tokenCount")?;
            total
                .checked_add(count)
                .ok_or_else(|| "modality token count overflow".into())
        })
}

fn validate_details(details: &[gemini::ModalityTokenCount]) -> Result<(), String> {
    for detail in details {
        if let Some(count) = detail.token_count {
            required_i64(Some(count), "modality tokenCount")?;
        }
    }
    Ok(())
}

pub(super) fn tier_name(tier: &gemini::ServiceTier) -> Option<String> {
    serde_json::to_value(tier).ok()?.as_str().map(str::to_owned)
}

pub(super) fn response_tier(headers: &http::HeaderMap) -> Option<String> {
    headers
        .get("x-gemini-service-tier")
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

pub(super) fn apply_response_tier(usage: &mut NormalizedUsage, headers: &http::HeaderMap) {
    if let Some(tier) = response_tier(headers) {
        usage.dimensions.insert("service_tier".into(), tier);
    }
}

fn add_metric(usage: &mut NormalizedUsage, name: &str, value: u64) {
    if value > 0 {
        usage.metrics.insert(name.into(), Decimal::from(value));
    }
}

fn required(value: Option<i32>, field: &str) -> Result<u64, String> {
    nonnegative(value.ok_or_else(|| format!("{field} is missing"))?, field)
}

fn optional(value: Option<i32>, field: &str) -> Result<u64, String> {
    value.map_or(Ok(0), |value| nonnegative(value, field))
}

fn nonnegative(value: i32, field: &str) -> Result<u64, String> {
    u64::try_from(value).map_err(|_| format!("{field} must be nonnegative"))
}

fn required_i64(value: Option<i64>, field: &str) -> Result<u64, String> {
    u64::try_from(value.ok_or_else(|| format!("{field} is missing"))?)
        .map_err(|_| format!("{field} must be nonnegative"))
}
