use crate::{
    transform::{Report, TransformError},
    wire::openai::chat::{response as r, stream as s},
};

pub(crate) fn collect(input: s::ChunkUsage, report: &mut Report) -> r::Usage {
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
pub(crate) fn synthesize(input: r::Usage) -> Result<s::ChunkUsage, TransformError> {
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

    Ok(out)
}
