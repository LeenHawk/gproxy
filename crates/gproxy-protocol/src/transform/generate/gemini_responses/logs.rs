use crate::{
    transform::{Report, TransformError},
    wire::{gemini as g, openai::responses::input as r},
};

pub(super) fn to_gemini(input: Vec<r::OutputLogprob>) -> Result<g::LogprobsResult, TransformError> {
    let mut chosen = Vec::new();
    let mut top = Vec::new();
    for token in input {
        let probability = token
            .logprob
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or_else(|| TransformError::invalid_result("logprob", "invalid double"))?;
        chosen.push(
            g::LogprobCandidate::builder()
                .token(token.token)
                .log_probability(probability)
                .build(),
        );
        let mut candidates = Vec::new();
        for token in token.top_logprobs {
            let probability = token
                .logprob
                .as_f64()
                .filter(|v| v.is_finite())
                .ok_or_else(|| TransformError::invalid_result("logprob", "invalid double"))?;
            candidates.push(
                g::LogprobCandidate::builder()
                    .token(token.token)
                    .log_probability(probability)
                    .build(),
            );
        }
        top.push(g::TopCandidates::builder().candidates(candidates).build());
    }
    Ok(g::LogprobsResult::builder()
        .chosen_candidates(chosen)
        .top_candidates(top)
        .build())
}

pub(super) fn citations(
    input: Vec<r::OutputAnnotation>,
    report: &mut Report,
) -> Result<Vec<g::CitationSource>, TransformError> {
    let mut out = Vec::new();
    for citation in input {
        match citation {
            r::OutputAnnotation::Url(value) => {
                if value.start_index < 0 || value.end_index < value.start_index {
                    return Err(TransformError::invalid_result("citation", "invalid bounds"));
                }
                out.push(
                    g::CitationSource::builder()
                        .start_index(value.start_index)
                        .end_index(value.end_index)
                        .uri(value.url)
                        .build(),
                );
                report.omitted("citation.title", "Gemini citation lacks title");
            }
            r::OutputAnnotation::File(_)
            | r::OutputAnnotation::ContainerFile(_)
            | r::OutputAnnotation::Path(_) => report.omitted(
                "citation.file",
                "Gemini lacks Responses file citation identity",
            ),
        }
    }
    Ok(out)
}
