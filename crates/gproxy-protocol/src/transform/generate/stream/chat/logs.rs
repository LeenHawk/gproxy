use crate::{
    transform::TransformError,
    wire::openai::chat::{response as r, stream as s},
};
pub(crate) fn append(
    old: &mut Option<r::Logprobs>,
    source: s::ChunkLogprobs,
) -> Result<(), TransformError> {
    let old = old.get_or_insert_with(|| r::Logprobs::builder(None, None).build());
    if let Some(Some(content)) = source.content {
        old.content.get_or_insert_with(Vec::new).extend(
            content
                .into_iter()
                .map(collect)
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    if let Some(Some(refusal)) = source.refusal {
        old.refusal.get_or_insert_with(Vec::new).extend(
            refusal
                .into_iter()
                .map(collect)
                .collect::<Result<Vec<_>, _>>()?,
        );
    }
    Ok(())
}
fn collect(source: s::ChunkTokenLogprob) -> Result<r::TokenLogprob, TransformError> {
    let mut top = Vec::new();
    for value in source.top_logprobs {
        top.push(
            r::TokenLogprobTop::builder(value.token, value.bytes.flatten(), value.logprob).build(),
        );
    }
    Ok(r::TokenLogprob::builder(source.token, source.bytes.flatten(), source.logprob, top).build())
}

pub(crate) fn synthesize(input: r::Logprobs) -> Result<s::ChunkLogprobs, TransformError> {
    let content = input
        .content
        .map(|v| {
            v.into_iter()
                .map(synthesize_token)
                .collect::<Result<Vec<_>, _>>()
        })
        .map(crate::transform::optional)
        .transpose()?
        .flatten();
    let refusal = input
        .refusal
        .map(|v| {
            v.into_iter()
                .map(synthesize_token)
                .collect::<Result<Vec<_>, _>>()
        })
        .map(crate::transform::optional)
        .transpose()?
        .flatten();
    let mut out = s::ChunkLogprobs::builder().build();
    out.content = content.map(Some);
    out.refusal = refusal.map(Some);
    Ok(out)
}
fn synthesize_token(input: r::TokenLogprob) -> Result<s::ChunkTokenLogprob, TransformError> {
    let mut top = Vec::new();
    for v in input.top_logprobs {
        let mut item = s::ChunkTopLogprob::builder(v.token, v.logprob).build();
        item.bytes = Some(v.bytes);
        top.push(item);
    }
    let mut out = s::ChunkTokenLogprob::builder(input.token, input.logprob, top).build();
    out.bytes = Some(input.bytes);
    Ok(out)
}
