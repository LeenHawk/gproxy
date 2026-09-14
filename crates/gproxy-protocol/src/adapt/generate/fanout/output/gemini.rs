use super::*;
pub(super) fn aggregate(
    mut values: Vec<g::GenerateContentResponseBody>,
    id: String,
    report: &mut Report,
) -> Result<g::GenerateContentResponseBody, TransformError> {
    if values.is_empty()
        || values
            .iter()
            .any(|v| v.candidates.as_ref().is_none_or(|v| v.len() != 1))
    {
        return Err(TransformError::invalid_result(
            "fanout.candidates",
            "each completed child must provide exactly one candidate; retained partial or blocked children require reconciliation",
        ));
    }
    let mut out = values.remove(0).into_declared();
    out.response_id = Some(id);
    out.candidates.as_mut().expect("validated")[0].index = Some(0);
    for (index, value) in values.into_iter().enumerate() {
        out.model_version = common(
            out.model_version,
            value.model_version,
            "modelVersion",
            report,
        );
        out.model_status = common(out.model_status, value.model_status, "modelStatus", report);
        out.prompt_feedback = common(
            out.prompt_feedback,
            value.prompt_feedback,
            "promptFeedback",
            report,
        );
        out.usage_metadata = usage(out.usage_metadata, value.usage_metadata, report)?;
        let mut candidate = value
            .candidates
            .expect("validated")
            .into_iter()
            .next()
            .expect("validated");
        candidate.index = Some(i64::try_from(index + 1).map_err(|_| limit())?);
        out.candidates.as_mut().expect("validated").push(candidate);
    }
    report.changed(
        "responseId,candidates,usageMetadata",
        "aggregate identity, ordered child candidates and sum of actual per-call charges",
    );
    Ok(out)
}
fn usage(
    a: Option<g::UsageMetadata>,
    b: Option<g::UsageMetadata>,
    r: &mut Report,
) -> Result<Option<g::UsageMetadata>, TransformError> {
    match (a, b) {
        (Some(mut a), Some(b)) => {
            a.prompt_token_count = optional(
                a.prompt_token_count,
                b.prompt_token_count,
                "usageMetadata.promptTokenCount",
                r,
            )?;
            a.cached_content_token_count = optional(
                a.cached_content_token_count,
                b.cached_content_token_count,
                "usageMetadata.cachedContentTokenCount",
                r,
            )?;
            a.candidates_token_count = optional(
                a.candidates_token_count,
                b.candidates_token_count,
                "usageMetadata.candidatesTokenCount",
                r,
            )?;
            a.tool_use_prompt_token_count = optional(
                a.tool_use_prompt_token_count,
                b.tool_use_prompt_token_count,
                "usageMetadata.toolUsePromptTokenCount",
                r,
            )?;
            a.thoughts_token_count = optional(
                a.thoughts_token_count,
                b.thoughts_token_count,
                "usageMetadata.thoughtsTokenCount",
                r,
            )?;
            a.total_token_count = optional(
                a.total_token_count,
                b.total_token_count,
                "usageMetadata.totalTokenCount",
                r,
            )?;
            a.prompt_tokens_details = details(
                a.prompt_tokens_details,
                b.prompt_tokens_details,
                "usageMetadata.promptTokensDetails",
                r,
            )?;
            a.cache_tokens_details = details(
                a.cache_tokens_details,
                b.cache_tokens_details,
                "usageMetadata.cacheTokensDetails",
                r,
            )?;
            a.candidates_tokens_details = details(
                a.candidates_tokens_details,
                b.candidates_tokens_details,
                "usageMetadata.candidatesTokensDetails",
                r,
            )?;
            a.tool_use_prompt_tokens_details = details(
                a.tool_use_prompt_tokens_details,
                b.tool_use_prompt_tokens_details,
                "usageMetadata.toolUsePromptTokensDetails",
                r,
            )?;
            a.service_tier = common(
                a.service_tier,
                b.service_tier,
                "usageMetadata.serviceTier",
                r,
            );
            Ok(Some(a))
        }
        (None, None) => Ok(None),
        _ => {
            r.omitted(
                "usageMetadata",
                "some child calls lack usage; aggregate counts are unknown",
            );
            Ok(None)
        }
    }
}
fn details(
    a: Option<Vec<g::ModalityTokenCount>>,
    b: Option<Vec<g::ModalityTokenCount>>,
    field: &str,
    r: &mut Report,
) -> Result<Option<Vec<g::ModalityTokenCount>>, TransformError> {
    let (Some(mut a), Some(b)) = (a, b) else {
        r.omitted(field, "one or more calls lack modality counts");
        return Ok(None);
    };
    if a.len() != b.len()
        || a.iter()
            .any(|x| b.iter().filter(|y| x.modality == y.modality).count() != 1)
        || b.iter()
            .any(|x| a.iter().filter(|y| x.modality == y.modality).count() != 1)
    {
        r.omitted(field, "modality coverage differs across calls");
        return Ok(None);
    }
    for x in &mut a {
        let y = b
            .iter()
            .find(|y| x.modality == y.modality)
            .expect("validated");
        x.token_count = optional(x.token_count, y.token_count, field, r)?;
    }
    Ok(Some(a))
}
