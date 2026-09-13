use crate::{
    transform::TransformError,
    wire::openai::chat::{response as r, stream as s},
};
pub(super) fn append(
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
    validate(
        source.logprob,
        source.bytes.as_ref().and_then(Option::as_ref),
    )?;
    let mut top = Vec::new();
    for value in source.top_logprobs {
        validate(value.logprob, value.bytes.as_ref().and_then(Option::as_ref))?;
        top.push(
            r::TokenLogprobTop::builder(value.token, value.bytes.flatten(), value.logprob).build(),
        );
    }
    Ok(r::TokenLogprob::builder(source.token, source.bytes.flatten(), source.logprob, top).build())
}
fn validate(logprob: f64, bytes: Option<&Vec<i64>>) -> Result<(), TransformError> {
    if !logprob.is_finite() || bytes.is_some_and(|b| b.iter().any(|v| !(0..=255).contains(v))) {
        return Err(TransformError::invalid_result(
            "logprobs",
            "invalid probability or token bytes",
        ));
    }
    Ok(())
}
pub(super) fn synthesize(input: r::Logprobs) -> Result<s::ChunkLogprobs, TransformError> {
    let content = input
        .content
        .map(|v| {
            v.into_iter()
                .map(synthesize_token)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    let refusal = input
        .refusal
        .map(|v| {
            v.into_iter()
                .map(synthesize_token)
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?;
    let mut out = s::ChunkLogprobs::builder().build();
    out.content = content.map(Some);
    out.refusal = refusal.map(Some);
    Ok(out)
}
fn synthesize_token(input: r::TokenLogprob) -> Result<s::ChunkTokenLogprob, TransformError> {
    validate(input.logprob, input.bytes.as_ref())?;
    let mut top = Vec::new();
    for v in input.top_logprobs {
        validate(v.logprob, v.bytes.as_ref())?;
        let mut item = s::ChunkTopLogprob::builder(v.token, v.logprob).build();
        item.bytes = Some(v.bytes);
        top.push(item);
    }
    let mut out = s::ChunkTokenLogprob::builder(input.token, input.logprob, top).build();
    out.bytes = Some(input.bytes);
    Ok(out)
}
