use crate::{
    transform::{Report, TransformError},
    wire::openai::{chat::response as c, responses::input as r},
};

pub(super) fn to_responses(
    input: Option<Vec<c::Annotation>>,
) -> Result<Vec<r::OutputAnnotation>, TransformError> {
    input
        .unwrap_or_default()
        .into_iter()
        .map(|item| {
            if item.url_citation.start_index < 0
                || item.url_citation.end_index < item.url_citation.start_index
            {
                return Err(TransformError::invalid_result(
                    "annotations",
                    "invalid citation range",
                ));
            }
            Ok(r::OutputAnnotation::Url(r::UrlCitation {
                start_index: item.url_citation.start_index,
                end_index: item.url_citation.end_index,
                title: item.url_citation.title,
                url: item.url_citation.url,
                type_: r::UrlCitationType::UrlCitation,
                rest: Default::default(),
            }))
        })
        .collect()
}
pub(super) fn to_chat(
    input: Vec<r::OutputAnnotation>,
    offset: i64,
    report: &mut Report,
) -> Result<Vec<c::Annotation>, TransformError> {
    let mut result = Vec::new();
    for item in input {
        match item {
            r::OutputAnnotation::Url(item) => {
                let start = item.start_index.checked_add(offset).ok_or_else(|| {
                    TransformError::invalid_result("annotations", "index overflow")
                })?;
                let end = item.end_index.checked_add(offset).ok_or_else(|| {
                    TransformError::invalid_result("annotations", "index overflow")
                })?;
                if item.start_index < 0 || item.end_index < item.start_index {
                    return Err(TransformError::invalid_result(
                        "annotations",
                        "invalid citation range",
                    ));
                }
                result.push(c::Annotation {
                    type_: c::AnnotationType::UrlCitation,
                    url_citation: c::UrlCitation {
                        start_index: start,
                        end_index: end,
                        title: item.title,
                        url: item.url,
                        rest: Default::default(),
                    },
                    rest: Default::default(),
                });
            }
            r::OutputAnnotation::File(_)
            | r::OutputAnnotation::ContainerFile(_)
            | r::OutputAnnotation::Path(_) => report.omitted(
                "output.content.annotations",
                "Chat annotations cannot represent file citations",
            ),
        }
    }
    Ok(result)
}
fn number(value: f64) -> Result<serde_json::Number, TransformError> {
    serde_json::Number::from_f64(value)
        .ok_or_else(|| TransformError::invalid_result("logprobs", "non-finite log probability"))
}
pub(super) fn logs_to_responses(
    input: Option<Vec<c::TokenLogprob>>,
    report: &mut Report,
) -> Result<Vec<r::OutputLogprob>, TransformError> {
    let Some(input) = input else {
        return Ok(Vec::new());
    };
    if input
        .iter()
        .any(|p| p.bytes.is_none() || p.top_logprobs.iter().any(|q| q.bytes.is_none()))
    {
        report.omitted(
            "logprobs.content",
            "Responses requires token bytes absent from this Chat logprob record",
        );
        return Ok(Vec::new());
    }
    input
        .into_iter()
        .map(|p| {
            Ok(r::OutputLogprob {
                token: p.token,
                bytes: p.bytes.expect("checked"),
                logprob: number(p.logprob)?,
                top_logprobs: p
                    .top_logprobs
                    .into_iter()
                    .map(|q| {
                        Ok(r::TopLogprob {
                            token: q.token,
                            bytes: q.bytes.expect("checked"),
                            logprob: number(q.logprob)?,
                            rest: Default::default(),
                        })
                    })
                    .collect::<Result<_, TransformError>>()?,
                rest: Default::default(),
            })
        })
        .collect()
}
fn float(value: serde_json::Number) -> Result<f64, TransformError> {
    value.as_f64().filter(|n| n.is_finite()).ok_or_else(|| {
        TransformError::invalid_result("logprobs", "log probability exceeds target numeric range")
    })
}
pub(super) fn logs_to_chat(
    input: Vec<r::OutputLogprob>,
) -> Result<Vec<c::TokenLogprob>, TransformError> {
    input
        .into_iter()
        .map(|p| {
            Ok(c::TokenLogprob {
                token: p.token,
                bytes: Some(p.bytes),
                logprob: float(p.logprob)?,
                top_logprobs: p
                    .top_logprobs
                    .into_iter()
                    .map(|q| {
                        Ok(c::TokenLogprobTop {
                            token: q.token,
                            bytes: Some(q.bytes),
                            logprob: float(q.logprob)?,
                            rest: Default::default(),
                        })
                    })
                    .collect::<Result<_, TransformError>>()?,
                rest: Default::default(),
            })
        })
        .collect()
}
