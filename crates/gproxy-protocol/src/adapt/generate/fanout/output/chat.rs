use super::*;
pub(super) fn aggregate(
    mut values: Vec<h::GenerateContentResponseBody>,
    id: String,
    report: &mut Report,
) -> Result<h::GenerateContentResponseBody, TransformError> {
    if values.is_empty() || values.iter().any(|v| v.choices.len() != 1) {
        return Err(TransformError::invalid_result(
            "fanout.choices",
            "each completed child must provide exactly one choice",
        ));
    }
    let mut out = values.remove(0).into_declared();
    out.id = id;
    out.choices[0].index = 0;
    for (index, value) in values.into_iter().enumerate() {
        if out.model != value.model {
            return Err(TransformError::invalid_result(
                "fanout.model",
                "child model identities differ",
            ));
        }
        // Creation time belongs to the first actual child; no invented clock.
        out.service_tier = common(out.service_tier, value.service_tier, "service_tier", report);
        out.system_fingerprint = common(
            out.system_fingerprint,
            value.system_fingerprint,
            "system_fingerprint",
            report,
        );
        out.moderation = common(out.moderation, value.moderation, "moderation", report);
        out.usage = usage(out.usage, value.usage, report)?;
        let mut choice = value.choices.into_iter().next().expect("validated");
        choice.index = i64::try_from(index + 1).map_err(|_| limit())?;
        out.choices.push(choice);
    }
    report.changed("id,choices,usage","aggregate identity, ordered child choices and sum of actual per-call charges; created is the first child's actual time");
    Ok(out)
}
fn usage(
    a: Option<h::Usage>,
    b: Option<h::Usage>,
    report: &mut Report,
) -> Result<Option<h::Usage>, TransformError> {
    match (a, b) {
        (Some(mut a), Some(b)) => {
            a.prompt_tokens = sum(a.prompt_tokens, b.prompt_tokens)?;
            a.completion_tokens = sum(a.completion_tokens, b.completion_tokens)?;
            a.total_tokens = sum(a.total_tokens, b.total_tokens)?;
            a.prompt_tokens_details =
                prompt(a.prompt_tokens_details, b.prompt_tokens_details, report)?;
            a.completion_tokens_details = completion(
                a.completion_tokens_details,
                b.completion_tokens_details,
                report,
            )?;
            Ok(Some(a))
        }
        (None, None) => Ok(None),
        _ => {
            report.omitted(
                "usage",
                "some child calls lack usage; aggregate counts are unknown",
            );
            Ok(None)
        }
    }
}
fn prompt(
    a: Option<h::PromptTokensDetails>,
    b: Option<h::PromptTokensDetails>,
    r: &mut Report,
) -> Result<Option<h::PromptTokensDetails>, TransformError> {
    match (a, b) {
        (Some(mut a), Some(b)) => {
            a.cache_write_tokens = optional(
                a.cache_write_tokens,
                b.cache_write_tokens,
                "usage.prompt.cache_write",
                r,
            )?;
            a.cached_tokens = optional(a.cached_tokens, b.cached_tokens, "usage.prompt.cached", r)?;
            a.audio_tokens = optional(a.audio_tokens, b.audio_tokens, "usage.prompt.audio", r)?;
            Ok(Some(a))
        }
        (None, None) => Ok(None),
        _ => {
            r.omitted(
                "usage.prompt_tokens_details",
                "some child calls lack details",
            );
            Ok(None)
        }
    }
}
fn completion(
    a: Option<h::CompletionTokensDetails>,
    b: Option<h::CompletionTokensDetails>,
    r: &mut Report,
) -> Result<Option<h::CompletionTokensDetails>, TransformError> {
    match (a, b) {
        (Some(mut a), Some(b)) => {
            a.accepted_prediction_tokens = optional(
                a.accepted_prediction_tokens,
                b.accepted_prediction_tokens,
                "usage.completion.accepted_prediction",
                r,
            )?;
            a.rejected_prediction_tokens = optional(
                a.rejected_prediction_tokens,
                b.rejected_prediction_tokens,
                "usage.completion.rejected_prediction",
                r,
            )?;
            a.reasoning_tokens = optional(
                a.reasoning_tokens,
                b.reasoning_tokens,
                "usage.completion.reasoning",
                r,
            )?;
            a.audio_tokens = optional(a.audio_tokens, b.audio_tokens, "usage.completion.audio", r)?;
            Ok(Some(a))
        }
        (None, None) => Ok(None),
        _ => {
            r.omitted(
                "usage.completion_tokens_details",
                "some child calls lack details",
            );
            Ok(None)
        }
    }
}
