use crate::{
    transform::{Report, TransformError},
    wire::openai::{chat::response as c, responses::response as r},
};

pub(crate) fn to_responses(
    value: c::ModerationResponse,
    report: &mut Report,
) -> Result<r::ResponseModerationReport, TransformError> {
    Ok(r::ResponseModerationReport {
        input: outcome_to_responses(value.input, report)?,
        output: outcome_to_responses(value.output, report)?,
        rest: Default::default(),
    })
}

fn outcome_to_responses(
    value: c::ModerationResultOrError,
    report: &mut Report,
) -> Result<r::ResponseModerationOutcome, TransformError> {
    Ok(match value {
        c::ModerationResultOrError::Error(value) => {
            r::ResponseModerationOutcome::Error(r::ResponseModerationError {
                code: value.code,
                message: value.message,
                type_: r::ResponseModerationErrorType::Error,
                rest: Default::default(),
            })
        }
        c::ModerationResultOrError::Result(value) => {
            if value.results.len() != 1 {
                return Err(TransformError::unsupported(
                    "moderation.results",
                    "Responses has one moderation result per input/output, not a batch",
                ));
            }
            let item = value.results.into_iter().next().expect("one result");
            if value.model != item.model {
                report.omitted(
                    "moderation.model",
                    "Responses retains per-result model; Chat batch model differs",
                );
            }
            r::ResponseModerationOutcome::Result(r::ResponseModerationResult {
                categories: item.categories,
                category_applied_input_types: item
                    .category_applied_input_types
                    .into_iter()
                    .map(|(k, v)| {
                        (
                            k,
                            v.into_iter()
                                .map(|v| match v {
                                    c::ModerationInputType::Text => {
                                        r::ResponseModerationInputType::Text
                                    }
                                    c::ModerationInputType::Image => {
                                        r::ResponseModerationInputType::Image
                                    }
                                })
                                .collect(),
                        )
                    })
                    .collect(),
                category_scores: item
                    .category_scores
                    .into_iter()
                    .map(|(k, v)| {
                        Ok((
                            k,
                            serde_json::Number::from_f64(v).ok_or_else(|| {
                                TransformError::invalid_result(
                                    "moderation.category_scores",
                                    "non-finite score",
                                )
                            })?,
                        ))
                    })
                    .collect::<Result<_, TransformError>>()?,
                flagged: item.flagged,
                model: item.model,
                type_: r::ResponseModerationResultType::ModerationResult,
                rest: Default::default(),
            })
        }
    })
}

pub(crate) fn to_chat(
    value: r::ResponseModerationReport,
) -> Result<c::ModerationResponse, TransformError> {
    Ok(c::ModerationResponse {
        input: outcome_to_chat(value.input)?,
        output: outcome_to_chat(value.output)?,
        rest: Default::default(),
    })
}

fn outcome_to_chat(
    value: r::ResponseModerationOutcome,
) -> Result<c::ModerationResultOrError, TransformError> {
    Ok(match value {
        r::ResponseModerationOutcome::Error(value) => {
            c::ModerationResultOrError::Error(c::ModerationError {
                code: value.code,
                message: value.message,
                type_: c::ModerationErrorType::Error,
                rest: Default::default(),
            })
        }
        r::ResponseModerationOutcome::Result(value) => {
            c::ModerationResultOrError::Result(c::ModerationResult {
                model: value.model.clone(),
                type_: c::ModerationResultType::ModerationResults,
                results: vec![c::ModerationResultItem {
                    categories: value.categories,
                    category_applied_input_types: value
                        .category_applied_input_types
                        .into_iter()
                        .map(|(k, v)| {
                            (
                                k,
                                v.into_iter()
                                    .map(|v| match v {
                                        r::ResponseModerationInputType::Text => {
                                            c::ModerationInputType::Text
                                        }
                                        r::ResponseModerationInputType::Image => {
                                            c::ModerationInputType::Image
                                        }
                                    })
                                    .collect(),
                            )
                        })
                        .collect(),
                    category_scores: value
                        .category_scores
                        .into_iter()
                        .map(|(k, v)| {
                            Ok((
                                k,
                                v.as_f64().filter(|n| n.is_finite()).ok_or_else(|| {
                                    TransformError::invalid_result(
                                        "moderation.category_scores",
                                        "score exceeds target numeric range",
                                    )
                                })?,
                            ))
                        })
                        .collect::<Result<_, TransformError>>()?,
                    flagged: value.flagged,
                    model: value.model,
                    type_: c::ModerationItemType::ModerationResult,
                    rest: Default::default(),
                }],
                rest: Default::default(),
            })
        }
    })
}
