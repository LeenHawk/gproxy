use crate::{transform::TransformError, wire::gemini as g};

pub(super) fn usage(
    old: &mut g::UsageMetadata,
    new: g::UsageMetadata,
) -> Result<(), TransformError> {
    counter(
        &mut old.prompt_token_count,
        new.prompt_token_count,
        "prompt_token_count",
    )?;
    counter(
        &mut old.cached_content_token_count,
        new.cached_content_token_count,
        "cached_content_token_count",
    )?;
    counter(
        &mut old.candidates_token_count,
        new.candidates_token_count,
        "candidates_token_count",
    )?;
    counter(
        &mut old.tool_use_prompt_token_count,
        new.tool_use_prompt_token_count,
        "tool_use_prompt_token_count",
    )?;
    counter(
        &mut old.thoughts_token_count,
        new.thoughts_token_count,
        "thoughts_token_count",
    )?;
    counter(
        &mut old.total_token_count,
        new.total_token_count,
        "total_token_count",
    )?;
    if new.prompt_tokens_details.is_some() {
        old.prompt_tokens_details = new.prompt_tokens_details;
    }
    if new.cache_tokens_details.is_some() {
        old.cache_tokens_details = new.cache_tokens_details;
    }
    if new.candidates_tokens_details.is_some() {
        old.candidates_tokens_details = new.candidates_tokens_details;
    }
    if new.tool_use_prompt_tokens_details.is_some() {
        old.tool_use_prompt_tokens_details = new.tool_use_prompt_tokens_details;
    }
    if new.service_tier.is_some() {
        old.service_tier = new.service_tier;
    }
    Ok(())
}

fn counter(old: &mut Option<i64>, new: Option<i64>, field: &str) -> Result<(), TransformError> {
    if let Some(value) = new {
        if value < 0 || old.is_some_and(|n| value < n) {
            return Err(TransformError::invalid_result(
                format!("usage.{field}"),
                "negative or decreasing cumulative token count",
            ));
        }
        *old = Some(value);
    }
    Ok(())
}

pub(super) fn candidate(old: &mut g::Candidate, new: g::Candidate) -> Result<(), TransformError> {
    if super::collector::terminal(old.finish_reason)
        && new.finish_reason.is_some()
        && new.finish_reason != old.finish_reason
    {
        return Err(TransformError::invalid_result(
            "finish_reason",
            "candidate terminal reason changed",
        ));
    }
    if new.finish_reason.is_some() {
        old.finish_reason = new.finish_reason;
    }
    if new.safety_ratings.is_some() {
        old.safety_ratings = new.safety_ratings;
    }
    if new.citation_metadata.is_some() {
        old.citation_metadata = new.citation_metadata;
    }
    if new.token_count.is_some() {
        old.token_count = new.token_count;
    }
    if new.grounding_attributions.is_some() {
        old.grounding_attributions = new.grounding_attributions;
    }
    if new.grounding_metadata.is_some() {
        old.grounding_metadata = new.grounding_metadata;
    }
    if new.avg_logprobs.is_some() {
        old.avg_logprobs = new.avg_logprobs;
    }
    if let Some(new) = new.logprobs_result {
        let old = old
            .logprobs_result
            .get_or_insert_with(|| g::LogprobsResult::builder().build());
        // Native metadata snapshots may arrive with only one field present.
        // Missing fields do not erase earlier declared metadata.
        if new.top_candidates.is_some() {
            old.top_candidates = new.top_candidates;
        }
        if new.chosen_candidates.is_some() {
            old.chosen_candidates = new.chosen_candidates;
        }
        if new.log_probability_sum.is_some() {
            old.log_probability_sum = new.log_probability_sum;
        }
    }
    if new.url_context_metadata.is_some() {
        old.url_context_metadata = new.url_context_metadata;
    }
    if new.finish_message.is_some() {
        old.finish_message = new.finish_message;
    }
    Ok(())
}
