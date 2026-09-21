use crate::{
    transform::{Report, TransformError},
    wire::{gemini as g, openai::chat as c},
};

fn valid(value: f64) -> Result<f64, TransformError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(TransformError::invalid_result(
            "logprobs",
            "non-finite probability",
        ))
    }
}

fn fields(value: g::LogprobCandidate) -> Result<(String, f64), TransformError> {
    Ok((
        value
            .token
            .ok_or_else(|| TransformError::missing_metadata("logprobs.token"))?,
        valid(
            value
                .log_probability
                .ok_or_else(|| TransformError::missing_metadata("logprobs.log_probability"))?,
        )?,
    ))
}

pub(super) fn to_chat(
    value: g::LogprobsResult,
    report: &mut Report,
) -> Result<c::Logprobs, TransformError> {
    let chosen = value
        .chosen_candidates
        .ok_or_else(|| TransformError::missing_metadata("logprobs.chosen_candidates"))?;
    let top = value.top_candidates.unwrap_or_default();
    if !top.is_empty() && top.len() != chosen.len() {
        return Err(TransformError::invalid_result(
            "logprobs",
            "top/chosen lengths disagree",
        ));
    }
    let mut top = top.into_iter();
    let mut tokens = Vec::new();
    for chosen in chosen {
        let (token, logprob) = fields(chosen)?;
        let alternatives = top
            .next()
            .and_then(|v| v.candidates)
            .unwrap_or_default()
            .into_iter()
            .map(|v| {
                let (token, logprob) = fields(v)?;
                Ok(c::TokenLogprobTop {
                    token,
                    logprob,
                    bytes: None,
                    rest: Default::default(),
                })
            })
            .collect::<Result<Vec<_>, TransformError>>()?;
        tokens.push(c::TokenLogprob {
            token,
            logprob,
            bytes: None,
            top_logprobs: alternatives,
            rest: Default::default(),
        });
    }
    if value.log_probability_sum.is_some() {
        report.omitted(
            "logprobs.log_probability_sum",
            "Chat has per-token log probabilities only",
        );
    }
    Ok(c::Logprobs {
        content: Some(tokens),
        refusal: None,
        rest: Default::default(),
    })
}

pub(super) fn to_gemini(
    value: &c::Logprobs,
    report: &mut Report,
) -> Result<g::LogprobsResult, TransformError> {
    let mut chosen = Vec::new();
    let mut top = Vec::new();
    for token in value.content.iter().flatten() {
        chosen.push(
            g::LogprobCandidate::builder()
                .token(token.token.clone())
                .log_probability(valid(token.logprob)?)
                .build(),
        );
        let mut candidates = Vec::new();
        for token in &token.top_logprobs {
            candidates.push(
                g::LogprobCandidate::builder()
                    .token(token.token.clone())
                    .log_probability(valid(token.logprob)?)
                    .build(),
            );
        }
        top.push(g::TopCandidates::builder().candidates(candidates).build());
    }
    if value.refusal.is_some() {
        report.omitted(
            "logprobs.refusal",
            "Gemini has no separate refusal logprob channel",
        );
    }
    let mut out = g::LogprobsResult::builder().build();
    out.chosen_candidates = Some(chosen);
    out.top_candidates = Some(top);
    Ok(out)
}
