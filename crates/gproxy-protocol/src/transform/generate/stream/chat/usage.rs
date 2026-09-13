use crate::{
    transform::{Report, TransformError},
    wire::openai::chat::{response as r, stream as s},
};
pub(super) fn validate(v: &s::ChunkUsage) -> Result<(), TransformError> {
    if v.prompt_tokens < 0
        || v.completion_tokens < 0
        || v.prompt_tokens.checked_add(v.completion_tokens) != Some(v.total_tokens)
    {
        return Err(TransformError::invalid_result(
            "usage",
            "negative or inconsistent total",
        ));
    }
    if let Some(Some(d)) = &v.prompt_tokens_details {
        for value in [
            d.audio_tokens,
            d.cache_write_tokens,
            d.cached_tokens,
            d.image_tokens,
            d.text_tokens,
        ]
        .into_iter()
        .flatten()
        .flatten()
        {
            if value < 0 || value > v.prompt_tokens {
                return Err(TransformError::invalid_result(
                    "usage.prompt_tokens_details",
                    "detail outside prompt total",
                ));
            }
        }
        if d.cache_write_tokens
            .flatten()
            .zip(d.cached_tokens.flatten())
            .is_some_and(|(a, b)| {
                a.checked_add(b)
                    .is_none_or(|vtotal| vtotal > v.prompt_tokens)
            })
        {
            return Err(TransformError::invalid_result(
                "usage.cache",
                "cache exceeds prompt",
            ));
        }
    }
    if let Some(Some(d)) = &v.completion_tokens_details {
        for value in [
            d.audio_tokens,
            d.reasoning_tokens,
            d.accepted_prediction_tokens,
            d.text_tokens,
        ]
        .into_iter()
        .flatten()
        .flatten()
        {
            if value < 0 || value > v.completion_tokens {
                return Err(TransformError::invalid_result(
                    "usage.completion_tokens_details",
                    "detail outside completion total",
                ));
            }
        }
        if d.rejected_prediction_tokens
            .flatten()
            .is_some_and(|v| v < 0)
        {
            return Err(TransformError::invalid_result(
                "usage.rejected_prediction_tokens",
                "negative count",
            ));
        }
    }
    Ok(())
}
pub(super) fn collect(input: s::ChunkUsage, report: &mut Report) -> r::Usage {
    let mut out = r::Usage::builder(
        input.prompt_tokens,
        input.completion_tokens,
        input.total_tokens,
    )
    .build();
    out.prompt_tokens_details = input.prompt_tokens_details.flatten().map(|v| {
        if v.text_tokens.is_some() || v.image_tokens.is_some() {
            report.omitted(
                "usage.prompt_tokens_details.text/image",
                "buffered Chat has no corresponding usage detail",
            );
        }
        let mut out = r::PromptTokensDetails::builder().build();
        out.audio_tokens = v.audio_tokens.flatten();
        out.cache_write_tokens = v.cache_write_tokens.flatten();
        out.cached_tokens = v.cached_tokens.flatten();
        out
    });
    out.completion_tokens_details = input.completion_tokens_details.flatten().map(|v| {
        if v.text_tokens.is_some() {
            report.omitted(
                "usage.completion_tokens_details.text",
                "buffered Chat has no corresponding usage detail",
            );
        }
        let mut out = r::CompletionTokensDetails::builder().build();
        out.audio_tokens = v.audio_tokens.flatten();
        out.reasoning_tokens = v.reasoning_tokens.flatten();
        out.accepted_prediction_tokens = v.accepted_prediction_tokens.flatten();
        out.rejected_prediction_tokens = v.rejected_prediction_tokens.flatten();
        out
    });
    out
}
pub(super) fn synthesize(input: r::Usage) -> Result<s::ChunkUsage, TransformError> {
    let mut out = s::ChunkUsage::builder(
        input.completion_tokens,
        input.prompt_tokens,
        input.total_tokens,
    )
    .build();
    out.prompt_tokens_details = input.prompt_tokens_details.map(|v| {
        let mut out = s::ChunkPromptTokensDetails::builder().build();
        out.audio_tokens = v.audio_tokens.map(Some);
        out.cache_write_tokens = v.cache_write_tokens.map(Some);
        out.cached_tokens = v.cached_tokens.map(Some);
        Some(out)
    });
    out.completion_tokens_details = input.completion_tokens_details.map(|v| {
        let mut out = s::ChunkCompletionTokensDetails::builder().build();
        out.audio_tokens = v.audio_tokens.map(Some);
        out.reasoning_tokens = v.reasoning_tokens.map(Some);
        out.accepted_prediction_tokens = v.accepted_prediction_tokens.map(Some);
        out.rejected_prediction_tokens = v.rejected_prediction_tokens.map(Some);
        Some(out)
    });
    validate(&out)?;
    Ok(out)
}
